use rs_wwidget::registry;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
#[test]
fn all_starter_backends_are_valid_packages_and_never_overwrite() {
    let root = std::env::temp_dir().join(format!(
        "wwidget-author-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    for backend in ["python", "javascript", "qml"] {
        let target = root.join(backend);
        let args = vec![
            target.to_string_lossy().into_owned(),
            "--backend".into(),
            backend.into(),
        ];
        rs_wwidget::author::create(&args).unwrap();
        let cfg = registry::resolve_directory(&target).unwrap();
        assert_eq!(cfg.id, backend);
        let before = fs::read(target.join("widget.toml")).unwrap();
        assert!(rs_wwidget::author::create(&args).is_err());
        assert_eq!(fs::read(target.join("widget.toml")).unwrap(), before);
    }
    fs::remove_dir_all(root).unwrap();
}
