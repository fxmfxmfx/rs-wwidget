//! Direct Wayland transport: one event loop, no GTK, renderer threads, or async runtime.
use cairo::{Context, Format, ImageSurface};
use rs_wwidget::{
    Result,
    config::{self, Config},
    layout::{self, Layout, Position, WIDTH},
    metrics::{self, Display, Sampler},
    plugin::Runtime,
    render::{self, Renderer},
};
use std::{
    fs::File,
    os::fd::{AsFd, AsRawFd, FromRawFd},
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_output, wl_pointer, wl_region, wl_registry,
        wl_seat, wl_shm, wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, Layer as ShellLayer},
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity},
};

static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn signal(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SurfaceId(u64);

struct Output {
    global: u32,
    proxy: wl_output::WlOutput,
    xdg: Option<zxdg_output_v1::ZxdgOutputV1>,
    name: String,
    description: String,
    pixels: (i32, i32),
    logical: Option<(i32, i32)>,
    scale: i32,
    rotated: bool,
}
impl Output {
    fn size(&self) -> (i32, i32) {
        self.logical.unwrap_or_else(|| {
            let (w, h) = self.pixels;
            if self.rotated {
                (h / self.scale, w / self.scale)
            } else {
                (w / self.scale, h / self.scale)
            }
        })
    }
}

/// Owned mmap storage. Cairo keeps this value alive until its last surface reference dies.
struct Mapping {
    ptr: NonNull<u8>,
    len: usize,
}
impl AsMut<[u8]> for Mapping {
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: mmap has `len` bytes, exclusively owned by this surface; no Rust aliases.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr().cast(), self.len);
        }
    }
}

struct Buffer {
    proxy: wl_buffer::WlBuffer,
    image: ImageSurface,
    busy: Arc<AtomicBool>,
    preview: Option<Position>,
    fresh: bool,
}
impl Buffer {
    fn new(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<App>,
        width: i32,
        height: i32,
        scale: i32,
    ) -> Result<Self> {
        let w = width.checked_mul(scale).ok_or("buffer width overflow")?;
        let h = height.checked_mul(scale).ok_or("buffer height overflow")?;
        let stride = Format::ARgb32.stride_for_width(w as u32)?;
        let len = stride
            .checked_mul(h)
            .filter(|n| *n > 0)
            .ok_or("buffer size overflow")?;
        // SAFETY: memfd_create returns a new owned fd or -1; mmap is checked before use.
        let fd = unsafe { libc::memfd_create(c"rs-wwidget".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        file.set_len(len as u64)?;
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error().into());
        }
        let mapping = Mapping {
            ptr: NonNull::new(ptr.cast()).ok_or("mmap returned null")?,
            len: len as usize,
        };
        let image = ImageSurface::create_for_data(mapping, Format::ARgb32, w, h, stride)?;
        image.set_device_scale(scale as f64, scale as f64);
        let pool = shm.create_pool(file.as_fd(), len, qh, ());
        let busy = Arc::new(AtomicBool::new(false));
        let proxy = pool.create_buffer(0, w, h, stride, wl_shm::Format::Argb8888, qh, busy.clone());
        pool.destroy();
        Ok(Self {
            proxy,
            image,
            busy,
            preview: None,
            fresh: true,
        })
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        self.proxy.destroy();
    }
}

struct Surface {
    id: SurfaceId,
    panel: Option<usize>,
    wl: wl_surface::WlSurface,
    layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    size: (i32, i32),
    scale: i32,
    configured: bool,
    dirty: bool,
    first_commit: bool,
    buffers: Vec<Buffer>,
}
impl Drop for Surface {
    fn drop(&mut self) {
        self.layer.destroy();
        self.wl.destroy();
    }
}

#[derive(Clone, Copy)]
struct Drag {
    panel: usize,
    initial: Position,
    preview: Position,
    start: (f64, f64),
    last_activity: Instant,
    source_hidden: bool,
}

#[derive(Clone, Copy)]
struct RetiringOverlay {
    id: SurfaceId,
    panel: usize,
}

struct App {
    config: Config,
    plugins: Vec<(usize, Runtime)>,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    output_manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    outputs: Vec<Output>,
    selected: Option<u32>,
    geometry: (i32, i32, i32),
    outputs_changed: bool,
    pointer: Option<wl_pointer::WlPointer>,
    focus: Option<SurfaceId>,
    pointer_pos: (f64, f64),
    surfaces: Vec<Surface>,
    next_id: u64,
    layout: Layout,
    renderer: Renderer,
    sampler: Sampler,
    display: Display,
    drag: Option<Drag>,
    retiring_overlay: Option<RetiringOverlay>,
    state_dir: std::path::PathBuf,
    started: Instant,
    ticks: u64,
    running: bool,
}

impl App {
    fn new(config: Config) -> Self {
        let plugins = config
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, p)| p.enabled)
            .map(|(i, p)| (i + 3, Runtime::new(p.clone())))
            .collect();
        Self {
            plugins,
            renderer: Renderer::new(config.colors, config.background_opacity)
                .with_font_family(&config.font_family)
                .with_language(config.language),
            config,
            compositor: None,
            shm: None,
            shell: None,
            output_manager: None,
            outputs: Vec::new(),
            selected: None,
            geometry: (0, 0, 1),
            outputs_changed: false,
            pointer: None,
            focus: None,
            pointer_pos: (0.0, 0.0),
            surfaces: Vec::with_capacity(4),
            next_id: 0,
            layout: Layout::defaults(1920, 1080),
            sampler: Sampler::default(),
            display: Display::default(),
            drag: None,
            retiring_overlay: None,
            state_dir: config::state_dir(),
            started: Instant::now(),
            ticks: 0,
            running: true,
        }
    }

    fn create_surface(&mut self, panel: Option<usize>, qh: &QueueHandle<Self>) {
        self.next_id += 1;
        let id = SurfaceId(self.next_id);
        let output = self
            .outputs
            .iter()
            .find(|o| Some(o.global) == self.selected)
            .unwrap();
        let compositor = self.compositor.as_ref().unwrap();
        let wl = compositor.create_surface(qh, id);
        let layer = self.shell.as_ref().unwrap().get_layer_surface(
            &wl,
            Some(&output.proxy),
            if panel.is_some() {
                ShellLayer::Bottom
            } else {
                ShellLayer::Top
            },
            "rs-wwidget".into(),
            qh,
            id,
        );
        layer.set_anchor(Anchor::Top | Anchor::Left);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        let size = panel.map_or((self.geometry.0, self.geometry.1), |i| {
            (WIDTH, self.config.panel_height(i))
        });
        layer.set_size(size.0 as u32, size.1 as u32);
        let pos = panel.map_or(Position::default(), |i| self.layout.position(i));
        layer.set_margin(pos.y, 0, 0, pos.x);
        let scale = self.geometry.2;
        wl.set_buffer_scale(scale);
        if panel.is_some() {
            let region = compositor.create_region(qh, ());
            region.add(0, 0, WIDTH, 42);
            wl.set_input_region(Some(&region));
            region.destroy();
        }
        wl.commit();
        self.surfaces.push(Surface {
            id,
            panel,
            wl,
            layer,
            size,
            scale,
            configured: false,
            dirty: true,
            first_commit: true,
            buffers: Vec::with_capacity(2),
        });
    }

    fn choose_output(&self) -> Option<u32> {
        let available = |o: &&Output| o.size().0 > 0 && o.size().1 > 0;
        if self.config.monitor != "auto" {
            self.outputs
                .iter()
                .filter(available)
                .find(|o| o.name == self.config.monitor)
                .map(|o| o.global)
        } else {
            self.outputs
                .iter()
                .filter(available)
                .min_by_key(|o| o.global)
                .map(|o| o.global)
        }
    }

    fn sync_outputs(&mut self, qh: &QueueHandle<Self>, initial: bool) -> Result<()> {
        if !self.outputs_changed && !initial {
            return Ok(());
        }
        self.outputs_changed = false;
        let chosen = self.choose_output().or_else(|| {
            if initial {
                None
            } else {
                self.outputs
                    .iter()
                    .find(|o| o.size().0 > 0 && o.size().1 > 0)
                    .map(|o| o.global)
            }
        });
        let Some(chosen) = chosen else {
            if initial {
                return Err(format!(
                    "Monitor '{}' unavailable. Use rs-wwidget --list-monitors.",
                    self.config.monitor
                )
                .into());
            }
            self.finish_drag(false);
            self.surfaces.clear();
            self.retiring_overlay = None;
            self.selected = None;
            return Ok(());
        };
        let output = self.outputs.iter().find(|o| o.global == chosen).unwrap();
        let (w, h) = output.size();
        let geometry = (w, h, output.scale);
        if self.selected == Some(chosen)
            && geometry == self.geometry
            && self.surfaces.iter().filter(|s| s.panel.is_some()).count()
                == self.config.enabled_panel_count()
            && !initial
        {
            return Ok(());
        }
        self.finish_drag(false);
        self.surfaces.clear();
        self.retiring_overlay = None;
        self.focus = None;
        self.geometry = geometry;
        self.selected = Some(chosen);
        if initial {
            self.layout = Layout::defaults(w, h).with_plugins(&self.config, w, h);
            let path = self.state_dir.join("layout.ini");
            if !path.exists() {
                let old = config::xdg("XDG_STATE_HOME", ".local/state")
                    .join("desktop-widgets/layout.ini");
                self.layout.load(&old, w, h);
            }
            self.layout.load(&path, w, h);
        } else {
            self.layout.constrain(w, h);
        }
        for i in 0..self.config.panel_count() {
            if self.config.panel_enabled(i) {
                self.create_surface(Some(i), qh);
            }
        }
        self.status();
        Ok(())
    }

    fn status(&self) {
        let (rss, pss) = metrics::memory_usage();
        let (cpu, threads) = unsafe {
            let mut usage: libc::rusage = std::mem::zeroed();
            libc::getrusage(libc::RUSAGE_SELF, &mut usage);
            let cpu = (usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) * 1_000_000
                + usage.ru_utime.tv_usec
                + usage.ru_stime.tv_usec;
            let threads = std::fs::read_dir("/proc/self/task")
                .map(|d| d.count())
                .unwrap_or(0);
            (cpu, threads)
        };
        let monitor = self
            .outputs
            .iter()
            .find(|o| Some(o.global) == self.selected)
            .map_or("", |o| o.name.as_str());
        let data = serde_json::json!({"pid":std::process::id(),"updated":unsafe{libc::time(std::ptr::null_mut())},"rss_kib":rss,"pss_kib":pss,"interval_seconds":5,"version":5,"drag_active":self.drag.is_some(),"cpu_usec":cpu,"elapsed_usec":self.started.elapsed().as_micros() as u64,"threads":threads,"monitor":monitor,"layer":"bottom","namespace":"rs-wwidget"});
        if let Err(e) = config::atomic_write(
            &self.state_dir.join("status.json"),
            data.to_string().as_bytes(),
        ) {
            eprintln!("Cannot write status: {e}");
        }
    }

    fn finish_drag(&mut self, commit: bool) {
        let Some(drag) = self.drag.take() else { return };
        if commit {
            self.layout.set_position(drag.panel, drag.preview);
            if let Err(e) = self.layout.save(&self.state_dir.join("layout.ini")) {
                eprintln!("Cannot save layout: {e}");
            }
        }
        // Keep the last preview visible until the restored panel has committed a buffer.
        // Destroying the overlay here would expose the cleared source for one frame.
        self.retiring_overlay =
            self.surfaces
                .iter()
                .find(|s| s.panel.is_none())
                .map(|s| RetiringOverlay {
                    id: s.id,
                    panel: drag.panel,
                });
        if let Some(s) = self
            .surfaces
            .iter_mut()
            .find(|s| s.panel == Some(drag.panel))
        {
            let p = self.layout.position(drag.panel);
            s.layer.set_margin(p.y, 0, 0, p.x);
            s.dirty = true;
        }
        eprintln!(
            "drag {}: panel {}",
            if commit { "commit" } else { "cancel" },
            drag.panel
        );
        self.status();
    }

    fn begin_drag(&mut self, qh: &QueueHandle<Self>) {
        if self.retiring_overlay.is_some() {
            return;
        }
        if self.drag.is_some() {
            self.finish_drag(false);
            return;
        }
        let Some(panel) = self
            .surfaces
            .iter()
            .find(|s| Some(s.id) == self.focus)
            .and_then(|s| s.panel)
        else {
            return;
        };
        let p = self.layout.position(panel);
        self.drag = Some(Drag {
            panel,
            initial: p,
            preview: p,
            start: self.pointer_pos,
            last_activity: Instant::now(),
            source_hidden: false,
        });
        // Do not clear the source while the overlay is still waiting for configure.
        // Its first frame callback transfers visibility after the preview is ready.
        self.create_surface(None, qh);
        eprintln!("drag begin: panel {panel}");
        self.status();
    }

    fn motion(&mut self) {
        let Some(mut drag) = self.drag else { return };
        let Some(surface) = self.surfaces.iter().find(|s| Some(s.id) == self.focus) else {
            return;
        };
        if surface.panel.is_some() && surface.panel != Some(drag.panel) {
            return;
        }
        let mut point = self.pointer_pos;
        if surface.panel.is_none() {
            point.0 -= drag.initial.x as f64;
            point.1 -= drag.initial.y as f64;
        }
        drag.last_activity = Instant::now();
        let next = layout::dragged_height(
            drag.initial,
            drag.start,
            point,
            self.geometry.0,
            self.geometry.1,
            self.config.panel_height(drag.panel),
        );
        if next != drag.preview {
            drag.preview = next;
            if let Some(s) = self.surfaces.iter_mut().find(|s| s.panel.is_none()) {
                s.dirty = true;
            }
        }
        self.drag = Some(drag);
    }

    fn update(&mut self) {
        let after = self.sampler.sample();
        let changed = self.display.changes(after);
        self.display = after;
        for s in &mut self.surfaces {
            let i = s.panel.or_else(|| self.drag.map(|d| d.panel));
            if i.is_some_and(|i| changed.get(i).copied().unwrap_or(false)) {
                s.dirty = true;
            }
        }
        self.ticks += 1;
        if self.ticks == 1 || self.ticks.is_multiple_of(12) {
            self.status();
        }
    }

    fn redraw(&mut self, qh: &QueueHandle<Self>) -> Result<()> {
        let shm = self.shm.as_ref().unwrap();
        let mut restored_overlay = None;
        for s in &mut self.surfaces {
            if self
                .retiring_overlay
                .is_some_and(|overlay| overlay.id == s.id)
            {
                continue;
            }
            if !s.dirty || !s.configured {
                continue;
            }
            let free = s
                .buffers
                .iter()
                .position(|b| !b.busy.load(Ordering::Relaxed));
            let index = if let Some(i) = free {
                i
            } else if s.buffers.len() < 2 {
                s.buffers
                    .push(Buffer::new(shm, qh, s.size.0, s.size.1, s.scale)?);
                s.buffers.len() - 1
            } else {
                continue;
            };
            let b = &mut s.buffers[index];
            let cr = Context::new(&b.image)?;
            let damage = if s.panel.is_none() && !b.fresh {
                b.preview.zip(self.drag).map(|(old, drag)| {
                    [
                        layout::damage_height(
                            old,
                            self.config.panel_height(drag.panel),
                            s.size.0,
                            s.size.1,
                        ),
                        layout::damage_height(
                            drag.preview,
                            self.config.panel_height(drag.panel),
                            s.size.0,
                            s.size.1,
                        ),
                    ]
                })
            } else {
                None
            };
            if let Some(areas) = damage {
                for (x, y, w, h) in areas {
                    cr.rectangle(x as f64, y as f64, w as f64, h as f64);
                }
                cr.clip();
            }
            render::clear(&cr)?;
            if let Some(panel) = s.panel {
                if self
                    .drag
                    .is_none_or(|d| d.panel != panel || !d.source_hidden)
                {
                    draw_panel(
                        &self.renderer,
                        &cr,
                        panel,
                        &self.display,
                        &self.config,
                        &self.plugins,
                    )?;
                }
            } else {
                self.renderer.grid(&cr, s.size.0, s.size.1)?;
                if let Some(d) = self.drag {
                    cr.save()?;
                    cr.translate(d.preview.x as f64, d.preview.y as f64);
                    draw_panel(
                        &self.renderer,
                        &cr,
                        d.panel,
                        &self.display,
                        &self.config,
                        &self.plugins,
                    )?;
                    let c = self.config.colors.accent;
                    cr.set_source_rgb(
                        ((c >> 16) & 255) as f64 / 255.0,
                        ((c >> 8) & 255) as f64 / 255.0,
                        (c & 255) as f64 / 255.0,
                    );
                    cr.set_line_width(2.0);
                    cr.rectangle(
                        1.0,
                        1.0,
                        (WIDTH - 2) as f64,
                        (self.config.panel_height(d.panel) - 2) as f64,
                    );
                    cr.stroke()?;
                    cr.restore()?;
                    b.preview = Some(d.preview);
                }
            }
            drop(cr);
            b.image.flush();
            b.fresh = false;
            b.busy.store(true, Ordering::Relaxed);
            s.wl.attach(Some(&b.proxy), 0, 0);
            if let Some(areas) = damage {
                for (x, y, w, h) in areas {
                    s.wl.damage(x, y, w, h);
                }
            } else {
                s.wl.damage(0, 0, s.size.0, s.size.1);
            }
            if s.panel.is_none() && s.first_commit {
                s.wl.frame(qh, s.id);
            }
            s.wl.commit();
            s.first_commit = false;
            s.dirty = false;
            if let Some(overlay) = self.retiring_overlay
                && s.panel == Some(overlay.panel)
            {
                restored_overlay = Some(overlay.id);
            }
        }
        // Commit restoration before sending destroy for the last visible preview.
        if let Some(id) = restored_overlay {
            self.surfaces.retain(|s| s.id != id);
            self.retiring_overlay = None;
        }
        Ok(())
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(
        s: &mut Self,
        reg: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_compositor" => s.compositor = Some(reg.bind(name, version.min(4), qh, ())),
                "wl_shm" => s.shm = Some(reg.bind(name, 1, qh, ())),
                "zwlr_layer_shell_v1" => s.shell = Some(reg.bind(name, version.min(4), qh, ())),
                "zxdg_output_manager_v1" => {
                    let manager: zxdg_output_manager_v1::ZxdgOutputManagerV1 =
                        reg.bind(name, version.min(3), qh, ());
                    for output in &mut s.outputs {
                        output.xdg = Some(manager.get_xdg_output(&output.proxy, qh, output.global));
                    }
                    s.output_manager = Some(manager);
                }
                "wl_output" => {
                    let proxy: wl_output::WlOutput = reg.bind(name, version.min(4), qh, name);
                    let xdg = s
                        .output_manager
                        .as_ref()
                        .map(|m| m.get_xdg_output(&proxy, qh, name));
                    s.outputs.push(Output {
                        global: name,
                        proxy,
                        xdg,
                        name: format!("output-{name}"),
                        description: String::new(),
                        pixels: (0, 0),
                        logical: None,
                        scale: 1,
                        rotated: false,
                    });
                    s.outputs_changed = true;
                }
                "wl_seat" if s.pointer.is_none() => {
                    let _: wl_seat::WlSeat = reg.bind(name, version.min(5), qh, ());
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name }
                if s.outputs.iter().any(|o| o.global == name) =>
            {
                s.outputs.retain(|o| {
                    if o.global == name {
                        if let Some(x) = &o.xdg {
                            x.destroy();
                        }
                        if o.proxy.version() >= 3 {
                            o.proxy.release();
                        }
                        false
                    } else {
                        true
                    }
                });
                s.outputs_changed = true;
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_output::WlOutput, u32> for App {
    fn event(
        s: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(o) = s.outputs.iter_mut().find(|o| o.global == *id) else {
            return;
        };
        match event {
            wl_output::Event::Name { name } => o.name = name,
            wl_output::Event::Description { description } => o.description = description,
            wl_output::Event::Mode {
                flags: WEnum::Value(flags),
                width,
                height,
                ..
            } if flags.contains(wl_output::Mode::Current) => o.pixels = (width, height),
            wl_output::Event::Scale { factor } => o.scale = factor.max(1),
            wl_output::Event::Geometry {
                transform: WEnum::Value(t),
                ..
            } => {
                o.rotated = matches!(
                    t,
                    wl_output::Transform::_90
                        | wl_output::Transform::_270
                        | wl_output::Transform::Flipped90
                        | wl_output::Transform::Flipped270
                )
            }
            _ => {}
        }
        s.outputs_changed = true;
    }
}
impl Dispatch<zxdg_output_v1::ZxdgOutputV1, u32> for App {
    fn event(
        s: &mut Self,
        _: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(o) = s.outputs.iter_mut().find(|o| o.global == *id) else {
            return;
        };
        match event {
            zxdg_output_v1::Event::Name { name } => o.name = name,
            zxdg_output_v1::Event::Description { description } => o.description = description,
            zxdg_output_v1::Event::LogicalSize { width, height } if width > 0 && height > 0 => {
                o.logical = Some((width, height))
            }
            _ => {}
        }
        s.outputs_changed = true;
    }
}
impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, SurfaceId> for App {
    fn event(
        s: &mut Self,
        layer: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                if let Some(surface) = s.surfaces.iter_mut().find(|surface| surface.id == *id) {
                    layer.ack_configure(serial);
                    surface.configured = true;
                    surface.dirty = true;
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                if s.surfaces
                    .iter()
                    .any(|surface| surface.id == *id && surface.panel.is_none())
                {
                    s.finish_drag(false);
                } else {
                    s.outputs_changed = true;
                    s.surfaces.retain(|surface| surface.id != *id);
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_callback::WlCallback, SurfaceId> for App {
    fn event(
        s: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Ignore late callbacks from an overlay that was cancelled or replaced.
        if s.surfaces
            .iter()
            .any(|surface| surface.id == *id && surface.panel.is_none())
            && let Some(drag) = &mut s.drag
        {
            drag.source_hidden = true;
            if let Some(surface) = s
                .surfaces
                .iter_mut()
                .find(|surface| surface.panel == Some(drag.panel))
            {
                surface.dirty = true;
            }
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, Arc<AtomicBool>> for App {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        busy: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            busy.store(false, Ordering::Relaxed);
        }
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(
        s: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(c),
        } = event
        {
            if c.contains(wl_seat::Capability::Pointer) && s.pointer.is_none() {
                s.pointer = Some(seat.get_pointer(qh, ()));
            }
            if !c.contains(wl_seat::Capability::Pointer) {
                if let Some(p) = s.pointer.take() {
                    p.release();
                }
                s.focus = None;
                s.finish_drag(false);
            }
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for App {
    fn event(
        s: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                s.focus = surface.data::<SurfaceId>().copied();
                s.pointer_pos = (surface_x, surface_y);
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if s.focus == surface.data::<SurfaceId>().copied() {
                    s.focus = None;
                }
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                s.pointer_pos = (surface_x, surface_y);
                s.motion();
            }
            wl_pointer::Event::Button {
                button,
                state: WEnum::Value(state),
                ..
            } => {
                if state == wl_pointer::ButtonState::Pressed {
                    if s.drag.is_some() {
                        s.finish_drag(false);
                    } else if button == 0x110 {
                        s.begin_drag(qh);
                    }
                } else if button == 0x110 && s.drag.is_some() {
                    s.motion();
                    s.finish_drag(true);
                }
            }
            _ => {}
        }
    }
}
delegate_noop!(App: ignore wl_compositor::WlCompositor);
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: ignore wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_region::WlRegion);
impl Dispatch<wl_surface::WlSurface, SurfaceId> for App {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
delegate_noop!(App: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(App: ignore zxdg_output_manager_v1::ZxdgOutputManagerV1);

pub fn run(config: Config, list: bool) -> Result<()> {
    let conn = Connection::connect_to_env().map_err(|e| {
        format!("Cannot connect to Wayland: {e}. Launch from your desktop session.")
    })?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let mut app = App::new(config);
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut app)?;
    queue.roundtrip(&mut app)?;
    if list {
        for o in &app.outputs {
            let (w, h) = o.size();
            println!(
                "{}\t{}x{} scale={}\t{}",
                o.name, w, h, o.scale, o.description
            );
        }
        return Ok(());
    }
    if app.compositor.is_none() || app.shm.is_none() || app.shell.is_none() {
        return Err("Compositor requires wl_compositor, wl_shm and wlr-layer-shell support".into());
    }
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&app.state_dir)?;
    let mut lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(app.state_dir.join("lock"))?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let e = std::io::Error::last_os_error();
        if e.kind() == std::io::ErrorKind::WouldBlock {
            return Ok(());
        }
        return Err(e.into());
    }
    rs_wwidget::instance::record(&mut lock)?;
    // SA_RESTART is deliberately off: SIGTERM interrupts the idle poll immediately.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = signal as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut());
        libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut());
    }
    app.display = app.sampler.sample();
    app.sync_outputs(&qh, true)?;
    let mut next_sample = Instant::now() + Duration::from_secs(5);
    let mut poll_fds = Vec::new();
    let result = (|| -> Result<()> {
        while app.running && !STOP.load(Ordering::Relaxed) {
            queue.dispatch_pending(&mut app)?;
            app.sync_outputs(&qh, false)?;
            let now = Instant::now();
            if now >= next_sample {
                app.update();
                next_sample = now + Duration::from_secs(5);
            }
            if app
                .drag
                .is_some_and(|d| d.last_activity.elapsed() >= Duration::from_secs(30))
            {
                app.finish_drag(false);
            }
            for (panel, plugin) in &mut app.plugins {
                if plugin.tick(now) {
                    for surface in &mut app.surfaces {
                        if surface.panel.or_else(|| app.drag.map(|d| d.panel)) == Some(*panel) {
                            surface.dirty = true;
                        }
                    }
                }
            }
            app.redraw(&qh)?;
            let Some(guard) = queue.prepare_read() else {
                continue;
            };
            let mut events = libc::POLLIN;
            if let Err(e) = conn.flush() {
                if matches!(&e,wayland_client::backend::WaylandError::Io(e) if e.kind()==std::io::ErrorKind::WouldBlock)
                {
                    events |= libc::POLLOUT;
                } else {
                    return Err(e.into());
                }
            }
            let mut deadline = app.drag.map_or(next_sample, |d| {
                next_sample.min(d.last_activity + Duration::from_secs(30))
            });
            for (_, plugin) in &app.plugins {
                deadline = deadline.min(plugin.deadline());
            }
            let timeout = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(i32::MAX as u128) as i32;
            let mut fd = libc::pollfd {
                fd: conn.as_fd().as_raw_fd(),
                events,
                revents: 0,
            };
            let n = if app.plugins.is_empty() {
                unsafe { libc::poll(&mut fd, 1, timeout) }
            } else {
                poll_fds.clear();
                poll_fds.push(fd);
                for (_, plugin) in &app.plugins {
                    if let Some(plugin_fd) = plugin.poll_fd() {
                        poll_fds.push(libc::pollfd {
                            fd: plugin_fd,
                            events: libc::POLLIN,
                            revents: 0,
                        });
                    }
                }
                let n = unsafe {
                    libc::poll(
                        poll_fds.as_mut_ptr(),
                        poll_fds.len() as libc::nfds_t,
                        timeout,
                    )
                };
                fd = poll_fds[0];
                n
            };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                drop(guard);
                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e.into());
            }
            if fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err("Wayland connection closed".into());
            }
            if n > 0 && fd.revents & libc::POLLIN != 0 {
                if let Err(e) = guard.read()
                    && !matches!(&e,wayland_client::backend::WaylandError::Io(e) if e.kind()==std::io::ErrorKind::WouldBlock)
                {
                    return Err(e.into());
                }
            } else {
                drop(guard);
            }
        }
        Ok(())
    })();
    app.plugins.clear();
    app.finish_drag(false);
    app.surfaces.clear();
    let _ = conn.flush();
    let _ = std::fs::remove_file(app.state_dir.join("status.json"));
    drop(lock);
    result
}

fn draw_panel(
    renderer: &Renderer,
    cr: &Context,
    panel: usize,
    display: &Display,
    config: &Config,
    plugins: &[(usize, Runtime)],
) -> Result<()> {
    if panel < 3 {
        renderer.panel(cr, panel, display)?;
    } else if let Some((_, plugin)) = plugins.iter().find(|(i, _)| *i == panel) {
        renderer.plugin(cr, config.panel_height(panel), plugin.frame())?;
    }
    Ok(())
}
