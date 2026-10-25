use rs_wwidget::{
    config::{Input, Layer},
    plugin::Runtime,
    registry,
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
fn root() -> PathBuf {
    let r = std::env::temp_dir().join(format!(
        "wwidget-reload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(r.join("sample")).unwrap();
    r
}
fn manifest() -> &'static str {
    "version=2\nid='sample'\nframe='frame.json'\nwidth=64\nheight=64\ndecoration='none'\nwatch_files=['frame.json']\n"
}
#[test]
fn static_reload_keeps_last_good_frame_on_bad_edits_then_recovers() {
    let root = root();
    let dir = root.join("sample");
    fs::write(dir.join("widget.toml"), manifest()).unwrap();
    fs::write(dir.join("frame.json"), r#"{"version":2,"title":"first"}"#).unwrap();
    let mut rt = Runtime::new(registry::resolve_directory(&dir).unwrap());
    let now = Instant::now();
    assert!(!rt.tick(now));
    fs::write(
        dir.join("frame.json"),
        r#"{"version":2,"title":"second-longer"}"#,
    )
    .unwrap();
    assert!(rt.tick(now + Duration::from_secs(1)));
    assert_eq!(rt.frame().title, "second-longer");
    assert_eq!(rt.config_generation(), 1);
    fs::write(dir.join("frame.json"), "invalid").unwrap();
    assert!(!rt.tick(now + Duration::from_secs(2)));
    assert_eq!(rt.frame().title, "second-longer");
    fs::write(
        dir.join("frame.json"),
        r#"{"version":2,"title":"recovered"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("widget.toml"),
        format!("{}locked=true\nlayer='top'\n", manifest()),
    )
    .unwrap();
    assert!(rt.tick(now + Duration::from_secs(3)));
    assert_eq!(rt.frame().title, "recovered");
    assert!(rt.config().locked);
    assert_eq!(rt.config().layer, Layer::Top);
    assert!(rt.poll_fd().is_none());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn changing_command_to_static_reaps_child_without_losing_the_new_source() {
    let root = root();
    let dir = root.join("sample");
    fs::write(
        dir.join("widget.toml"),
        "version=2\nid='sample'\ncommand=['/bin/sleep','10']\nwatch_files=['main.py']\n",
    )
    .unwrap();
    let mut rt = Runtime::new(registry::resolve_directory(&dir).unwrap());
    let now = Instant::now();
    rt.tick(now);
    assert!(rt.poll_fd().is_some());
    fs::write(dir.join("frame.json"), r#"{"version":2,"title":"static"}"#).unwrap();
    fs::write(dir.join("widget.toml"), manifest()).unwrap();
    rt.tick(now + Duration::from_secs(1));
    for _ in 0..100 {
        rt.tick(now + Duration::from_secs(2));
        if rt.config_generation() > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(rt.frame().title, "static");
    assert!(rt.poll_fd().is_none());
    assert!(rt.config().frame.is_some());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn package_cwd_env_and_lock_metadata_are_resolved() {
    let root = root();
    let dir = root.join("sample");
    fs::write(dir.join("widget.toml"),"version=2\nid='sample'\ncommand=['/bin/sleep','10']\ncwd='.'\nlocked=true\ninput='interactive'\nprotocol=2\n[env]\nASSET='{widget_dir}/settings.json'\n").unwrap();
    let cfg = registry::resolve_directory(&dir).unwrap();
    assert_eq!(cfg.handle_height(), 0);
    assert_eq!(cfg.input, Input::Interactive);
    assert_eq!(PathBuf::from(cfg.cwd.unwrap()), dir.join("."));
    assert_eq!(
        cfg.env["ASSET"],
        dir.join("settings.json").to_string_lossy()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_script_environment_and_unbounded_watch_lists_are_rejected() {
    use rs_wwidget::config::PluginConfig;
    let mut cfg = PluginConfig {
        command: vec!["test".into()],
        ..PluginConfig::default()
    };
    cfg.watch_files = vec!["x".into(); 65];
    assert!(cfg.validate().is_err());
    cfg.watch_files.clear();
    cfg.env.insert("bad=name".into(), "x".into());
    assert!(cfg.validate().is_err());
    cfg.env.clear();
    cfg.cwd = Some(String::new());
    assert!(cfg.validate().is_err());
}

#[test]
fn live_configuration_validates_before_writing_and_applies_on_tick() {
    let root = root();
    let dir = root.join("sample");
    fs::write(dir.join("widget.toml"), manifest()).unwrap();
    fs::write(
        dir.join("frame.json"),
        r#"{"version":2,"title":"original"}"#,
    )
    .unwrap();
    let mut rt = Runtime::new(registry::resolve_directory(&dir).unwrap());
    let now = Instant::now();
    rt.configure(&serde_json::json!({"width":128,"locked":true}), now)
        .unwrap();
    rt.tick(now);
    assert_eq!(rt.config().width, 128);
    assert!(rt.config().locked);
    assert_eq!(rt.frame().title, "original");
    let saved = fs::read(dir.join("widget.toml")).unwrap();
    for bad in [
        serde_json::json!({"width":8193}),
        serde_json::json!({"id":"renamed"}),
        serde_json::json!({"input":"interactive"}),
        serde_json::json!({"unknown":1}),
    ] {
        assert!(rt.configure(&bad, now).is_err());
        assert_eq!(fs::read(dir.join("widget.toml")).unwrap(), saved);
    }
    fs::remove_dir_all(root).unwrap();
}
