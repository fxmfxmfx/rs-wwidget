//! Test-only virtual pointer driver for an isolated wlroots compositor.
//! Usage: cargo run --example test_pointer -- HEADLESS-1 1920 1080
//! Then send `move x y`, `press`, `release`, `right`, `middle`,
//! `middle_press`, `middle_release`, `scroll`, or `wait MS` on stdin.
use std::{
    collections::HashMap,
    io::{self, BufRead},
    thread,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{
        wl_output,
        wl_pointer::{Axis, ButtonState},
        wl_registry,
    },
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};

struct App {
    outputs: HashMap<u32, wl_output::WlOutput>,
    names: HashMap<u32, String>,
    manager: Option<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1>,
}
impl App {
    fn new() -> Self {
        Self {
            outputs: HashMap::new(),
            names: HashMap::new(),
            manager: None,
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_output" => {
                    let output =
                        registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, name);
                    state.outputs.insert(name, output);
                    state.names.insert(name, format!("output-{name}"));
                }
                "zwlr_virtual_pointer_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(2), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for App {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            state.names.insert(*id, name);
        }
    }
}
impl Dispatch<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
        _: zwlr_virtual_pointer_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
        _: zwlr_virtual_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
delegate_noop!(App: ignore wl_output::WlOutput);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: test_pointer OUTPUT WIDTH HEIGHT".into());
    }
    let connector = &args[0];
    let width: i32 = args[1].parse()?;
    let height: i32 = args[2].parse()?;
    if width <= 0 || height <= 0 {
        return Err("extents must be positive".into());
    }

    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let mut app = App::new();
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut app)?;
    queue.roundtrip(&mut app)?;
    let output_id = app
        .names
        .iter()
        .find_map(|(id, name)| (name == connector).then_some(*id))
        .ok_or_else(|| format!("output {connector:?} not found"))?;
    let output = app.outputs.get(&output_id).expect("named output is bound");
    let manager = app
        .manager
        .as_ref()
        .ok_or("compositor has no virtual-pointer manager")?;
    let pointer = manager.create_virtual_pointer_with_output(None, Some(output), &qh, ());
    conn.flush()?;
    queue.roundtrip(&mut app)?;
    println!("ready");
    use std::io::Write;
    io::stdout().flush()?;

    let start = Instant::now();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let mut parts = line.split_whitespace();
        let command = parts.next().unwrap_or("");
        match command {
            "move" => {
                let x: i32 = parts.next().ok_or("move requires x y")?.parse()?;
                let y: i32 = parts.next().ok_or("move requires x y")?.parse()?;
                if parts.next().is_some() {
                    return Err("move accepts exactly x y".into());
                }
                let x = x.clamp(0, width - 1);
                let y = y.clamp(0, height - 1);
                pointer.motion_absolute(
                    elapsed_ms(start),
                    x as u32,
                    y as u32,
                    width as u32,
                    height as u32,
                );
                pointer.frame();
            }
            "press" => button(&pointer, start, 0x110, true),
            "release" => button(&pointer, start, 0x110, false),
            "middle_press" => button(&pointer, start, 0x112, true),
            "middle_release" => button(&pointer, start, 0x112, false),
            "right" => {
                button(&pointer, start, 0x111, true);
                finish_action(&conn, &mut queue, &mut app)?;
                thread::sleep(Duration::from_millis(80));
                button(&pointer, start, 0x111, false);
            }
            "middle" => {
                button(&pointer, start, 0x112, true);
                finish_action(&conn, &mut queue, &mut app)?;
                thread::sleep(Duration::from_millis(80));
                button(&pointer, start, 0x112, false);
            }
            "scroll" => {
                pointer.axis(elapsed_ms(start), Axis::VerticalScroll, 10.0);
                pointer.frame();
            }
            "wait" => {
                let ms: u64 = parts.next().ok_or("wait requires milliseconds")?.parse()?;
                if parts.next().is_some() {
                    return Err("wait accepts one duration".into());
                }
                finish_action(&conn, &mut queue, &mut app)?;
                thread::sleep(Duration::from_millis(ms));
                println!("done");
                io::stdout().flush()?;
                continue;
            }
            "" => continue,
            other => return Err(format!("unknown command {other:?}").into()),
        }
        finish_action(&conn, &mut queue, &mut app)?;
        thread::sleep(Duration::from_millis(80));
        println!("done");
        io::stdout().flush()?;
    }
    pointer.destroy();
    conn.flush()?;
    Ok(())
}

fn elapsed_ms(start: Instant) -> u32 {
    start.elapsed().as_millis() as u32
}
fn button(
    pointer: &zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
    start: Instant,
    button: u32,
    pressed: bool,
) {
    pointer.button(
        elapsed_ms(start),
        button,
        if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    );
    pointer.frame();
}
fn finish_action(
    conn: &Connection,
    queue: &mut wayland_client::EventQueue<App>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.flush()?;
    queue.roundtrip(app)?;
    Ok(())
}
