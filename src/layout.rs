use std::{fs, path::Path};
pub const WIDTH: i32 = 300;
pub const HEIGHTS: [i32; 3] = [144, 274, 178];
pub const NAMES: [&str; 3] = ["clock", "calendar", "system"];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub pos: [Position; 3],
    extra: Vec<(String, i32, Position, i32)>,
}
pub fn snap(value: f64, extent: i32, size: i32, inset: i32) -> i32 {
    let max = (extent - size).max(0) / 16 * 16;
    let min = ((inset.max(0) + 15) / 16 * 16).min(max);
    if !value.is_finite() {
        return min;
    }
    (((value.clamp(min as f64, max as f64) + 8.0) / 16.0).floor() as i32 * 16).clamp(min, max)
}
impl Layout {
    pub fn defaults(w: i32, h: i32) -> Self {
        let mut l = Self {
            pos: [64, 224, 514].map(|y| Position {
                x: w - WIDTH - 40,
                y,
            }),
            extra: Vec::new(),
        };
        l.constrain(w, h);
        l
    }
    pub fn with_plugins(mut self, config: &crate::config::Config, w: i32, h: i32) -> Self {
        if config.plugins.is_empty() {
            return self;
        }
        let reserved = if config
            .widgets
            .iter()
            .any(|name| NAMES.contains(&name.as_str()))
        {
            WIDTH + 40
        } else {
            0
        };
        let column_width = config
            .plugins
            .iter()
            .filter(|p| p.hosts_panel())
            .map(|p| p.width + 20)
            .max()
            .unwrap_or(320);
        let columns = ((w - reserved - 96) / column_width).max(1) as usize;
        let mut next_y = vec![64; columns];
        self.extra = config
            .plugins
            .iter()
            .map(|p| {
                let column = next_y
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, y)| **y)
                    .map(|(i, _)| i)
                    .unwrap();
                let position = p.position.map_or(
                    Position {
                        x: 48 + column as i32 * column_width,
                        y: next_y[column],
                    },
                    |[x, y]| Position { x, y },
                );
                if p.hosts_panel() {
                    next_y[column] += (p.height + 31) / 16 * 16;
                }
                (p.id.clone(), p.height, position, p.width)
            })
            .collect();
        self.constrain(w, h);
        self
    }
    pub fn position(&self, index: usize) -> Position {
        if index < 3 {
            self.pos[index]
        } else {
            self.extra[index - 3].2
        }
    }
    pub fn set_position(&mut self, index: usize, position: Position) {
        if index < 3 {
            self.pos[index] = position;
        } else {
            self.extra[index - 3].2 = position;
        }
    }
    pub fn update_sizes(&mut self, config: &crate::config::Config, w: i32, h: i32) {
        for (id, height, _, width) in &mut self.extra {
            if let Some(plugin) = config.plugins.iter().find(|p| p.id == *id) {
                *height = plugin.height;
                *width = plugin.width;
            }
        }
        self.constrain(w, h);
    }
    pub fn constrain(&mut self, w: i32, h: i32) {
        for (p, height) in self.pos.iter_mut().zip(HEIGHTS) {
            p.x = snap(p.x as f64, w, WIDTH, 0);
            p.y = snap(p.y as f64, h, height, 48);
        }
        for (_, height, p, width) in &mut self.extra {
            p.x = snap(p.x as f64, w, *width, 0);
            p.y = snap(p.y as f64, h, *height, 48);
        }
    }
    pub fn load(&mut self, path: &Path, w: i32, h: i32) {
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        let mut section = None;
        let mut xy = vec![[None; 2]; 3 + self.extra.len()];
        for line in text.lines().map(str::trim) {
            if let Some(s) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = NAMES
                    .iter()
                    .position(|n| *n == s)
                    .or_else(|| self.extra.iter().position(|p| p.0 == s).map(|i| i + 3));
            } else if let (Some(i), Some((key, value))) = (section, line.split_once('=')) {
                let axis = match key.trim() {
                    "x" => 0,
                    "y" => 1,
                    _ => continue,
                };
                xy[i][axis] = value.trim().parse::<i32>().ok();
            }
        }
        for (i, v) in xy.into_iter().enumerate() {
            if let [Some(x), Some(y)] = v {
                self.set_position(i, Position { x, y });
            }
        }
        self.constrain(w, h);
    }
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut text = String::new();
        use std::fmt::Write;
        for (name, p) in NAMES.iter().zip(self.pos) {
            writeln!(text, "[{name}]\nx={}\ny={}\n", p.x, p.y).unwrap();
        }
        for (name, _, p, _) in &self.extra {
            writeln!(text, "[{name}]\nx={}\ny={}\n", p.x, p.y).unwrap();
        }
        crate::config::atomic_write(path, text.as_bytes())
    }
}
pub fn dragged(
    initial: Position,
    start: (f64, f64),
    point: (f64, f64),
    w: i32,
    h: i32,
    index: usize,
) -> Position {
    dragged_height(initial, start, point, w, h, HEIGHTS[index])
}
pub fn dragged_height(
    initial: Position,
    start: (f64, f64),
    point: (f64, f64),
    w: i32,
    h: i32,
    height: i32,
) -> Position {
    dragged_size(initial, start, point, (w, h), (WIDTH, height))
}
pub fn dragged_size(
    initial: Position,
    start: (f64, f64),
    point: (f64, f64),
    extent: (i32, i32),
    size: (i32, i32),
) -> Position {
    let (w, h) = extent;
    let (width, height) = size;
    Position {
        x: snap(initial.x as f64 + point.0 - start.0, w, width, 0),
        y: snap(initial.y as f64 + point.1 - start.1, h, height, 48),
    }
}
pub fn damage(p: Position, index: usize, w: i32, h: i32) -> (i32, i32, i32, i32) {
    damage_height(p, HEIGHTS[index], w, h)
}
pub fn damage_height(p: Position, height: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
    damage_size(p, WIDTH, height, w, h)
}
pub fn damage_size(p: Position, width: i32, height: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let (x, y) = ((p.x - 1).max(0), (p.y - 1).max(0));
    (
        x,
        y,
        (p.x + width + 1).min(w).saturating_sub(x).max(0),
        (p.y + height + 1).min(h).saturating_sub(y).max(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plugin_positions_follow_ids_across_reordering_and_disabling() {
        let path = std::env::temp_dir().join(format!(
            "rs-wwidget-plugin-layout-{}.ini",
            std::process::id()
        ));
        let text = "[[plugins]]\nid = 'one'\ncommand = ['one']\nenabled = true\n[[plugins]]\nid = 'two'\ncommand = ['two']\n";
        let config = crate::config::Config::parse(text).unwrap();
        let mut layout = Layout::defaults(1920, 1080).with_plugins(&config, 1920, 1080);
        layout.set_position(3, Position { x: 320, y: 160 });
        layout.set_position(4, Position { x: 640, y: 320 });
        layout.save(&path).unwrap();
        let mut config = config;
        config.plugins.reverse();
        let mut restored = Layout::defaults(1920, 1080).with_plugins(&config, 1920, 1080);
        restored.load(&path, 1920, 1080);
        assert_eq!(restored.position(3), Position { x: 640, y: 320 });
        assert_eq!(restored.position(4), Position { x: 320, y: 160 });
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn persisted_layout_and_partial_legacy_ini() {
        let path =
            std::env::temp_dir().join(format!("rs-wwidget-layout-{}.ini", std::process::id()));
        let mut expected = Layout::defaults(1920, 1080);
        expected.pos[0] = Position { x: 320, y: 128 };
        expected.save(&path).unwrap();
        let mut actual = Layout::defaults(1920, 1080);
        actual.load(&path, 1920, 1080);
        assert_eq!(actual, expected);
        fs::write(
            &path,
            "[clock]\nx=100\ny=80\n[calendar]\nx=broken\ny=96\n[system]\nx=64\n",
        )
        .unwrap();
        actual.load(&path, 1920, 1080);
        assert_eq!(actual.pos[0], Position { x: 96, y: 80 });
        assert_eq!(actual.pos[1..], expected.pos[1..]);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn snap_clamp_tiny_and_nonfinite() {
        assert_eq!(snap(23.9, 1920, 300, 0), 16);
        assert_eq!(snap(24.0, 1920, 300, 0), 32);
        assert_eq!(snap(9999.0, 200, 300, 48), 0);
        assert_eq!(snap(f64::NAN, 1080, 144, 48), 48);
        let l = Layout::defaults(1920, 1080);
        assert_eq!(l.pos[2], Position { x: 1584, y: 512 });
    }
    #[test]
    fn drag_damage_and_resolution_shrink() {
        let mut l = Layout::defaults(1920, 1080);
        l.constrain(640, 480);
        assert!(
            l.pos
                .iter()
                .zip(HEIGHTS)
                .all(|(p, h)| p.x >= 0 && p.x + WIDTH <= 640 && p.y + h <= 480)
        );
        assert_eq!(
            dragged(
                Position { x: 320, y: 64 },
                (5.0, 5.0),
                (22.0, 22.0),
                960,
                640,
                1
            ),
            Position { x: 336, y: 80 }
        );
        assert_eq!(
            damage(Position { x: 0, y: 48 }, 1, 960, 640),
            (0, 47, 301, 276)
        );
    }
}
