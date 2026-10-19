use crate::locale::Language;
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const DEFAULT_FONT_FAMILY: &str = "MxPlus IBM VGA 8x16";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Widget {
    Clock,
    Calendar,
    System,
}

impl Widget {
    pub const fn index(self) -> usize {
        match self {
            Self::Clock => 0,
            Self::Calendar => 1,
            Self::System => 2,
        }
    }
}

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

#[derive(Debug)]
pub struct Config {
    pub monitor: String,
    pub widgets: Vec<Widget>,
    pub language: Language,
    pub background_opacity: f64,
    pub font_family: String,
    pub colors: Palette,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    monitor: Option<String>,
    widgets: Option<Vec<Widget>>,
    language: Option<Language>,
    background_opacity: Option<f64>,
    font_family: Option<String>,
    colors: Colors,
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
            widgets: vec![Widget::Clock, Widget::Calendar, Widget::System],
            language: Language::En,
            background_opacity: 0.96,
            font_family: DEFAULT_FONT_FAMILY.into(),
            colors: Palette::DEFAULT,
        }
    }
}

impl Config {
    pub fn panel_enabled(&self, index: usize) -> bool {
        self.widgets.iter().any(|widget| widget.index() == index)
    }

    pub fn parse(text: &str) -> crate::Result<Self> {
        let raw: FileConfig = toml::from_str(text)?;
        let mut cfg = Self {
            language: raw.language.unwrap_or_default(),
            ..Self::default()
        };
        if let Some(widgets) = raw.widgets {
            for (i, widget) in widgets.iter().enumerate() {
                if widgets[..i].contains(widget) {
                    return Err("widgets must not contain duplicate names".into());
                }
            }
            cfg.widgets = widgets;
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

    pub fn load(path: &Path, explicit: bool) -> crate::Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()).into()),
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
    fn enabled_widgets_default_subset_and_empty() {
        assert_eq!(
            Config::parse("").unwrap().widgets,
            vec![Widget::Clock, Widget::Calendar, Widget::System]
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
