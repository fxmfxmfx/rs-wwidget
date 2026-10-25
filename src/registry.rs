//! On-demand widget package lookup. The default configuration does not scan packages.
use crate::{
    Result,
    config::{self, PluginConfig},
};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

pub fn valid_id(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn roots(config_path: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(parent) = config_path.and_then(Path::parent) {
        roots.push(parent.join("widgets"));
    }
    roots.push(config::config_dir().join("widgets"));
    roots.push(config::xdg("XDG_DATA_HOME", ".local/share").join("rs-wwidget/widgets"));
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            roots.push(parent.join("widgets"));
            roots.push(parent.join("../share/rs-wwidget/widgets"));
        }
        // cargo run/test binaries live below target/{debug,release}; no baked-in source path.
        for parent in exe.ancestors().take(5) {
            if parent.join("Cargo.toml").is_file() {
                roots.push(parent.join("widgets"));
                break;
            }
        }
    }
    roots
}

pub fn resolve(name: &str, config_path: Option<&Path>) -> Result<PluginConfig> {
    resolve_in(name, &roots(config_path))
}

fn resolve_in(name: &str, roots: &[PathBuf]) -> Result<PluginConfig> {
    if !valid_id(name) {
        return Err(format!("invalid widget name: {name}").into());
    }
    for root in roots {
        let path = root.join(name).join("widget.toml");
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", path.display()).into()),
        };
        let mut fields: toml::Table =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let version = fields.remove("version").and_then(|v| v.as_integer());
        if !matches!(version, Some(1 | 2)) || fields.contains_key("enabled") {
            return Err(format!(
                "{}: manifest version must be 1 or 2; select packages using widgets",
                path.display()
            )
            .into());
        }
        let mut plugin: PluginConfig = toml::Value::Table(fields)
            .try_into()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if plugin.id != name {
            return Err(
                format!("{}: manifest id must match directory name", path.display()).into(),
            );
        }
        plugin
            .validate()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let directory = fs::canonicalize(path.parent().unwrap())?;
        let mut command = plugin.command;
        for argument in &mut command {
            *argument = argument.replace("{widget_dir}", &directory.to_string_lossy());
        }
        if command.first().is_some_and(|c| c.starts_with('.')) {
            command[0] = directory.join(&command[0]).to_string_lossy().into_owned();
        }
        plugin.id = name.into();
        plugin.command = command;
        plugin.enabled = true;
        plugin.asset_dir = directory;
        plugin.load_static()?;
        return Ok(plugin);
    }
    Err(
        format!("unknown widget '{name}'; install its widget.toml package or use --list-widgets")
            .into(),
    )
}

pub fn available(config_path: Option<&Path>) -> Vec<String> {
    let mut names: BTreeSet<String> = crate::layout::NAMES.iter().map(|s| (*s).into()).collect();
    for root in roots(config_path) {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                if entry.path().join("widget.toml").is_file()
                    && let Some(name) = entry.file_name().to_str().filter(|name| valid_id(name))
                {
                    names.insert(name.into());
                }
            }
        }
    }
    names.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root() -> PathBuf {
        let root = env::temp_dir().join(format!(
            "widget-registry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("custom")).unwrap();
        root
    }

    #[test]
    fn package_metadata_resolves_without_running_command() {
        let root = temporary_root();
        fs::write(root.join("custom/widget.toml"), "version=1\nid='custom'\ncommand=['./custom-bin','{widget_dir}/data.txt']\nheight=200\nrestart_seconds=15\ntimeout_seconds=45\n").unwrap();
        let plugin = resolve_in("custom", std::slice::from_ref(&root)).unwrap();
        assert_eq!(
            Path::new(&plugin.command[0]),
            root.join("custom/custom-bin")
        );
        assert_eq!(
            plugin.command[1],
            root.join("custom/data.txt").to_string_lossy()
        );
        assert_eq!(plugin.height, 200);
        assert!(plugin.enabled);
        assert!(!root.join("custom/custom-bin").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_override_does_not_silently_fall_back() {
        let root = temporary_root();
        fs::write(root.join("custom/widget.toml"), "version=99\nid='other'\ncommand=['/bin/true']\nheight=200\nrestart_seconds=1\ntimeout_seconds=60\n").unwrap();
        assert!(resolve_in("custom", std::slice::from_ref(&root)).is_err());
        assert!(resolve_in("../custom", std::slice::from_ref(&root)).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn widgets_list_enables_custom_package_without_plugin_block() {
        let root = temporary_root();
        fs::write(root.join("custom/widget.toml"), "version=1\nid='custom'\ncommand=['/bin/true']\nheight=200\nrestart_seconds=1\ntimeout_seconds=60\n").unwrap();
        // An explicit configuration discovers packages from its sibling widgets directory.
        let package_root = root.join("config/widgets");
        fs::create_dir_all(package_root.join("custom")).unwrap();
        fs::copy(
            root.join("custom/widget.toml"),
            package_root.join("custom/widget.toml"),
        )
        .unwrap();
        let config = crate::config::Config::parse_at(
            "widgets=['clock','custom']",
            Some(&root.join("config/config.toml")),
        )
        .unwrap();
        assert!(config.panel_enabled(0));
        assert!(!config.panel_enabled(1));
        assert!(config.panel_enabled(3));
        assert_eq!(config.enabled_panel_count(), 2);
        assert_eq!(config.plugins[0].id, "custom");
        fs::remove_dir_all(root).unwrap();
    }
}
