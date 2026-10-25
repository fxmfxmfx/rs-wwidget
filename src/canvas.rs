//! Protocol v2: bounded display lists, rendered in logical panel coordinates.
use crate::{Result, config::Palette};
use cairo::{Context, Format, ImageSurface};
use pangocairo::functions::{create_layout, show_layout};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::VecDeque,
    path::{Path, PathBuf},
};

fn one() -> f64 {
    1.0
}
fn center() -> [f64; 2] {
    [0.5, 0.5]
}
fn font_size() -> f64 {
    16.0
}
fn foreground() -> String {
    "foreground".into()
}
fn accent() -> String {
    "accent".into()
}
fn background() -> String {
    "bar_background".into()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Element {
    /// [x, y, width, height], in logical pixels. Each element clips to its box.
    pub bounds: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default = "one")]
    pub opacity: f64,
    /// Clockwise degrees, around the normalized origin within bounds.
    #[serde(default)]
    pub rotation: f64,
    #[serde(default = "center")]
    pub origin: [f64; 2],
    #[serde(default)]
    pub clip_radius: f64,
    pub content: Content,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    #[default]
    Contain,
    Cover,
    Stretch,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Sampling {
    Nearest,
    #[default]
    Bilinear,
    Best,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Content {
    Text {
        text: String,
        #[serde(default = "font_size")]
        size: f64,
        #[serde(default = "foreground")]
        color: String,
        #[serde(default)]
        align: Align,
        #[serde(default)]
        wrap: bool,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        italic: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font: Option<String>,
    },
    Image {
        path: String,
        #[serde(default)]
        fit: Fit,
        /// Bump after overwriting an image at the same path to invalidate its cache.
        #[serde(default)]
        revision: u64,
        /// Normalized image position within the box (also controls cover cropping).
        #[serde(default = "center")]
        position: [f64; 2],
        #[serde(default)]
        filter: Sampling,
    },
    Rect {
        #[serde(default = "accent")]
        color: String,
        #[serde(default)]
        radius: f64,
    },
    Line {
        #[serde(default = "accent")]
        color: String,
        #[serde(default = "one")]
        thickness: f64,
    },
    Bar {
        value: f64,
        #[serde(default = "accent")]
        color: String,
        #[serde(default = "background")]
        background: String,
        #[serde(default)]
        radius: f64,
    },
    Graph {
        values: Vec<f64>,
        #[serde(default = "accent")]
        color: String,
        #[serde(default)]
        fill: bool,
        #[serde(default = "one")]
        thickness: f64,
    },
}

pub fn color(value: &str, p: Palette) -> Result<(u32, f64)> {
    let named = match value {
        "background" => Some(p.background),
        "foreground" => Some(p.foreground),
        "muted" => Some(p.muted),
        "accent" => Some(p.accent),
        "green" => Some(p.green),
        "border" => Some(p.border),
        "bar_background" => Some(p.bar_background),
        "grid_background" => Some(p.grid_background),
        "transparent" => return Ok((0, 0.0)),
        _ => None,
    };
    if let Some(rgb) = named {
        return Ok((rgb, 1.0));
    }
    let hex = value
        .strip_prefix('#')
        .ok_or("expected palette name, #RRGGBB or #RRGGBBAA")?;
    if ![6, 8].contains(&hex.len()) || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected palette name, #RRGGBB or #RRGGBBAA".into());
    }
    Ok((
        u32::from_str_radix(&hex[..6], 16)?,
        if hex.len() == 8 {
            u8::from_str_radix(&hex[6..], 16)? as f64 / 255.0
        } else {
            1.0
        },
    ))
}
fn unit(x: f64) -> bool {
    x.is_finite() && (0.0..=1.0).contains(&x)
}
impl Element {
    /// Inverse-transform a pointer into the element's local, rounded clip.
    /// Image alpha is deliberately not a hit mask.
    pub fn hit(&self, point: (f64, f64)) -> Option<[f64; 2]> {
        if self.opacity <= 0.0 || !point.0.is_finite() || !point.1.is_finite() {
            return None;
        }
        let [x, y, w, h] = self.bounds;
        let [ox, oy] = self.origin;
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let (dx, dy) = (point.0 - x - w * ox, point.1 - y - h * oy);
        let (lx, ly) = (dx * cos + dy * sin + w * ox, -dx * sin + dy * cos + h * oy);
        if lx < 0.0 || ly < 0.0 || lx >= w || ly >= h {
            return None;
        }
        let radius = self.clip_radius.min(w / 2.0).min(h / 2.0);
        let (cx, cy) = (lx.clamp(radius, w - radius), ly.clamp(radius, h - radius));
        if (lx - cx).powi(2) + (ly - cy).powi(2) > radius * radius {
            return None;
        }
        Some([lx, ly])
    }

    pub fn validate(&self) -> Result<()> {
        let [x, y, w, h] = self.bounds;
        if self
            .id
            .as_ref()
            .is_some_and(|id| !crate::registry::valid_id(id))
        {
            return Err("element id must be 1..64 ASCII letters, digits, _ or -".into());
        }
        if ![x, y, w, h]
            .iter()
            .all(|n| n.is_finite() && n.abs() <= 8192.0)
            || w <= 0.0
            || h <= 0.0
            || !unit(self.opacity)
        {
            return Err("invalid element bounds/opacity (positive size, coordinates within 8192, opacity 0..1)".into());
        }
        if !self.rotation.is_finite()
            || !(-360.0..=360.0).contains(&self.rotation)
            || !self.origin.iter().all(|n| unit(*n))
            || !self.clip_radius.is_finite()
            || !(0.0..=2048.0).contains(&self.clip_radius)
        {
            return Err("rotation must be -360..360, origin 0..1, clip_radius 0..2048".into());
        }
        let check_color = |c: &str| color(c, Palette::DEFAULT).map(|_| ());
        let check_stroke = |n: f64| -> Result<()> {
            if !n.is_finite() || !(0.1..=128.0).contains(&n) {
                return Err("thickness must be 0.1..128".into());
            }
            Ok(())
        };
        let check_radius = |n: f64| -> Result<()> {
            if !n.is_finite() || !(0.0..=2048.0).contains(&n) {
                return Err("radius must be 0..2048".into());
            }
            Ok(())
        };
        match &self.content {
            Content::Text {
                text,
                size,
                color,
                font,
                ..
            } => {
                if text.chars().count() > 4096 || !size.is_finite() || !(1.0..=512.0).contains(size)
                {
                    return Err("text limit 4096 characters; size 1..512".into());
                }
                if font.as_ref().is_some_and(|font| {
                    font.trim().is_empty() || font.chars().count() > 128 || font.contains('\0')
                }) {
                    return Err(
                        "font family must be nonempty, at most 128 characters, without NUL".into(),
                    );
                }
                check_color(color)?;
            }
            Content::Image { path, position, .. } => {
                if !position.iter().all(|n| unit(*n)) {
                    return Err("image position must be 0..1".into());
                }
                if path.trim().is_empty() || path.len() > 4096 || path.contains('\0') {
                    return Err("invalid image path".into());
                }
            }
            Content::Rect { color, radius } => {
                check_color(color)?;
                check_radius(*radius)?;
            }
            Content::Line { color, thickness } => {
                check_color(color)?;
                check_stroke(*thickness)?;
            }
            Content::Bar {
                value,
                color,
                background,
                radius,
            } => {
                if !unit(*value) {
                    return Err("bar value must be 0..1".into());
                }
                check_color(color)?;
                check_color(background)?;
                check_radius(*radius)?;
            }
            Content::Graph {
                values,
                color,
                thickness,
                ..
            } => {
                if values.is_empty() || values.len() > 512 || !values.iter().all(|v| unit(*v)) {
                    return Err("graph needs 1..512 values in 0..1".into());
                }
                check_color(color)?;
                check_stroke(*thickness)?;
            }
        }
        Ok(())
    }
}

struct CachedImage {
    key: (PathBuf, u64),
    surface: Option<ImageSurface>,
    bytes: usize,
}
/// Negative lookups are cached too. No file reads or decoding on an unchanged frame.
#[derive(Default)]
pub struct Canvas {
    images: RefCell<VecDeque<CachedImage>>,
}
impl Canvas {
    pub fn clear_cache(&self) {
        self.images.borrow_mut().clear();
    }
    fn image(&self, path: PathBuf, revision: u64) -> Option<ImageSurface> {
        let mut images = self.images.borrow_mut();
        let key = (path, revision);
        if let Some(index) = images.iter().position(|i| i.key == key) {
            let entry = images.remove(index).unwrap();
            let surface = entry.surface.clone();
            images.push_back(entry);
            return surface;
        }
        let surface = match load_image(&key.0) {
            Ok(image) => Some(image),
            Err(e) => {
                eprintln!("widget image {}: {e}", key.0.display());
                None
            }
        };
        let bytes = surface
            .as_ref()
            .map_or(0, |s| s.stride() as usize * s.height() as usize);
        while images.len() >= 32
            || images.iter().map(|i| i.bytes).sum::<usize>() + bytes > 64 * 1024 * 1024
        {
            if images.pop_front().is_none() {
                break;
            }
        }
        images.push_back(CachedImage {
            key,
            surface: surface.clone(),
            bytes,
        });
        surface
    }

    pub fn draw(
        &self,
        cr: &Context,
        elements: &[Element],
        assets: &Path,
        palette: Palette,
        font: &pango::FontDescription,
    ) -> std::result::Result<(), cairo::Error> {
        for element in elements {
            cr.save()?;
            let [x, y, w, h] = element.bounds;
            cr.translate(x, y);
            if element.rotation != 0.0 {
                let [ox, oy] = element.origin;
                cr.translate(w * ox, h * oy);
                cr.rotate(element.rotation.to_radians());
                cr.translate(-w * ox, -h * oy);
            }
            rounded(cr, w, h, element.clip_radius);
            cr.clip();
            if element.opacity < 1.0 {
                cr.push_group();
            }
            let set = |c: &str| {
                let (rgb, alpha) = color(c, palette).expect("validated frame color");
                super::render::set_color(cr, rgb, alpha);
            };
            match &element.content {
                Content::Text {
                    text,
                    size,
                    color,
                    align,
                    wrap,
                    bold,
                    italic,
                    font: family,
                } => {
                    let layout = create_layout(cr);
                    let mut font = font.clone();
                    font.set_absolute_size(size * pango::SCALE as f64);
                    if let Some(family) = family {
                        font.set_family(family);
                    }
                    if *italic {
                        font.set_style(pango::Style::Italic);
                    }
                    if *bold {
                        font.set_weight(pango::Weight::Bold);
                    }
                    layout.set_font_description(Some(&font));
                    layout.set_text(text);
                    layout.set_width((w * pango::SCALE as f64) as i32);
                    layout.set_height((h * pango::SCALE as f64) as i32);
                    layout.set_single_paragraph_mode(!wrap);
                    layout.set_wrap(pango::WrapMode::WordChar);
                    layout.set_ellipsize(pango::EllipsizeMode::End);
                    layout.set_alignment(match align {
                        Align::Left => pango::Alignment::Left,
                        Align::Center => pango::Alignment::Center,
                        Align::Right => pango::Alignment::Right,
                    });
                    set(color);
                    cr.move_to(0.0, 0.0);
                    show_layout(cr, &layout);
                }
                Content::Image {
                    path,
                    fit,
                    revision,
                    position,
                    filter,
                } => {
                    if let Some(image) = self.image(assets.join(path), *revision) {
                        let (iw, ih) = (image.width() as f64, image.height() as f64);
                        let (sx, sy) = match fit {
                            Fit::Stretch => (w / iw, h / ih),
                            Fit::Contain => {
                                let s = (w / iw).min(h / ih);
                                (s, s)
                            }
                            Fit::Cover => {
                                let s = (w / iw).max(h / ih);
                                (s, s)
                            }
                        };
                        cr.translate((w - iw * sx) * position[0], (h - ih * sy) * position[1]);
                        cr.scale(sx, sy);
                        cr.set_source_surface(&image, 0.0, 0.0)?;
                        cr.source().set_extend(cairo::Extend::Pad);
                        cr.source().set_filter(match filter {
                            Sampling::Nearest => cairo::Filter::Nearest,
                            Sampling::Bilinear => cairo::Filter::Bilinear,
                            Sampling::Best => cairo::Filter::Best,
                        });
                        cr.rectangle(0.0, 0.0, iw, ih);
                        cr.fill()?;
                    }
                }
                Content::Rect { color, radius } => {
                    set(color);
                    rounded(cr, w, h, *radius);
                    cr.fill()?;
                }
                Content::Line { color, thickness } => {
                    set(color);
                    cr.set_line_width(*thickness);
                    cr.move_to(0.0, h);
                    cr.line_to(w, 0.0);
                    cr.stroke()?;
                }
                Content::Bar {
                    value,
                    color,
                    background,
                    radius,
                } => {
                    rounded(cr, w, h, *radius);
                    cr.clip();
                    set(background);
                    cr.paint()?;
                    set(color);
                    cr.rectangle(0.0, 0.0, w * value, h);
                    cr.fill()?;
                }
                Content::Graph {
                    values,
                    color,
                    fill,
                    thickness,
                } => {
                    set(color);
                    cr.set_line_width(*thickness);
                    let n = values.len().saturating_sub(1).max(1) as f64;
                    for (i, v) in values.iter().enumerate() {
                        let (px, py) = (
                            i as f64 / n * w,
                            (1.0 - v) * (h - thickness) + thickness / 2.0,
                        );
                        if i == 0 {
                            cr.move_to(px, py);
                        } else {
                            cr.line_to(px, py);
                        }
                    }
                    if values.len() == 1 {
                        cr.line_to(w, (1.0 - values[0]) * (h - thickness) + thickness / 2.0);
                    }
                    if *fill {
                        cr.line_to(w, h);
                        cr.line_to(0.0, h);
                        cr.close_path();
                        cr.fill()?;
                    } else {
                        cr.stroke()?;
                    }
                }
            }
            if element.opacity < 1.0 {
                cr.pop_group_to_source()?;
                cr.paint_with_alpha(element.opacity)?;
            }
            cr.restore()?;
        }
        Ok(())
    }
}
fn rounded(cr: &Context, w: f64, h: f64, radius: f64) {
    let r = radius.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    for (x, y, start) in [
        (w - r, r, -90.0_f64),
        (w - r, h - r, 0.0),
        (r, h - r, 90.0),
        (r, r, 180.0),
    ] {
        cr.arc(x, y, r, start.to_radians(), (start + 90.0).to_radians());
    }
    cr.close_path();
}
pub(crate) fn read_image(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut data = Vec::new();
    std::fs::File::open(path)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 32 * 1024 * 1024 {
        return Err("image file exceeds 32 MiB".into());
    }
    Ok(data)
}

pub(crate) fn decode_image(data: &[u8]) -> Result<image::RgbaImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?.to_rgba8())
}

fn load_image(path: &Path) -> Result<ImageSurface> {
    let image = decode_image(&read_image(path)?)?;
    let (w, h) = (image.width() as i32, image.height() as i32);
    let mut bytes = Vec::with_capacity(w as usize * h as usize * 4);
    for p in image.pixels() {
        let [r, g, b, a] = p.0;
        let premultiply = |v: u8| ((v as u32 * a as u32 + 127) / 255) as u8;
        bytes.extend_from_slice(
            &u32::from_be_bytes([a, premultiply(r), premultiply(g), premultiply(b)]).to_ne_bytes(),
        );
    }
    Ok(ImageSurface::create_for_data(
        bytes,
        Format::ARgb32,
        w,
        h,
        w * 4,
    )?)
}
