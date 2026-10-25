mod control;
mod wayland;
use rs_wwidget::{
    Result,
    config::{self, Config},
    layout::{self, Layout},
    metrics::{Display, Sampler},
    render::Renderer,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

fn preview(cfg: Config, path: &Path, wallpaper: Option<&Path>, grid: bool) -> Result<()> {
    use cairo::{Context, Format, ImageSurface};
    let image = wallpaper
        .map(image::open)
        .transpose()?
        .map(|i| i.to_rgba8());
    let plugin_height: i32 = cfg
        .plugins
        .iter()
        .filter(|p| p.enabled)
        .map(|p| p.height + 16)
        .sum();
    let builtin_height = [144, 434, 628]
        .into_iter()
        .enumerate()
        .filter(|(i, _)| cfg.panel_enabled(*i))
        .map(|(_, h)| h)
        .max()
        .unwrap_or(0);
    let plugin_start = if builtin_height > 0 {
        builtin_height + 16
    } else {
        0
    };
    let preview_height = if plugin_height > 0 {
        plugin_start + plugin_height - 16
    } else {
        628
    };
    let plugin_width = cfg
        .plugins
        .iter()
        .filter(|p| p.enabled)
        .map(|p| p.width)
        .max()
        .unwrap_or(300);
    let preview_width = if builtin_height > 0 {
        plugin_width.max(300)
    } else {
        plugin_width
    };
    let (w, h) = image.as_ref().map_or(
        if grid {
            (1920, 1080)
        } else {
            (preview_width, preview_height)
        },
        |i| (i.width() as i32, i.height() as i32),
    );
    let surface = ImageSurface::create(Format::ARgb32, w, h)?;
    let cr = Context::new(&surface)?;
    if let Some(image) = image {
        let mut bytes = Vec::with_capacity(w as usize * h as usize * 4);
        for p in image.pixels() {
            let [r, g, b, a] = p.0;
            let premultiply = |c: u8| ((c as u32 * a as u32 + 127) / 255) as u8;
            bytes.extend_from_slice(
                &u32::from_be_bytes([a, premultiply(r), premultiply(g), premultiply(b)])
                    .to_ne_bytes(),
            );
        }
        let background = ImageSurface::create_for_data(bytes, Format::ARgb32, w, h, w * 4)?;
        cr.set_source_surface(&background, 0.0, 0.0)?;
        cr.paint()?;
    }
    let renderer = Renderer::new(cfg.colors, cfg.background_opacity)
        .with_font_family(&cfg.font_family)
        .with_language(cfg.language);
    let state = Sampler::default().sample();
    let mut demo = Layout::defaults(w, h).with_plugins(&cfg, w, h);
    let positioned = grid || wallpaper.is_some();
    if grid {
        demo.pos[1] = layout::dragged(demo.pos[1], (0.0, 0.0), (-480.0, -32.0), w, h, 1);
        renderer.grid(&cr, w, h)?;
    }
    for (i, y) in [0, 160, 450].into_iter().enumerate() {
        if !cfg.panel_enabled(i) {
            continue;
        }
        cr.save()?;
        let (x, y) = if positioned {
            (demo.pos[i].x, demo.pos[i].y)
        } else {
            (0, y)
        };
        cr.translate(x as f64, y as f64);
        renderer.panel(&cr, i, &state)?;
        cr.restore()?;
    }
    let mut next_y = plugin_start;
    for (i, plugin) in cfg.plugins.iter().enumerate().filter(|(_, p)| p.enabled) {
        let position = if positioned {
            demo.position(i + 3)
        } else {
            layout::Position { x: 0, y: next_y }
        };
        cr.save()?;
        cr.translate(position.x as f64, position.y as f64);
        let placeholder = rs_wwidget::plugin::Frame::placeholder(&plugin.id, "Preview");
        renderer.widget(
            &cr,
            plugin,
            plugin.static_data.as_ref().unwrap_or(&placeholder),
        )?;
        cr.restore()?;
        next_y += plugin.height + 16;
    }
    surface.write_to_png(&mut File::create(path)?)?;
    Ok(())
}

fn benchmark(cfg: Config) -> Result<()> {
    let r = Renderer::new(cfg.colors, cfg.background_opacity)
        .with_font_family(&cfg.font_family)
        .with_language(cfg.language);
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 300, 628)?;
    let cr = cairo::Context::new(&surface)?;
    let mut state = Sampler::default().sample();
    state.cpu = Some(12.345);
    state.memory = Some((5.2, 15.5));
    state.disk = Some((132.4, 0.321));
    let mut before = Display::default();
    let start = cpu_time();
    let mut draws = 0;
    for minute in 0..20 {
        for tick in 0..12 {
            state.date.minute = minute;
            state.cpu = Some(12.345 + tick as f64);
            let changed = before.changes(state);
            before = state;
            for (i, changed) in changed.into_iter().enumerate() {
                if changed && cfg.panel_enabled(i) {
                    r.panel(&cr, i, &state)?;
                    draws += 1;
                }
            }
        }
    }
    println!(
        "Render CPU benchmark: 20 simulated minutes, {draws} panel draws, {:.2} ms",
        (cpu_time() - start) * 1000.0
    );
    println!(
        "Wayland loop: no worker threads, at most two SHM buffers per surface; grid buffers exist only during dragging."
    );
    Ok(())
}
fn cpu_time() -> f64 {
    unsafe {
        let mut t: libc::timespec = std::mem::zeroed();
        libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut t);
        t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("rs-wwidget: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut explicit = None;
    if let Some(i) = args.iter().position(|s| s == "--config") {
        if i + 1 >= args.len() {
            return Err("--config requires a file path".into());
        }
        explicit = Some(PathBuf::from(args.remove(i + 1)));
        args.remove(i);
    }
    let action = args.first().map_or("--run", String::as_str);
    match action {
        "--help" | "-h" => {
            println!(
                "rs-wwidget [--config PATH] [start|stop|restart|toggle|status|remove]\n  --run                       Run in the foreground (default)\n  --list-monitors             List Wayland connector names and logical sizes\n  --list-widgets              List installed widget names\n  --check-config              Validate config and exit\n  --print-default-config      Print current default palette\n  --preview OUTPUT [WALLPAPER] Render PNG without Wayland (PNG/JPEG wallpaper)\n  --preview-grid OUTPUT       Render drag grid preview\n  --benchmark                 Headless rendering CPU benchmark"
            );
            return Ok(());
        }
        "--print-default-config" => {
            print!("{}", include_str!("../config.example.toml"));
            return Ok(());
        }
        "stop" => return control::stop(),
        "status" => return control::print_status(),
        "remove" => return control::remove(),
        _ => {}
    }
    let path = explicit
        .clone()
        .unwrap_or_else(|| config::config_dir().join("config.toml"));
    if action == "--list-widgets" && args.len() == 1 {
        for name in rs_wwidget::registry::available(Some(&path)) {
            println!("{name}");
        }
        return Ok(());
    }
    let cfg = Config::load(&path, explicit.is_some())?;
    match action {
        "--run" if args.len() <= 1 => wayland::run(cfg, false),
        "--list-monitors" if args.len() == 1 => wayland::run(cfg, true),
        "--check-config" if args.len() == 1 => {
            println!("Config OK: {} (monitor: {})", path.display(), cfg.monitor);
            Ok(())
        }
        "--preview" if (2..=3).contains(&args.len()) => {
            preview(cfg, Path::new(&args[1]), args.get(2).map(Path::new), false)
        }
        "--preview-grid" if args.len() == 2 => preview(cfg, Path::new(&args[1]), None, true),
        "--benchmark" if args.len() == 1 => benchmark(cfg),
        "start" if args.len() == 1 => control::start(explicit.as_deref()),
        "restart" if args.len() == 1 => {
            control::stop()?;
            control::start(explicit.as_deref())
        }
        "toggle" if args.len() == 1 => {
            if control::running_pid().is_some() {
                control::stop()
            } else {
                control::start(explicit.as_deref())
            }
        }
        _ => Err("Invalid arguments. Use rs-wwidget --help".into()),
    }
}
