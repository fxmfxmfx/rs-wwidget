use cairo::{Context, Format, ImageSurface};
use image::ImageFormat;
use rs_wwidget::{
    config::Palette,
    locale::Language,
    metrics::{Date, Display},
    render::{Renderer, clear},
};
use std::process::Command;

fn reference(kind: &str, mode: usize, scale: usize, index: usize) -> Vec<u8> {
    static BINARY: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let binary = BINARY.get_or_init(|| {
        let dir =
            std::env::temp_dir().join(format!("widget-render-reference-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("reference");
        let flags = String::from_utf8(
            Command::new("pkg-config")
                .args(["--cflags", "--libs", "cairo", "pangocairo"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let status = Command::new("cc")
            .args([
                "-std=c11",
                "-O2",
                "-Wall",
                "-Wextra",
                "-Werror",
                "tests/reference/render_reference.c",
                "-o",
            ])
            .arg(&binary)
            .args(flags.split_whitespace())
            .status()
            .unwrap();
        assert!(status.success(), "could not build frozen C renderer");
        binary
    });
    let output = Command::new(binary)
        .args([
            kind,
            &mode.to_string(),
            &scale.to_string(),
            &index.to_string(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "reference renderer failed");
    output.stdout
}

fn date(mode: usize) -> Date {
    if mode == 2 {
        Date {
            year: 2024,
            month: 2,
            day: 29,
            weekday: 4,
            hour: 23,
            minute: 59,
        }
    } else {
        Date {
            year: 2026,
            month: 10,
            day: 19,
            weekday: 1,
            hour: 1,
            minute: 23,
        }
    }
}
fn display(mode: usize) -> Display {
    Display {
        date: date(mode),
        cpu: (mode != 1).then_some(12.345),
        memory: (mode != 1).then_some((5.2, 15.5)),
        disk: (mode != 1).then_some((132.4, 0.321)),
    }
}
fn png(surface: &mut ImageSurface) -> Vec<u8> {
    let mut bytes = Vec::new();
    surface.write_to_png(&mut bytes).unwrap();
    image::load_from_memory_with_format(&bytes, ImageFormat::Png)
        .unwrap()
        .to_rgba8()
        .into_raw()
}
fn reference_pixels(kind: &str, mode: usize, scale: usize, index: usize) -> Vec<u8> {
    image::load_from_memory_with_format(&reference(kind, mode, scale, index), ImageFormat::Png)
        .unwrap()
        .to_rgba8()
        .into_raw()
}
fn render_panel(mode: usize, scale: usize, index: usize) -> Vec<u8> {
    let heights = [144, 274, 178];
    let mut surface = ImageSurface::create(
        Format::ARgb32,
        300 * scale as i32,
        heights[index] * scale as i32,
    )
    .unwrap();
    surface.set_device_scale(scale as f64, scale as f64);
    let cr = Context::new(&surface).unwrap();
    Renderer::new(Palette::DEFAULT, 0.96)
        .with_language(Language::Ru)
        .panel(&cr, index, &display(mode))
        .unwrap();
    drop(cr);
    png(&mut surface)
}

#[test]
fn panels_match_frozen_legacy_pixels() {
    for mode in 0..3 {
        for scale in 1..=2 {
            for index in 0..3 {
                assert_eq!(
                    render_panel(mode, scale, index),
                    reference_pixels("panel", mode, scale, index),
                    "panel={index}, fixture={mode}, scale={scale}"
                );
            }
        }
    }
}

#[test]
fn plugin_content_is_clipped_and_scales_with_its_panel() {
    use rs_wwidget::plugin::{Frame, Row};
    let frame = Frame {
        elements: Vec::new(),
        version: 1,
        title: "Очень длинный заголовок ".repeat(4),
        rows: vec![
            Row {
                label: "Label ".repeat(50),
                value: "Значение ".repeat(40),
                fraction: Some(0.5),
                history: vec![0.0, 0.5, 1.0, 0.25],
            };
            12
        ],
    };
    for scale in [1, 2] {
        let mut surface = ImageSurface::create(Format::ARgb32, 320 * scale, 198 * scale).unwrap();
        surface.set_device_scale(scale as f64, scale as f64);
        let cr = Context::new(&surface).unwrap();
        cr.translate(10.0, 10.0);
        Renderer::new(Palette::DEFAULT, 1.0)
            .plugin(&cr, 178, &frame)
            .unwrap();
        drop(cr);
        let pixels = png(&mut surface);
        let width = 320 * scale as usize;
        for y in 0..198 * scale as usize {
            for x in 0..width {
                let alpha = pixels[(y * width + x) * 4 + 3];
                if x < 10 * scale as usize
                    || x >= 310 * scale as usize
                    || y < 10 * scale as usize
                    || y >= 188 * scale as usize
                {
                    assert_eq!(alpha, 0, "plugin painted outside its panel at {x},{y}");
                } else {
                    assert_eq!(alpha, 255, "opaque plugin panel has a gap at {x},{y}");
                }
            }
        }
    }
}

#[test]
fn grid_and_panel_match_frozen_legacy_pixels() {
    for scale in 1..=2 {
        let mut surface = ImageSurface::create(Format::ARgb32, 960 * scale, 640 * scale).unwrap();
        surface.set_device_scale(scale as f64, scale as f64);
        let cr = Context::new(&surface).unwrap();
        clear(&cr).unwrap();
        let renderer = Renderer::new(Palette::DEFAULT, 0.96).with_language(Language::Ru);
        renderer.grid(&cr, 960, 640).unwrap();
        cr.save().unwrap();
        cr.translate(320.0, 64.0);
        renderer.panel(&cr, 1, &display(0)).unwrap();
        cr.restore().unwrap();
        cr.save().unwrap();
        set_source(&cr, Palette::DEFAULT.accent);
        cr.set_line_width(2.0);
        cr.rectangle(321.0, 65.0, 298.0, 272.0);
        cr.stroke().unwrap();
        cr.restore().unwrap();
        drop(cr);
        assert_eq!(
            png(&mut surface),
            reference_pixels("grid", 0, scale as usize, 1),
            "grid scale={scale}"
        );
    }
}

#[test]
fn clipped_drag_redraw_matches_full_reference_each_step() {
    let steps = [
        (320, 64),
        (320, 64),
        (336, 80),
        (0, 48),
        (640, 256),
        (16, 320),
    ];
    for scale in 1..=2 {
        let mut surface = ImageSurface::create(Format::ARgb32, 960 * scale, 640 * scale).unwrap();
        surface.set_device_scale(scale as f64, scale as f64);
        let cr = Context::new(&surface).unwrap();
        let renderer = Renderer::new(Palette::DEFAULT, 0.96).with_language(Language::Ru);
        for (step, &(x, y)) in steps.iter().enumerate() {
            let mut expected =
                ImageSurface::create(Format::ARgb32, 960 * scale, 640 * scale).unwrap();
            expected.set_device_scale(scale as f64, scale as f64);
            let ecr = Context::new(&expected).unwrap();
            full_grid_frame(&renderer, &ecr, x, y);
            drop(ecr);
            cr.save().unwrap();
            if step > 0 {
                let (ox, oy) = steps[step - 1];
                let (ax, ay, aw, ah) = damage(ox, oy, 274, 960, 640);
                let (bx, by, bw, bh) = damage(x, y, 274, 960, 640);
                cr.rectangle(ax as f64, ay as f64, aw as f64, ah as f64);
                cr.rectangle(bx as f64, by as f64, bw as f64, bh as f64);
                cr.clip();
            }
            full_grid_frame(&renderer, &cr, x, y);
            cr.restore().unwrap();
            assert_eq!(
                png(&mut surface),
                png(&mut expected),
                "drag step={step}, scale={scale}"
            );
        }
    }
}

fn damage(x: i32, y: i32, height: i32, width: i32, screen_height: i32) -> (i32, i32, i32, i32) {
    let left = (x - 1).max(0);
    let top = (y - 1).max(0);
    let right = (x + 300 + 1).min(width);
    let bottom = (y + height + 1).min(screen_height);
    (left, top, (right - left).max(0), (bottom - top).max(0))
}

fn full_grid_frame(renderer: &Renderer, cr: &Context, x: i32, y: i32) {
    clear(cr).unwrap();
    renderer.grid(cr, 960, 640).unwrap();
    cr.save().unwrap();
    cr.translate(x as f64, y as f64);
    renderer.panel(cr, 1, &display(0)).unwrap();
    cr.restore().unwrap();
    cr.save().unwrap();
    set_source(cr, Palette::DEFAULT.accent);
    cr.set_line_width(2.0);
    cr.rectangle(x as f64 + 1.0, y as f64 + 1.0, 298.0, 272.0);
    cr.stroke().unwrap();
    cr.restore().unwrap();
}

#[test]
fn custom_palette_changes_panel_pixels() {
    let mut custom = Palette::DEFAULT;
    custom.background = 0x123456;
    custom.foreground = 0xfedcba;
    let mut surface = ImageSurface::create(Format::ARgb32, 300, 144).unwrap();
    let cr = Context::new(&surface).unwrap();
    Renderer::new(custom, 0.96)
        .with_language(Language::Ru)
        .panel(&cr, 0, &display(0))
        .unwrap();
    drop(cr);
    assert_ne!(png(&mut surface), reference_pixels("panel", 0, 1, 0));
}

#[test]
fn background_opacity_changes_fill_without_fading_text() {
    let render = |opacity| {
        let mut surface = ImageSurface::create(Format::ARgb32, 300, 144).unwrap();
        let cr = Context::new(&surface).unwrap();
        Renderer::new(Palette::DEFAULT, opacity)
            .with_language(Language::Ru)
            .panel(&cr, 0, &display(0))
            .unwrap();
        drop(cr);
        png(&mut surface)
    };
    let original = render(0.96);
    for (opacity, alpha) in [(0.0, 0), (0.5, 128), (1.0, 255)] {
        let pixels = render(opacity);
        let background = (140 * 300 + 150) * 4;
        assert_eq!(pixels[background + 3], alpha, "opacity={opacity}");
        let mut text_pixels = 0;
        for (before, after) in original
            .as_chunks::<4>()
            .0
            .iter()
            .zip(pixels.as_chunks::<4>().0.iter())
        {
            if *before == [0xd4, 0xbe, 0x98, 255] {
                assert_eq!(
                    after, before,
                    "opaque text must not fade with the card background"
                );
                text_pixels += 1;
            }
        }
        assert!(text_pixels > 0, "fixture must include foreground text");
    }
}
fn set_source(cr: &Context, color: u32) {
    cr.set_source_rgb(
        ((color >> 16) & 255) as f64 / 255.0,
        ((color >> 8) & 255) as f64 / 255.0,
        (color & 255) as f64 / 255.0,
    );
}

#[test]
fn english_is_the_default_and_both_languages_preserve_panel_geometry() {
    for (index, height) in [144, 274, 178].into_iter().enumerate() {
        let render = |language| {
            let mut surface = ImageSurface::create(Format::ARgb32, 300, height).unwrap();
            let cr = Context::new(&surface).unwrap();
            let renderer = Renderer::new(Palette::DEFAULT, 0.96);
            let renderer = if let Some(language) = language {
                renderer.with_language(language)
            } else {
                renderer
            };
            renderer.panel(&cr, index, &display(0)).unwrap();
            drop(cr);
            png(&mut surface)
        };
        let english = render(None);
        assert_eq!(english, render(Some(Language::En)));
        let russian = render(Some(Language::Ru));
        assert_ne!(english, russian, "panel {index} must use translated labels");
        let corner = (2 * 300 + 2) * 4;
        assert_eq!(&english[corner..corner + 4], &russian[corner..corner + 4]);
    }
}
