use crate::locale::Language;
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub(crate) fn read_text(path: &Path, limit: usize) -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("file exceeds {limit} bytes: {}", path.display()),
        ));
    }
    String::from_utf8(bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

pub const DEFAULT_FONT_FAMILY: &str = "MxPlus IBM VGA 8x16";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: u32,
    pub foreground: u32,
    pub muted: u32,
    pub accent: u32,
    pub green: u32,
    pub border: u32,
    pub bar_background: u32,
    pub grid_background: u32,
}

impl Palette {
    pub fn as_json(self) -> serde_json::Value {
        let mut colors = serde_json::Map::new();
        for (name, color) in [
            ("background", self.background),
            ("foreground", self.foreground),
            ("muted", self.muted),
            ("accent", self.accent),
            ("green", self.green),
            ("border", self.border),
            ("bar_background", self.bar_background),
            ("grid_background", self.grid_background),
        ] {
            colors.insert(
                name.into(),
                serde_json::Value::String(format!("#{color:06x}")),
            );
        }
        serde_json::Value::Object(colors)
    }
}

#[derive(Debug)]
pub struct Config {
    pub monitor: String,
    pub widgets: Vec<String>,
    pub language: Language,
    pub background_opacity: f64,
    pub font_family: String,
    pub colors: Palette,
    pub plugins: Vec<PluginConfig>,
}

/// Executables are opt-in; disabled definitions never create a runtime or surface.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PluginConfig {
    pub id: String,
    pub command: Vec<String>,
    pub enabled: bool,
    pub height: i32,
    pub width: i32,
    pub decoration: Decoration,
    pub input: Input,
    pub keyboard: bool,
    pub drag_height: Option<i32>,
    pub locked: bool,
    pub layer: Layer,
    pub render_mode: RenderMode,
    pub namespace: Option<String>,
    pub env: std::collections::BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub watch_files: Vec<String>,
    #[serde(skip)]
    pub package_path: Option<PathBuf>,
    #[serde(skip)]
    pub host_theme: Option<Palette>,
    pub protocol: u32,
    pub position: Option<[i32; 2]>,
    /// Static JSON frame instead of a child process.
    pub frame: Option<String>,
    #[serde(skip)]
    pub asset_dir: PathBuf,
    #[serde(skip)]
    pub static_data: Option<crate::plugin::Frame>,
    pub restart_seconds: u64,
    pub timeout_seconds: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decoration {
    #[default]
    Card,
    None,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    #[default]
    Title,
    All,
    None,
    Interactive,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Background,
    #[default]
    Bottom,
    Top,
    Overlay,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    #[default]
    Canvas,
    External,
}

impl PluginConfig {
    pub fn hosts_panel(&self) -> bool {
        self.enabled && self.render_mode == RenderMode::Canvas
    }
    pub fn handle_height(&self) -> i32 {
        if self.locked {
            return 0;
        }
        self.drag_height
            .unwrap_or(if self.decoration == Decoration::Card {
                42
            } else {
                0
            })
            .min(self.height)
    }

    pub fn validate(&self) -> crate::Result<()> {
        let command_ok = !self.command.is_empty()
            && !self.command[0].trim().is_empty()
            && !self.command.iter().any(|s| s.contains('\0'));
        if self.frame.is_some() {
            if !self.command.is_empty()
                || self
                    .frame
                    .as_ref()
                    .is_some_and(|p| p.trim().is_empty() || p.contains('\0'))
            {
                return Err("choose exactly one source: command or frame".into());
            }
        } else if !command_ok {
            return Err("command must be a nonempty argument array without NUL bytes".into());
        }
        if !(32..=8192).contains(&self.width)
            || !(32..=8192).contains(&self.height)
            || !(1..=86400).contains(&self.restart_seconds)
            || !(1..=86400).contains(&self.timeout_seconds)
            || ![1, 2].contains(&self.protocol)
        {
            return Err(
                "width/height must be 32..8192, protocol 1 or 2, restart/timeout 1..86400".into(),
            );
        }
        if self
            .drag_height
            .is_some_and(|h| !(0..=self.height).contains(&h))
        {
            return Err("drag_height must be between 0 and panel height".into());
        }
        if self
            .namespace
            .as_ref()
            .is_some_and(|s| !crate::registry::valid_id(s))
        {
            return Err("namespace must be a valid 1..64 character ID".into());
        }
        if self.watch_files.len() > 64
            || self
                .watch_files
                .iter()
                .any(|p| p.is_empty() || p.len() > 4096 || p.contains('\0'))
        {
            return Err(
                "watch_files accepts at most 64 nonempty paths, each at most 4096 bytes".into(),
            );
        }
        if self
            .cwd
            .as_ref()
            .is_some_and(|p| p.is_empty() || p.contains('\0'))
            || self.env.iter().any(|(key, value)| {
                key.is_empty() || key.contains(['=', '\0']) || value.contains('\0')
            })
        {
            return Err("invalid cwd or environment name/value".into());
        }
        if self.input == Input::Interactive && (self.frame.is_some() || self.protocol != 2) {
            return Err("interactive input requires a command and protocol = 2".into());
        }
        if self.render_mode == RenderMode::External
            && (self.frame.is_some() || self.input != Input::None || self.keyboard)
        {
            return Err("external renderers require a command, input='none', and keyboard=false; the external program owns input/surfaces".into());
        }
        if self.keyboard && self.input != Input::Interactive {
            return Err("keyboard requires input = 'interactive'".into());
        }
        Ok(())
    }

    pub fn load_static(&mut self) -> crate::Result<()> {
        if let Some(path) = &self.frame {
            let path = self.asset_dir.join(path);
            self.static_data = Some(
                crate::plugin::Frame::load(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?,
            );
        }
        Ok(())
    }
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            command: Vec::new(),
            enabled: false,
            height: 178,
            width: 300,
            decoration: Decoration::Card,
            input: Input::Title,
            keyboard: false,
            drag_height: None,
            locked: false,
            layer: Layer::Bottom,
            render_mode: RenderMode::Canvas,
            namespace: None,
            env: std::collections::BTreeMap::new(),
            cwd: None,
            watch_files: Vec::new(),
            package_path: None,
            host_theme: None,
            protocol: 1,
            position: None,
            frame: None,
            asset_dir: PathBuf::from("."),
            static_data: None,
            restart_seconds: 30,
            timeout_seconds: 60,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    monitor: Option<String>,
    widgets: Option<Vec<String>>,
    language: Option<Language>,
    background_opacity: Option<f64>,
    font_family: Option<String>,
    colors: Colors,
    plugins: Vec<PluginConfig>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Colors {
    background: Option<String>,
    foreground: Option<String>,
    muted: Option<String>,
    accent: Option<String>,
    green: Option<String>,
    border: Option<String>,
    bar_background: Option<String>,
    grid_background: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            monitor: "auto".into(),
            widgets: vec![
                "clock".to_string(),
                "calendar".to_string(),
                "system".to_string(),
            ],
            language: Language::En,
            background_opacity: 0.96,
            font_family: DEFAULT_FONT_FAMILY.into(),
            colors: Palette::DEFAULT,
            plugins: Vec::new(),
        }
    }
}

impl Config {
    pub fn panel_enabled(&self, index: usize) -> bool {
        if index < 3 {
            self.widgets
                .iter()
                .any(|widget| widget == crate::layout::NAMES[index])
        } else {
            self.plugins
                .get(index - 3)
                .is_some_and(PluginConfig::hosts_panel)
        }
    }

    pub fn parse(text: &str) -> crate::Result<Self> {
        Self::parse_at(text, None)
    }

    pub(crate) fn parse_at(text: &str, path: Option<&Path>) -> crate::Result<Self> {
        let raw: FileConfig = toml::from_str(text)?;
        let mut cfg = Self {
            language: raw.language.unwrap_or_default(),
            plugins: raw.plugins,
            ..Self::default()
        };
        for (i, plugin) in cfg.plugins.iter().enumerate() {
            if plugin.id.is_empty()
                || plugin.id.len() > 64
                || !plugin
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                || crate::layout::NAMES.contains(&plugin.id.as_str())
                || cfg.plugins[..i].iter().any(|p| p.id == plugin.id)
            {
                return Err("plugins require unique IDs of 1..64 ASCII letters, digits, _ or - (excluding builtin names)".into());
            }
            plugin
                .validate()
                .map_err(|e| format!("plugin {}: {e}", plugin.id))?;
        }
        for plugin in &mut cfg.plugins {
            plugin.asset_dir = path
                .and_then(Path::parent)
                .unwrap_or(Path::new("."))
                .to_path_buf();
        }
        if let Some(widgets) = raw.widgets {
            for (i, widget) in widgets.iter().enumerate() {
                if widgets[..i].contains(widget) {
                    return Err("widgets must not contain duplicate names".into());
                }
            }
            for name in &widgets {
                if !crate::registry::valid_id(name) {
                    return Err(format!("invalid widget name: {name}").into());
                }
                if !crate::layout::NAMES.contains(&name.as_str()) {
                    if let Some(plugin) = cfg.plugins.iter_mut().find(|p| p.id == *name) {
                        plugin.enabled = true;
                    } else {
                        cfg.plugins.push(crate::registry::resolve(name, path)?);
                    }
                }
            }
            cfg.widgets = widgets;
        }
        for plugin in cfg
            .plugins
            .iter_mut()
            .filter(|p| p.enabled && p.static_data.is_none())
        {
            plugin.load_static()?;
        }
        if let Some(family) = raw.font_family {
            if family.trim().is_empty() {
                return Err("font_family must not be empty".into());
            }
            cfg.font_family = family;
        }
        if let Some(opacity) = raw.background_opacity {
            if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                return Err(
                    "background_opacity must be a finite number between 0.0 and 1.0".into(),
                );
            }
            cfg.background_opacity = opacity;
        }
        if let Some(monitor) = raw.monitor {
            if monitor.trim().is_empty() {
                return Err("monitor must be auto or a connector name".into());
            }
            cfg.monitor = monitor;
        }
        let p = &mut cfg.colors;
        for (key, value, target) in [
            ("background", raw.colors.background, &mut p.background),
            ("foreground", raw.colors.foreground, &mut p.foreground),
            ("muted", raw.colors.muted, &mut p.muted),
            ("accent", raw.colors.accent, &mut p.accent),
            ("green", raw.colors.green, &mut p.green),
            ("border", raw.colors.border, &mut p.border),
            (
                "bar_background",
                raw.colors.bar_background,
                &mut p.bar_background,
            ),
            (
                "grid_background",
                raw.colors.grid_background,
                &mut p.grid_background,
            ),
        ] {
            if let Some(value) = value {
                let hex = value
                    .strip_prefix('#')
                    .ok_or_else(|| format!("colors.{key}: expected #RRGGBB"))?;
                if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(format!("colors.{key}: expected #RRGGBB").into());
                }
                *target = u32::from_str_radix(hex, 16)?;
            }
        }
        Ok(cfg)
    }

    pub fn panel_count(&self) -> usize {
        3 + self.plugins.len()
    }

    pub fn panel_width(&self, index: usize) -> i32 {
        if index < 3 {
            crate::layout::WIDTH
        } else {
            self.plugins[index - 3].width
        }
    }

    pub fn panel_input(&self, index: usize) -> Input {
        if index < 3 {
            Input::Title
        } else {
            self.plugins[index - 3].input
        }
    }

    pub fn panel_height(&self, index: usize) -> i32 {
        if index < 3 {
            crate::layout::HEIGHTS[index]
        } else {
            self.plugins[index - 3].height
        }
    }

    pub fn enabled_panel_count(&self) -> usize {
        self.widgets
            .iter()
            .filter(|name| crate::layout::NAMES.contains(&name.as_str()))
            .count()
            + self.plugins.iter().filter(|p| p.hosts_panel()).count()
    }

    pub fn load(path: &Path, explicit: bool) -> crate::Result<Self> {
        match read_text(path, 1_048_576) {
            Ok(text) => Self::parse_at(&text, Some(path))
                .map_err(|e| format!("{}: {e}", path.display()).into()),
            Err(e) if !explicit && e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display()).into()),
        }
    }
}

pub fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("rs-wwidget")
}
pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("rs-wwidget")
}
pub fn xdg(key: &str, fallback: &str) -> PathBuf {
    env::var_os(key)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}

/// Replace atomically; do not leave partially written status/layout files.
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(data)?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plugins_are_opt_in_and_reject_ambiguous_or_invalid_definitions() {
        let base = "[[plugins]]\nid = 'custom'\ncommand = ['python3', '/tmp/example.py']\n";
        let config = Config::parse(base).unwrap();
        assert_eq!(config.enabled_panel_count(), 3);
        assert!(!config.panel_enabled(3));
        assert!(Config::parse("").unwrap().plugins.is_empty());
        let config = Config::parse(&format!("{base}enabled = true\nheight = 250\n")).unwrap();
        assert!(config.panel_enabled(3));
        assert_eq!(config.panel_height(3), 250);
        assert_eq!(config.enabled_panel_count(), 4);
        for bad in [
            base.replace("'custom'", "'clock'"),
            base.replace("'custom'", "'../custom'"),
            base.replace("'custom'", "''"),
            base.replace("['python3', '/tmp/example.py']", "[]"),
            format!("{base}height = 8193\n"),
            format!("{base}timeout_seconds = 0\n"),
            format!("{base}restart_seconds = 0\n"),
            format!("{base}enabeld = true\n"),
            format!("{base}\n{base}"),
        ] {
            assert!(Config::parse(&bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn enabled_widgets_default_subset_and_empty() {
        assert_eq!(
            Config::parse("").unwrap().widgets,
            vec![
                "clock".to_string(),
                "calendar".to_string(),
                "system".to_string()
            ]
        );
        let config = Config::parse("widgets = ['system', 'clock']").unwrap();
        assert!(config.panel_enabled(0));
        assert!(!config.panel_enabled(1));
        assert!(config.panel_enabled(2));
        assert!(Config::parse("widgets = []").unwrap().widgets.is_empty());
        for text in [
            "widgets = ['clock', 'clock']",
            "widgets = ['calender']",
            "widgets = ['Clock']",
            "widgets = 'clock'",
            "widgets = [1]",
        ] {
            assert!(Config::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn defaults_and_partial_palette() {
        assert_eq!(Config::parse("").unwrap().colors, Palette::DEFAULT);
        assert_eq!(Config::parse("").unwrap().font_family, DEFAULT_FONT_FAMILY);
        let c = Config::parse("monitor = 'DP-2'\nfont_family = 'MxPlus IBM VGA 8x16 MidTilde'\n[colors]\naccent = '#abcdef'").unwrap();
        assert_eq!(c.monitor, "DP-2");
        assert_eq!(c.font_family, "MxPlus IBM VGA 8x16 MidTilde");
        assert_eq!(c.colors.accent, 0xabcdef);
        assert_eq!(c.colors.background, 0x282828);
    }
    #[test]
    fn rejects_bad_colors_and_typos() {
        for s in [
            "[colors]\naccent = '#123'",
            "[colors]\naccent = '#zzzzzz'",
            "monitr = 'DP-1'",
            "monitor = ''",
            "font_family = '   '",
        ] {
            assert!(Config::parse(s).is_err(), "{s}");
        }
    }

    #[test]
    fn background_opacity_defaults_bounds_and_invalid_values() {
        assert_eq!(Config::parse("").unwrap().background_opacity, 0.96);
        for opacity in ["0", "0.0", "0.35", "1.0", "1"] {
            let config = Config::parse(&format!("background_opacity = {opacity}")).unwrap();
            assert_eq!(config.background_opacity, opacity.parse::<f64>().unwrap());
        }
        for value in ["-0.01", "1.01", "nan", "inf", "-inf", "'0.5'"] {
            assert!(
                Config::parse(&format!("background_opacity = {value}")).is_err(),
                "{value}"
            );
        }
    }

    #[test]
    fn widget_language_is_explicit_and_defaults_to_english() {
        assert_eq!(Config::parse("").unwrap().language, Language::En);
        assert_eq!(
            Config::parse("language = 'ru'").unwrap().language,
            Language::Ru
        );
        assert_eq!(
            Config::parse("language = 'en'").unwrap().language,
            Language::En
        );
        for value in ["'de'", "'EN'", "1", "''"] {
            assert!(Config::parse(&format!("language = {value}")).is_err());
        }
    }
}
