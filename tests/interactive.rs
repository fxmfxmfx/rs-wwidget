use rs_wwidget::{
    config::{Config, Input, PluginConfig},
    input::{Event, PointerKind},
    plugin::{Frame, Runtime},
};
use std::time::{Duration, Instant};

fn config(script: &str) -> PluginConfig {
    PluginConfig {
        id: "interactive-test".into(),
        enabled: true,
        protocol: 2,
        input: Input::Interactive,
        command: vec!["python3".into(), "-c".into(), script.into()],
        ..PluginConfig::default()
    }
}
fn drain(rt: &mut Runtime, title: &str) {
    for _ in 0..100 {
        rt.service(Instant::now());
        if rt.frame().title == title {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("expected title {title}, got {}", rt.frame().title);
}
fn button() -> Event {
    Event::Pointer {
        kind: PointerKind::Button,
        x: 42.0,
        y: 60.0,
        target: Some("toggle".into()),
        local: Some([18., 4.]),
        button: Some(272),
        pressed: Some(true),
        axis: None,
        value: None,
        drag_handle: false,
    }
}

#[test]
fn real_child_receives_click_and_changes_its_frame() {
    let script = r#"import json,sys,time
for line in sys.stdin:
 e=json.loads(line)
 title='ready' if e['type']=='init' else ('clicked:'+e['target'] if e['type']=='pointer' else 'other')
 print(json.dumps({'version':2,'title':title,'elements':[]}),flush=True)
"#;
    let mut rt = Runtime::new(config(script));
    rt.tick(Instant::now());
    drain(&mut rt, "ready");
    assert!(!rt.send_event(&button(), Instant::now()));
    drain(&mut rt, "clicked:toggle");
    assert_eq!(rt.generation(), 1);
}

#[test]
fn blocked_input_consumer_never_blocks_host_and_overflow_restarts_it() {
    let mut cfg = config("import time; time.sleep(10)");
    cfg.keyboard = true;
    let mut rt = Runtime::new(cfg);
    let now = Instant::now();
    rt.tick(now);
    rt.send_event(
        &Event::Keymap {
            format: "xkb_v1",
            keymap: "x".repeat(200_000),
        },
        now,
    );
    let start = Instant::now();
    let mut failed = false;
    for _ in 0..1000 {
        if rt.send_event(&button(), now) {
            failed = true;
            break;
        }
    }
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(failed);
    assert!(rt.poll_write_fd().is_none());
    assert_eq!(rt.frame().rows[0].value, "Unavailable");
}

#[test]
fn closed_stdin_is_an_isolated_plugin_failure() {
    let script = "import os,json,time;os.close(0);print(json.dumps({'version':2,'title':'ready'}),flush=True);time.sleep(10)";
    let mut rt = Runtime::new(config(script));
    rt.tick(Instant::now());
    drain(&mut rt, "ready");
    assert!(rt.send_event(&button(), Instant::now()));
    assert_eq!(rt.frame().rows[0].value, "Unavailable");
}

#[test]
fn hit_testing_respects_rotation_rounding_opacity_and_z_order() {
    let frame = Frame::parse(
        br##"{"version":2,"elements":[
      {"bounds":[0,0,64,64],"id":"back","content":{"type":"rect"}},
      {"bounds":[16,24,32,16],"rotation":90,"id":"front","clip_radius":4,"content":{"type":"rect"}},
      {"bounds":[0,0,64,64],"opacity":0,"id":"hidden","content":{"type":"rect"}},
      {"bounds":[0,0,64,64],"content":{"type":"text","text":"label"}}
    ]}"##,
    )
    .unwrap();
    let (target, local) = frame.target((32., 18.)).unwrap();
    assert_eq!(target, "front");
    assert!((local[0] - 2.).abs() < 1e-6);
    assert!((local[1] - 8.).abs() < 1e-6);
    assert_eq!(frame.target((2., 2.)).unwrap().0, "back");
    assert!(frame.target((-1., 0.)).is_none());
    assert!(Frame::parse(br#"{"version":2,"elements":[{"bounds":[0,0,1,1],"id":"x","content":{"type":"rect"}},{"bounds":[0,0,1,1],"id":"x","content":{"type":"rect"}}]}"#).is_err());
}

#[test]
fn event_input_and_keyboard_are_explicitly_opt_in() {
    let base = "widgets=[]\n[[plugins]]\nid='test'\ncommand=['test']\n";
    assert!(Config::parse(&format!("{base}input='interactive'\n")).is_err());
    assert!(Config::parse(&format!("{base}keyboard=true\n")).is_err());
    assert!(
        Config::parse(&format!(
            "{base}input='interactive'\nprotocol=2\nkeyboard=true\ndrag_height=0\n"
        ))
        .is_ok()
    );
    assert!(
        Config::parse(&format!(
            "{base}input='interactive'\nprotocol=2\ndrag_height=999\n"
        ))
        .is_err()
    );
}

#[test]
fn external_renderer_is_supervised_without_panel_or_frame_protocol() {
    use rs_wwidget::config::RenderMode;
    let mut cfg = config("import time;print('not a JSON frame',flush=True);time.sleep(10)");
    cfg.render_mode = RenderMode::External;
    cfg.input = Input::None;
    assert!(cfg.validate().is_ok());
    assert!(!cfg.hosts_panel());
    let mut rt = Runtime::new(cfg);
    let now = Instant::now();
    rt.tick(now);
    assert_eq!(rt.generation(), 1);
    assert!(rt.poll_fd().is_none());
    assert!(rt.poll_write_fd().is_none());
    assert!(!rt.tick(now + Duration::from_secs(1000)));
    assert_eq!(rt.generation(), 1);
    let cfg=Config::parse("widgets=[]\n[[plugins]]\nid='external'\ncommand=['external']\nenabled=true\nrender_mode='external'\ninput='none'\n").unwrap();
    assert_eq!(cfg.enabled_panel_count(), 0);
    assert!(!cfg.panel_enabled(3));
}

#[test]
fn bounded_stderr_is_available_in_provider_diagnostics() {
    let mut rt = Runtime::new(config(
        "import json,sys,time;sys.stderr.write('provider diagnostic\\n');sys.stderr.flush();print(json.dumps({'version':2,'title':'ready'}),flush=True);time.sleep(10)",
    ));
    rt.tick(Instant::now());
    drain(&mut rt, "ready");
    assert!(
        rt.diagnostics()["stderr_tail"]
            .as_str()
            .unwrap()
            .contains("provider diagnostic")
    );
}

#[test]
fn custom_ipc_events_round_trip_to_script_logic() {
    let script = r#"import json,sys
for line in sys.stdin:
 e=json.loads(line)
 title='ready' if e['type']=='init' else (e['name']+':'+str(e['data']['value']))
 print(json.dumps({'version':2,'title':title}),flush=True)
"#;
    let mut rt = Runtime::new(config(script));
    rt.tick(Instant::now());
    drain(&mut rt, "ready");
    rt.send_event(
        &Event::Custom {
            name: "command".into(),
            data: serde_json::json!({"value":42}),
        },
        Instant::now(),
    );
    drain(&mut rt, "command:42");
}

#[test]
fn init_and_environment_share_the_resolved_host_theme() {
    let script = r#"import json,sys,os,time
e=json.loads(sys.stdin.readline())
color=e['theme']['accent']
assert os.environ['RS_WWIDGET_CONTROL_SOCKET'].endswith('control.sock')
assert color==json.loads(os.environ['RS_WWIDGET_THEME'])['accent']
print(json.dumps({'version':2,'title':color}),flush=True)
time.sleep(10)
"#;
    let mut cfg = config(script);
    let mut palette = rs_wwidget::config::Palette::DEFAULT;
    palette.accent = 0x123456;
    cfg.host_theme = Some(palette);
    let mut rt = Runtime::new(cfg);
    rt.tick(Instant::now());
    drain(&mut rt, "#123456");
}
