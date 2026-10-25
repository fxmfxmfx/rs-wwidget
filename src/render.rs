use cairo::Context;
use pango::{Alignment, FontDescription, Layout};
use pangocairo::functions::{create_layout, show_layout};

use crate::config::{DEFAULT_FONT_FAMILY, Palette};
use crate::locale::{Labels, Language};
use crate::metrics::Display;

const WIDTH: f64 = 300.0;
const HEIGHTS: [i32; 3] = [144, 274, 178];
const OFFSETS: [f64; 3] = [0.0, 160.0, 450.0];
pub struct Renderer {
    palette: Palette,
    canvas: crate::canvas::Canvas,
    labels: &'static Labels,
    background_opacity: f64,
    fonts: [FontDescription; 2],
}

impl Renderer {
    pub fn new(palette: Palette, background_opacity: f64) -> Self {
        let make_font = |size: i32| {
            let mut font = FontDescription::new();
            font.set_family(DEFAULT_FONT_FAMILY);
            font.set_absolute_size((size * pango::SCALE) as f64);
            font
        };
        Self {
            palette,
            canvas: crate::canvas::Canvas::default(),
            labels: Language::En.labels(),
            background_opacity,
            fonts: [make_font(16), make_font(64)],
        }
    }

    pub fn with_language(mut self, language: Language) -> Self {
        self.labels = language.labels();
        self
    }

    pub fn with_font_family(mut self, family: &str) -> Self {
        for font in &mut self.fonts {
            font.set_family(family);
        }
        self
    }

    pub fn panel(&self, cr: &Context, index: usize, state: &Display) -> Result<(), cairo::Error> {
        if index >= 3 {
            return Ok(());
        }
        cr.save()?;
        cr.translate(0.0, -OFFSETS[index]);
        let layout = create_layout(cr);
        self.card(cr, OFFSETS[index] as i32, HEIGHTS[index]);
        let d = state.date;
        match index {
            0 => {
                self.text(
                    cr,
                    &layout,
                    self.labels.now,
                    24.0,
                    20.0,
                    16,
                    self.palette.muted,
                    0,
                    Alignment::Left,
                );
                self.text(
                    cr,
                    &layout,
                    &format!("{:02}:{:02}", d.hour, d.minute),
                    24.0,
                    42.0,
                    64,
                    self.palette.foreground,
                    252,
                    Alignment::Center,
                );
                let weekday = self
                    .labels
                    .weekdays
                    .get(d.weekday as usize)
                    .copied()
                    .unwrap_or(self.labels.weekdays[0]);
                self.text(
                    cr,
                    &layout,
                    &format!("{weekday} · {:02}.{:02}", d.day, d.month),
                    24.0,
                    113.0,
                    16,
                    self.palette.accent,
                    252,
                    Alignment::Center,
                );
            }
            1 => {
                self.text(
                    cr,
                    &layout,
                    self.labels.calendar,
                    24.0,
                    180.0,
                    16,
                    self.palette.muted,
                    0,
                    Alignment::Left,
                );
                let month = d.month.clamp(1, 12);
                self.text(
                    cr,
                    &layout,
                    &format!("{} {}", self.labels.months[(month - 1) as usize], d.year),
                    24.0,
                    208.0,
                    16,
                    self.palette.foreground,
                    252,
                    Alignment::Center,
                );
                for (c, label) in self.labels.days.iter().enumerate() {
                    self.text(
                        cr,
                        &layout,
                        label,
                        24.0 + c as f64 * 36.0,
                        244.0,
                        16,
                        if c >= 5 {
                            self.palette.accent
                        } else {
                            self.palette.muted
                        },
                        36,
                        Alignment::Center,
                    );
                }
                let first = crate::metrics::month_start(d.year, month);
                for day in 1..=crate::metrics::month_days(d.year, month) {
                    let cell = first + day - 1;
                    let col = cell % 7;
                    let row = cell / 7;
                    let (x, y) = (24.0 + col as f64 * 36.0, 272.0 + row as f64 * 26.0);
                    let today = day == d.day;
                    if today {
                        self.box_fill(cr, x + 4.0, y - 3.0, 28.0, 22.0, self.palette.accent, 1.0);
                    }
                    self.text(
                        cr,
                        &layout,
                        &format!("{day:2}"),
                        x,
                        y,
                        16,
                        if today {
                            self.palette.background
                        } else if col >= 5 {
                            self.palette.accent
                        } else {
                            self.palette.foreground
                        },
                        36,
                        Alignment::Center,
                    );
                }
            }
            _ => {
                self.text(
                    cr,
                    &layout,
                    self.labels.system,
                    24.0,
                    470.0,
                    16,
                    self.palette.muted,
                    0,
                    Alignment::Left,
                );
                self.text(
                    cr,
                    &layout,
                    "CPU",
                    24.0,
                    500.0,
                    16,
                    self.palette.foreground,
                    0,
                    Alignment::Left,
                );
                let (cpu, cpu_fraction) = state
                    .cpu
                    .map_or(("—".to_owned(), 0.0), |v| (format!("{v:.0}%"), v / 100.0));
                self.text(
                    cr,
                    &layout,
                    &cpu,
                    72.0,
                    500.0,
                    16,
                    self.palette.accent,
                    204,
                    Alignment::Right,
                );
                self.bar(cr, 524.0, cpu_fraction, self.palette.accent);
                self.text(
                    cr,
                    &layout,
                    self.labels.memory,
                    24.0,
                    540.0,
                    16,
                    self.palette.foreground,
                    0,
                    Alignment::Left,
                );
                let (memory, memory_fraction) =
                    state.memory.map_or(("—".to_owned(), 0.0), |(used, total)| {
                        (format!("{used:.1} / {total:.1} GiB"), used / total)
                    });
                self.text(
                    cr,
                    &layout,
                    &memory,
                    72.0,
                    540.0,
                    16,
                    self.palette.foreground,
                    204,
                    Alignment::Right,
                );
                self.bar(cr, 564.0, memory_fraction, self.palette.green);
                self.text(
                    cr,
                    &layout,
                    self.labels.disk,
                    24.0,
                    580.0,
                    16,
                    self.palette.foreground,
                    0,
                    Alignment::Left,
                );
                let (disk, disk_fraction) = state
                    .disk
                    .map_or(("—".to_owned(), 0.0), |(free, fraction)| {
                        (format!("{free:.0} GiB {}", self.labels.free), fraction)
                    });
                self.text(
                    cr,
                    &layout,
                    &disk,
                    72.0,
                    580.0,
                    16,
                    self.palette.foreground,
                    204,
                    Alignment::Right,
                );
                self.bar(cr, 604.0, disk_fraction, self.palette.muted);
            }
        }
        cr.restore()?;
        Ok(())
    }

    pub fn grid(&self, cr: &Context, width: i32, height: i32) -> Result<(), cairo::Error> {
        self.box_fill(
            cr,
            0.0,
            0.0,
            width as f64,
            height as f64,
            self.palette.grid_background,
            0.08,
        );
        cr.set_line_width(1.0);
        let (x1, y1, x2, y2) = cr.clip_extents()?;
        let first_x = (((x1 - 1.0) / 16.0).floor() as i32 * 16).max(0);
        let first_y = (((y1 - 1.0) / 16.0).floor() as i32 * 16).max(0);
        for major in 0..2 {
            set_color(
                cr,
                self.palette.accent,
                if major == 1 { 0.32 } else { 0.13 },
            );
            let mut x = first_x;
            while x < width && (x as f64) <= x2 + 1.0 {
                if ((x % 64 == 0) as i32) == major {
                    cr.move_to(x as f64 + 0.5, 0.0);
                    cr.line_to(x as f64 + 0.5, height as f64);
                }
                x += 16;
            }
            let mut y = first_y;
            while y < height && (y as f64) <= y2 + 1.0 {
                if ((y % 64 == 0) as i32) == major {
                    cr.move_to(0.0, y as f64 + 0.5);
                    cr.line_to(width as f64, y as f64 + 0.5);
                }
                y += 16;
            }
            cr.stroke()?;
        }
        Ok(())
    }

    /// v1 rows remain pixel-identical at 300px; v2 elements use logical coordinates.
    pub fn widget(
        &self,
        cr: &Context,
        config: &crate::config::PluginConfig,
        frame: &crate::plugin::Frame,
    ) -> Result<(), cairo::Error> {
        cr.save()?;
        cr.rectangle(0.0, 0.0, config.width as f64, config.height as f64);
        cr.clip();
        let card = config.decoration == crate::config::Decoration::Card;
        if card || !frame.rows.is_empty() {
            cr.save()?;
            cr.scale(config.width as f64 / WIDTH, 1.0);
            self.plugin_content(cr, config.height, frame, card)?;
            cr.restore()?;
        }
        self.canvas.draw(
            cr,
            &frame.elements,
            &config.asset_dir,
            self.palette,
            &self.fonts[0],
        )?;
        cr.restore()?;
        Ok(())
    }

    /// Shared presentation keeps plugins independent of Cairo, fonts and Wayland.
    pub fn plugin(
        &self,
        cr: &Context,
        height: i32,
        frame: &crate::plugin::Frame,
    ) -> Result<(), cairo::Error> {
        self.plugin_content(cr, height, frame, true)
    }

    fn plugin_content(
        &self,
        cr: &Context,
        height: i32,
        frame: &crate::plugin::Frame,
        card: bool,
    ) -> Result<(), cairo::Error> {
        cr.save()?;
        cr.rectangle(0.0, 0.0, WIDTH, height as f64);
        cr.clip();
        if card {
            self.card(cr, 0, height);
        }
        let layout = create_layout(cr);
        layout.set_single_paragraph_mode(true);
        layout.set_ellipsize(pango::EllipsizeMode::End);
        layout.set_height(-1);
        if card {
            self.text(
                cr,
                &layout,
                &frame.title,
                24.0,
                20.0,
                16,
                self.palette.muted,
                224,
                Alignment::Left,
            );
        }
        let mut y = if card { 50.0 } else { 0.0 };
        for row in &frame.rows {
            let row_height = 26.0
                + if row.fraction.is_some() { 8.0 } else { 0.0 }
                + if row.history.is_empty() { 0.0 } else { 34.0 };
            if y + row_height > height as f64 - 12.0 {
                break;
            }
            if row.label.is_empty() {
                self.text(
                    cr,
                    &layout,
                    &row.value,
                    24.0,
                    y,
                    16,
                    self.palette.foreground,
                    252,
                    Alignment::Left,
                );
            } else {
                self.text(
                    cr,
                    &layout,
                    &row.label,
                    24.0,
                    y,
                    16,
                    self.palette.foreground,
                    112,
                    Alignment::Left,
                );
                self.text(
                    cr,
                    &layout,
                    &row.value,
                    140.0,
                    y,
                    16,
                    self.palette.accent,
                    136,
                    Alignment::Right,
                );
            }
            y += 26.0;
            if let Some(fraction) = row.fraction {
                self.bar(cr, y - 2.0, fraction, self.palette.accent);
                y += 8.0;
            }
            if !row.history.is_empty() {
                // Keep the chart on the card background, with a quiet fixed
                // 0..100% grid rather than an opaque stripe behind the data.
                set_color(cr, self.palette.muted, 0.16);
                cr.set_line_width(1.0);
                for offset in [0.5, 12.5, 25.5] {
                    cr.move_to(24.5, y + offset);
                    cr.line_to(275.5, y + offset);
                }
                cr.stroke()?;
                let denominator = row.history.len().saturating_sub(1).max(1) as f64;
                cr.move_to(24.5, y + 25.5);
                for (i, value) in row.history.iter().enumerate() {
                    cr.line_to(
                        24.5 + i as f64 * 251.0 / denominator,
                        y + 25.5 - value * 25.0,
                    );
                }
                cr.line_to(
                    24.5 + (row.history.len() - 1) as f64 * 251.0 / denominator,
                    y + 25.5,
                );
                cr.close_path();
                set_color(cr, self.palette.accent, 0.12);
                cr.fill()?;
                set_color(cr, self.palette.accent, 1.0);
                cr.set_line_width(1.25);
                for (i, value) in row.history.iter().enumerate() {
                    let x = 24.5 + i as f64 * 251.0 / denominator;
                    let py = y + 25.5 - value * 25.0;
                    if i == 0 {
                        cr.move_to(x, py);
                    } else {
                        cr.line_to(x, py);
                    }
                }
                cr.stroke()?;
                y += 34.0;
            }
        }
        cr.restore()?;
        Ok(())
    }

    fn card(&self, cr: &Context, y: i32, h: i32) {
        self.box_fill(
            cr,
            0.0,
            y as f64,
            WIDTH,
            h as f64,
            self.palette.background,
            self.background_opacity,
        );
        set_color(cr, self.palette.border, 1.0);
        cr.set_line_width(1.0);
        cr.rectangle(0.5, y as f64 + 0.5, WIDTH - 1.0, h as f64 - 1.0);
        let _ = cr.stroke();
        self.box_fill(
            cr,
            0.0,
            y as f64 + 18.0,
            2.0,
            16.0,
            self.palette.accent,
            1.0,
        );
        for row in 0..3 {
            self.box_fill(
                cr,
                268.0,
                y as f64 + 22.0 + row as f64 * 4.0,
                10.0,
                1.0,
                self.palette.muted,
                0.7,
            );
        }
    }
    fn bar(&self, cr: &Context, y: f64, fraction: f64, color: u32) {
        self.box_fill(cr, 24.0, y, 252.0, 3.0, self.palette.bar_background, 1.0);
        self.box_fill(
            cr,
            24.0,
            y,
            252.0 * fraction.clamp(0.0, 1.0),
            3.0,
            color,
            1.0,
        );
    }
    #[allow(clippy::too_many_arguments)] // Drawing primitives keep their coordinates explicit.
    fn box_fill(&self, cr: &Context, x: f64, y: f64, w: f64, h: f64, color: u32, alpha: f64) {
        set_color(cr, color, alpha);
        cr.rectangle(x, y, w, h);
        let _ = cr.fill();
    }
    #[allow(clippy::too_many_arguments)] // Mirrors Pango placement and styling in the reference.
    fn text(
        &self,
        cr: &Context,
        layout: &Layout,
        text: &str,
        x: f64,
        y: f64,
        size: i32,
        color: u32,
        width: i32,
        alignment: Alignment,
    ) {
        layout.set_font_description(Some(&self.fonts[usize::from(size == 64)]));
        layout.set_text(text);
        layout.set_width(if width > 0 { width * pango::SCALE } else { -1 });
        layout.set_alignment(alignment);
        set_color(cr, color, 1.0);
        cr.move_to(x, y);
        show_layout(cr, layout);
    }
}

pub fn clear(cr: &Context) -> Result<(), cairo::Error> {
    cr.save()?;
    cr.set_operator(cairo::Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    cr.paint()?;
    cr.set_operator(cairo::Operator::Over);
    cr.restore()?;
    Ok(())
}

impl Palette {
    pub const DEFAULT: Self = Self {
        background: 0x282828,
        foreground: 0xd4be98,
        muted: 0xa89984,
        accent: 0xe78a4e,
        green: 0xa9b665,
        border: 0x504945,
        bar_background: 0x3c3836,
        grid_background: 0x1b1b1b,
    };
}

pub(crate) fn set_color(cr: &Context, hex: u32, alpha: f64) {
    cr.set_source_rgba(
        ((hex >> 16) & 255) as f64 / 255.0,
        ((hex >> 8) & 255) as f64 / 255.0,
        (hex & 255) as f64 / 255.0,
        alpha,
    );
}
