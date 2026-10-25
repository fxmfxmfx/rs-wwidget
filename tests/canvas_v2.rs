use cairo::{Context, Format, ImageSurface};
use rs_wwidget::{
    config::{Config, Decoration, Input, Palette, PluginConfig},
    layout::{self, Layout, Position},
    plugin::{Frame, Runtime},
    render::Renderer,
};
use serde_json::json;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn temp() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rs-canvas-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
fn parse(v: serde_json::Value) -> Frame {
    Frame::parse(&serde_json::to_vec(&v).unwrap()).unwrap()
}
fn pixels(renderer: &Renderer, config: &PluginConfig, frame: &Frame, scale: i32) -> Vec<u8> {
    let mut s =
        ImageSurface::create(Format::ARgb32, config.width * scale, config.height * scale).unwrap();
    s.set_device_scale(scale as f64, scale as f64);
    let cr = Context::new(&s).unwrap();
    renderer.widget(&cr, config, frame).unwrap();
    drop(cr);
    s.data().unwrap().to_vec()
}
fn pixel(data: &[u8], width: usize, x: usize, y: usize) -> u32 {
    u32::from_ne_bytes(
        data[(y * width + x) * 4..(y * width + x + 1) * 4]
            .try_into()
            .unwrap(),
    )
}
fn config() -> PluginConfig {
    PluginConfig {
        width: 64,
        height: 64,
        decoration: Decoration::None,
        input: Input::None,
        ..PluginConfig::default()
    }
}

#[test]
fn strict_versions_and_element_limits() {
    assert!(Frame::parse(br#"{"version":1,"rows":[]}"#).is_err());
    assert!(Frame::parse(br#"{"version":1,"title":"","rows":[],"elements":[]}"#).is_err());
    assert!(Frame::parse(br#"{"version":2,"typo":1}"#).is_err());
    for content in [
        json!({"type":"bar","value":1.1}),
        json!({"type":"text","text":"x","color":"#nope"}),
        json!({"type":"graph","values":[]}),
        json!({"type":"image","path":"a","fit":"bad"}),
        json!({"type":"rect","radius":-1}),
        json!({"type":"rect","colour":"accent"}),
    ] {
        assert!(
            Frame::parse(
                &serde_json::to_vec(
                    &json!({"version":2,"elements":[{"bounds":[0,0,32,32],"content":content}]})
                )
                .unwrap()
            )
            .is_err()
        );
    }
    let good = json!({"bounds":[0,0,32,32],"content":{"type":"rect"}});
    assert!(
        Frame::parse(&serde_json::to_vec(&json!({"version":2,"elements":vec![good;257]})).unwrap())
            .is_err()
    );
    for bounds in [[0, 0, 0, 32], [0, 0, 32, -1], [9000, 0, 32, 32]] {
        assert!(
            Frame::parse(
                &serde_json::to_vec(
                    &json!({"version":2,"elements":[{"bounds":bounds,"content":{"type":"rect"}}]})
                )
                .unwrap()
            )
            .is_err()
        );
    }
}

#[test]
fn transparent_canvas_clips_layers_and_respects_scale() {
    let frame = parse(json!({"version":2,"elements":[
        {"bounds":[0,0,32,32],"content":{"type":"rect","color":"#ff0000"}},
        {"bounds":[16,16,32,32],"opacity":0.5,"content":{"type":"rect","color":"#0000ff"}},
        {"bounds":[56,56,32,32],"content":{"type":"rect","color":"#00ff00"}}
    ]}));
    let r = Renderer::new(Palette::DEFAULT, 1.0);
    for scale in [1, 2] {
        let data = pixels(&r, &config(), &frame, scale);
        let get = |x, y| {
            pixel(
                &data,
                64 * scale as usize,
                x * scale as usize,
                y * scale as usize,
            )
        };
        assert_eq!(get(8, 8), 0xffff0000);
        assert_eq!(get(24, 24), 0xff7f0080);
        assert_eq!(get(40, 40), 0x80000080);
        assert_eq!(get(52, 52), 0);
        assert_eq!(get(60, 60), 0xff00ff00);
    }
}

#[test]
fn image_alpha_fit_and_revision_cache() {
    let dir = temp();
    let path = dir.join("image.png");
    image::RgbaImage::from_pixel(8, 4, image::Rgba([255, 0, 0, 128]))
        .save(&path)
        .unwrap();
    let cfg = PluginConfig {
        asset_dir: dir.clone(),
        ..config()
    };
    let r = Renderer::new(Palette::DEFAULT, 1.0);
    let frame = |fit, revision| {
        parse(
            json!({"version":2,"elements":[{"bounds":[0,0,64,64],"content":{"type":"image","path":"image.png","fit":fit,"revision":revision}}]}),
        )
    };
    let contain = pixels(&r, &cfg, &frame("contain", 0), 1);
    assert_eq!(pixel(&contain, 64, 32, 4), 0);
    assert_eq!(pixel(&contain, 64, 32, 32), 0x80800000);
    let cover = pixels(&r, &cfg, &frame("cover", 0), 1);
    assert_eq!(pixel(&cover, 64, 32, 4), 0x80800000);
    std::fs::remove_file(&path).unwrap();
    // No reopen or re-decode: the same revision still draws after deleting the source.
    assert_eq!(contain, pixels(&r, &cfg, &frame("contain", 0), 1));
    image::RgbaImage::from_pixel(8, 4, image::Rgba([0, 255, 0, 255]))
        .save(&path)
        .unwrap();
    let changed = pixels(&r, &cfg, &frame("stretch", 1), 1);
    assert_eq!(pixel(&changed, 64, 32, 4), 0xff00ff00);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn static_package_validates_without_a_process_and_unknown_fields_fail() {
    let root = temp();
    let dir = root.join("widgets/photo");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = "version=2\nid='photo'\nframe='frame.json'\nwidth=640\nheight=480\ndecoration='none'\ninput='none'\nposition=[1200,800]\n";
    std::fs::write(dir.join("widget.toml"), manifest).unwrap();
    std::fs::write(dir.join("frame.json"), r#"{"version":2,"elements":[]}"#).unwrap();
    std::fs::write(root.join("config.toml"), "widgets=['photo']").unwrap();
    let cfg = Config::load(&root.join("config.toml"), true).unwrap();
    assert_eq!(cfg.panel_width(3), 640);
    assert_eq!(cfg.panel_input(3), Input::None);
    let mut rt = Runtime::new(cfg.plugins[0].clone());
    assert_eq!(rt.frame().version, 2);
    let now = Instant::now();
    assert!(!rt.tick(now + Duration::from_secs(1000)));
    assert!(rt.poll_fd().is_none());
    assert!(rt.deadline() > now + Duration::from_secs(1000));
    let layout = Layout::defaults(1280, 720).with_plugins(&cfg, 1280, 720);
    assert_eq!(layout.position(3), Position { x: 640, y: 240 });
    assert_eq!(
        layout::dragged_size(
            Position { x: 0, y: 48 },
            (0., 0.),
            (2000., 2000.),
            (1280, 720),
            (640, 480)
        ),
        Position { x: 640, y: 240 }
    );
    assert_eq!(
        layout::damage_size(Position { x: 32, y: 64 }, 640, 480, 1280, 720),
        (31, 63, 642, 482)
    );
    std::fs::write(dir.join("widget.toml"), format!("{manifest}widht=200\n")).unwrap();
    assert!(Config::load(&root.join("config.toml"), true).is_err());
    std::fs::write(
        dir.join("widget.toml"),
        format!("{manifest}command=['/bin/false']\n"),
    )
    .unwrap();
    assert!(Config::load(&root.join("config.toml"), true).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_image_does_not_hide_other_elements() {
    let r = Renderer::new(Palette::DEFAULT, 1.0);
    let frame = parse(json!({"version":2,"elements":[
        {"bounds":[0,0,64,64],"content":{"type":"image","path":"/nonexistent/rs-wwidget-test.png"}},
        {"bounds":[10,10,8,8],"content":{"type":"rect","color":"green"}}
    ]}));
    let data = pixels(&r, &config(), &frame, 1);
    assert_eq!(pixel(&data, 64, 12, 12), 0xffa9b665);
}

#[test]
fn dynamic_v2_receives_geometry_and_protocol_hints() {
    let script = r#"printf '{"version":2,"title":"%s/%s/%s","elements":[{"bounds":[0,0,64,64],"content":{"type":"rect"}}]}\n' "$RS_WWIDGET_PROTOCOL" "$RS_WWIDGET_WIDTH" "$RS_WWIDGET_HEIGHT"; sleep 5"#;
    let mut rt = Runtime::new(PluginConfig {
        enabled: true,
        protocol: 2,
        width: 128,
        height: 64,
        command: vec!["/bin/sh".into(), "-c".into(), script.into()],
        ..PluginConfig::default()
    });
    rt.tick(Instant::now());
    for _ in 0..50 {
        rt.service(Instant::now());
        if rt.frame().version == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(rt.frame().version, 2);
    assert_eq!(rt.frame().title, "2/128/64");
    assert_eq!(rt.frame().elements.len(), 1);
}

#[test]
fn sdk_output_is_accepted_by_the_host() {
    let script = r#"from rs_wwidget import Image, Text, Rect, Line, Bar, Graph, emit_canvas
emit_canvas([Image('x.png',(0,0,64,64)), Text('Привет',(0,0,60,20)), Rect((0,0,10,10)), Line((0,0,10,10)), Bar(.5,(0,0,60,4)), Graph([0,.5,1],(0,0,60,20))])"#;
    let output = std::process::Command::new("python3")
        .args(["-c", script])
        .env(
            "PYTHONPATH",
            concat!(env!("CARGO_MANIFEST_DIR"), "/sdk/python"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let frame = Frame::parse(&output.stdout).unwrap();
    assert_eq!(frame.version, 2);
    assert_eq!(frame.elements.len(), 6);
}

#[test]
fn bar_graph_and_text_render_without_panel_decoration() {
    let frame = parse(json!({"version":2,"elements":[
        {"bounds":[0,0,64,8],"content":{"type":"bar","value":0.25,"color":"#00ff00","background":"#000000"}},
        {"bounds":[0,16,64,16],"content":{"type":"graph","values":[0.5,0.5],"color":"#ff0000","thickness":2}},
        {"bounds":[0,36,64,28],"content":{"type":"text","text":"ABC","size":20,"color":"foreground"}}
    ]}));
    let data = pixels(&Renderer::new(Palette::DEFAULT, 1.0), &config(), &frame, 1);
    assert_eq!(pixel(&data, 64, 4, 4), 0xff00ff00);
    assert_eq!(pixel(&data, 64, 32, 4), 0xff000000);
    assert_eq!(pixel(&data, 64, 32, 24), 0xffff0000);
    assert_eq!(pixel(&data, 64, 32, 12), 0);
    assert!((36..64).any(|y| (0..64).any(|x| pixel(&data, 64, x, y) != 0)));
}

#[test]
fn cli_preview_uses_static_frame_and_custom_size() {
    let root = temp();
    let dir = root.join("widgets/photo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(root.join("config.toml"), "widgets=['photo']").unwrap();
    std::fs::write(dir.join("widget.toml"),"version=2\nid='photo'\nframe='frame.json'\nwidth=64\nheight=32\ndecoration='none'\ninput='none'\n").unwrap();
    std::fs::write(dir.join("frame.json"),r##"{"version":2,"elements":[{"bounds":[0,0,64,32],"content":{"type":"rect","color":"#ff0000"}}]}"##).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rs-wwidget"))
        .arg("--config")
        .arg(root.join("config.toml"))
        .arg("--preview")
        .arg(root.join("preview.png"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let image = image::open(root.join("preview.png")).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (64, 32));
    assert!(image.pixels().all(|p| p.0 == [255, 0, 0, 255]));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rotated_elements_use_local_clip_and_configurable_origin() {
    let renderer = Renderer::new(Palette::DEFAULT, 1.0);
    for scale in [1, 2] {
        let frame = parse(json!({"version":2,"elements":[{
            "bounds":[16,24,32,16],"rotation":90,"opacity":0.5,
            "content":{"type":"rect","color":"#ff0000"}
        }]}));
        let data = pixels(&renderer, &config(), &frame, scale);
        let get = |x, y| {
            pixel(
                &data,
                (64 * scale) as usize,
                (x * scale) as usize,
                (y * scale) as usize,
            )
        };
        // Rotation extends beyond the original 16px-high box rather than clipping to it.
        assert_eq!(get(32, 18), 0x80800000);
        assert_eq!(get(18, 32), 0);
        let origin = parse(json!({"version":2,"elements":[{
            "bounds":[16,24,32,16],"rotation":90,"origin":[0,0],
            "content":{"type":"rect","color":"#00ff00"}
        }]}));
        let data = pixels(&renderer, &config(), &origin, scale);
        assert_eq!(
            pixel(
                &data,
                (64 * scale) as usize,
                (8 * scale) as usize,
                (40 * scale) as usize
            ),
            0xff00ff00
        );
    }
}

#[test]
fn image_position_rounding_and_nearest_sampling() {
    let dir = temp();
    let mut image = image::RgbaImage::new(4, 2);
    for (x, _, p) in image.enumerate_pixels_mut() {
        *p = if x < 2 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        };
    }
    image.save(dir.join("crop.png")).unwrap();
    let cfg = PluginConfig {
        asset_dir: dir.clone(),
        ..config()
    };
    let renderer = Renderer::new(Palette::DEFAULT, 1.0);
    for (position, expected) in [(0.0, 0xffff0000), (1.0, 0xff0000ff)] {
        let frame = parse(json!({"version":2,"elements":[{
            "bounds":[0,0,64,64],"clip_radius":16,
            "content":{"type":"image","path":"crop.png","fit":"cover","position":[position,0.5],"filter":"nearest"}
        }]}));
        let data = pixels(&renderer, &cfg, &frame, 1);
        assert_eq!(pixel(&data, 64, 32, 32), expected);
        assert_eq!(pixel(&data, 64, 0, 0), 0);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn malformed_transforms_and_duplicate_fields_are_rejected() {
    for input in [
        r#"{"version":2,"version":1,"title":"","rows":[]}"#,
        r#"{"version":2,"title":null}"#,
        r#"{"version":2,"elements":null}"#,
        r#"{"version":2,"elements":[{"bounds":[0,0,64,64],"rotation":361,"content":{"type":"rect"}}]}"#,
        r#"{"version":2,"elements":[{"bounds":[0,0,64,64],"origin":[0,2],"content":{"type":"rect"}}]}"#,
        r#"{"version":2,"elements":[{"bounds":[0,0,64,64],"clip_radius":-1,"content":{"type":"rect"}}]}"#,
        r#"{"version":2,"elements":[{"bounds":[0,0,64,64],"content":{"type":"image","path":"x","position":[-1,0]}}]}"#,
        r#"{"version":2,"elements":[{"bounds":[0,0,64,64],"content":{"type":"image","path":"x","filter":"unknown"}}]}"#,
    ] {
        assert!(Frame::parse(input.as_bytes()).is_err(), "{input}");
    }
}

#[test]
fn disabled_static_legacy_definitions_do_not_read_frame_files() {
    let text = "widgets=[]\n[[plugins]]\nid='inactive'\nframe='/nonexistent/rs-wwidget-disabled-frame.json'\n";
    let cfg = Config::parse(text).unwrap();
    assert!(cfg.plugins[0].static_data.is_none());
    assert!(!cfg.panel_enabled(3));
    assert!(Config::parse(&text.replace("widgets=[]", "widgets=['inactive']")).is_err());
}

#[test]
fn poster_cli_creates_and_rotates_without_cropping_or_overwriting() {
    let root = temp();
    let widgets = root.join("widgets");
    std::fs::create_dir(&widgets).unwrap();
    let source = root.join("source.png");
    let dest = widgets.join("flyer");
    image::RgbaImage::from_pixel(80, 40, image::Rgba([255, 0, 0, 255]))
        .save(&source)
        .unwrap();
    let binary = env!("CARGO_BIN_EXE_rs-wwidget");
    let output = std::process::Command::new(binary)
        .arg("--create-poster")
        .arg(&source)
        .arg(&dest)
        .args(["--width", "80", "--rotation", "30", "--filter", "nearest"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config_path = root.join("config.toml");
    std::fs::write(&config_path, "widgets=['flyer']").unwrap();
    let cfg = Config::load(&config_path, true).unwrap();
    assert_eq!((cfg.panel_width(3), cfg.panel_height(3)), (90, 75));
    let before = std::fs::read(dest.join("widget.toml")).unwrap();
    let output = std::process::Command::new(binary)
        .arg("--create-poster")
        .arg(&source)
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(dest.join("widget.toml")).unwrap(), before);
    let output = std::process::Command::new(binary)
        .arg("--rotate-poster")
        .arg(&dest)
        .arg("90")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cfg = Config::load(&config_path, true).unwrap();
    let plugin = &cfg.plugins[0];
    assert_eq!((plugin.width, plugin.height), (40, 80));
    let r = Renderer::new(Palette::DEFAULT, 1.0);
    let data = pixels(&r, plugin, plugin.static_data.as_ref().unwrap(), 1);
    assert!(
        data.chunks_exact(4)
            .all(|p| u32::from_ne_bytes(p.try_into().unwrap()) == 0xffff0000)
    );
    let before = std::fs::read(dest.join("frame.json")).unwrap();
    let output = std::process::Command::new(binary)
        .arg("--rotate-poster")
        .arg(&dest)
        .arg("NaN")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(dest.join("frame.json")).unwrap(), before);
    let output = std::process::Command::new(binary)
        .arg("--check-frame")
        .arg(dest.join("frame.json"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        std::fs::read(dest.join("poster.png")).unwrap(),
        std::fs::read(source).unwrap()
    );
    std::fs::remove_dir_all(root).unwrap();
}
