use std::{fs, path::Path};
pub const WIDTH: i32 = 300;
pub const HEIGHTS: [i32; 3] = [144, 274, 178];
pub const NAMES: [&str; 3] = ["clock", "calendar", "system"];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub pos: [Position; 3],
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
        };
        l.constrain(w, h);
        l
    }
    pub fn constrain(&mut self, w: i32, h: i32) {
        for (p, height) in self.pos.iter_mut().zip(HEIGHTS) {
            p.x = snap(p.x as f64, w, WIDTH, 0);
            p.y = snap(p.y as f64, h, height, 48);
        }
    }
    pub fn load(&mut self, path: &Path, w: i32, h: i32) {
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        let mut section = None;
        let mut xy = [[None; 2]; 3];
        for line in text.lines().map(str::trim) {
            if let Some(s) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = NAMES.iter().position(|n| *n == s);
            } else if let (Some(i), Some((key, value))) = (section, line.split_once('=')) {
                let axis = match key.trim() {
                    "x" => 0,
                    "y" => 1,
                    _ => continue,
                };
                xy[i][axis] = value.trim().parse::<i32>().ok();
            }
        }
        for (p, v) in self.pos.iter_mut().zip(xy) {
            if let [Some(x), Some(y)] = v {
                *p = Position { x, y };
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
    Position {
        x: snap(initial.x as f64 + point.0 - start.0, w, WIDTH, 0),
        y: snap(initial.y as f64 + point.1 - start.1, h, HEIGHTS[index], 48),
    }
}
pub fn damage(p: Position, index: usize, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let (x, y) = ((p.x - 1).max(0), (p.y - 1).max(0));
    (
        x,
        y,
        (p.x + WIDTH + 1).min(w).saturating_sub(x).max(0),
        (p.y + HEIGHTS[index] + 1).min(h).saturating_sub(y).max(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
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
