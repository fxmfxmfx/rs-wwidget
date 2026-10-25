use rs_wwidget::ipc::{Overrides, Request};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
#[test]
fn persistent_control_overrides_round_trip_and_reject_bad_state() {
    let root = std::env::temp_dir().join(format!(
        "wwidget-controls-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let path = root.join("controls.json");
    let mut state = Overrides::load(&path).unwrap();
    let poster = state.0.entry("poster".into()).or_default();
    poster.locked = Some(true);
    poster.hidden = true;
    state.save(&path).unwrap();
    let read = Overrides::load(&path).unwrap();
    assert_eq!(read.0["poster"].locked, Some(true));
    assert!(read.0["poster"].hidden);
    fs::write(&path, r#"{"../bad":{"hidden":true}}"#).unwrap();
    assert!(Overrides::load(&path).is_err());
    fs::write(&path, r#"{"poster":{"mispelled":true}}"#).unwrap();
    assert!(Overrides::load(&path).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn ipc_request_schema_rejects_ambiguous_operations() {
    assert!(
        serde_json::from_str::<Request>(r#"{"op":"move","id":"poster","x":100,"y":200}"#).is_ok()
    );
    assert!(
        serde_json::from_str::<Request>(
            r#"{"op":"send","id":"controls","name":"refresh","data":{"n":2}}"#
        )
        .is_ok()
    );
    for bad in [
        r#"{"op":"move","id":"poster","x":0}"#,
        r#"{"op":"lock","id":"poster","locked":true,"execute":"x"}"#,
        r#"{"op":"shell","command":"x"}"#,
    ] {
        assert!(serde_json::from_str::<Request>(bad).is_err());
    }
}

#[test]
#[ignore = "requires Unix sockets; run with cargo test --test ipc -- --ignored on a desktop/CI host"]
fn real_datagram_channel_replies_without_blocking() {
    use std::os::unix::net::UnixDatagram;
    let root = std::env::temp_dir().join(format!("wwidget-ipc-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let server = rs_wwidget::ipc::Server::bind(root.join("control.sock")).unwrap();
    let client = UnixDatagram::bind(root.join("client.sock")).unwrap();
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    client
        .send_to(br#"{"op":"status","id":null}"#, root.join("control.sock"))
        .unwrap();
    let (request, peer) = server.receive().unwrap();
    assert!(matches!(request.unwrap(), Request::Status { id: None }));
    server.reply(&peer, Ok(serde_json::json!({"hello":true})));
    let mut buf = [0u8; 1024];
    let n = client.recv(&mut buf).unwrap();
    let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["hello"], true);
    drop(server);
    assert!(!root.join("control.sock").exists());
    drop(client);
    fs::remove_dir_all(root).unwrap();
}
