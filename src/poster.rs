//! Native poster authoring. Generated packages remain ordinary static API v2 widgets.
use crate::{Result, canvas, config, plugin::Frame};
use serde_json::json;
use std::{fs, path::Path};

const CREATE_USAGE: &str = "--create-poster IMAGE PACKAGE_DIR [--width N] [--height N] [--rotation DEG] [--fit contain|cover|stretch] [--opacity N] [--radius N] [--input none|title|all] [--filter nearest|bilinear|best]";

/// Canvas dimensions and centered unrotated image bounds. Avoid floating point
/// ceil artifacts at right angles, and keep a minimum legal 32px panel size.
fn geometry(w: f64, h: f64, angle: f64) -> Result<(i32, i32, [f64; 4])> {
    if !angle.is_finite()
        || !(-360.0..=360.0).contains(&angle)
        || ![w, h]
            .iter()
            .all(|n| n.is_finite() && (32.0..=8192.0).contains(n))
    {
        return Err("poster size must be 32..8192 and rotation -360..360 degrees".into());
    }
    let (sin, cos) = angle.to_radians().sin_cos();
    let width = ((w * cos.abs() + h * sin.abs() - 1e-9).ceil() as i32).max(32);
    let height = ((w * sin.abs() + h * cos.abs() - 1e-9).ceil() as i32).max(32);
    if width > 8192 || height > 8192 {
        return Err("rotated poster exceeds 8192px; reduce --width or --height".into());
    }
    Ok((
        width,
        height,
        [(width as f64 - w) / 2.0, (height as f64 - h) / 2.0, w, h],
    ))
}

pub fn create(args: &[String]) -> Result<()> {
    if args.len() < 2 || !(args.len() - 2).is_multiple_of(2) {
        return Err(CREATE_USAGE.into());
    }
    let source = Path::new(&args[0]);
    let directory = Path::new(&args[1]);
    let id = directory
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|id| crate::registry::valid_id(id) && !crate::layout::NAMES.contains(id))
        .ok_or("package directory must have a valid widget ID, excluding built-in names")?;
    let (mut width, mut height) = (None, None);
    let (mut rotation, mut opacity, mut radius) = (0.0_f64, 1.0_f64, 0.0_f64);
    let (mut fit, mut input, mut filter) = ("contain", "none", "bilinear");
    let mut seen = std::collections::BTreeSet::new();
    for pair in args[2..].chunks_exact(2) {
        if !seen.insert(pair[0].as_str()) {
            return Err(format!("duplicate option: {}", pair[0]).into());
        }
        match pair[0].as_str() {
            "--width" => width = Some(pair[1].parse::<f64>()?),
            "--height" => height = Some(pair[1].parse::<f64>()?),
            "--rotation" => rotation = pair[1].parse()?,
            "--opacity" => opacity = pair[1].parse()?,
            "--radius" => radius = pair[1].parse()?,
            "--fit" => fit = &pair[1],
            "--input" => input = &pair[1],
            "--filter" => filter = &pair[1],
            _ => return Err(format!("unknown poster option: {}\n{CREATE_USAGE}", pair[0]).into()),
        }
    }
    if !["none", "all", "title"].contains(&input) {
        return Err("input must be none, title or all".into());
    }
    let bytes = canvas::read_image(source)?;
    let image = canvas::decode_image(&bytes)?;
    let aspect = image.width() as f64 / image.height() as f64;
    let (w, h) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, (w / aspect).round()),
        (None, Some(h)) => ((h * aspect).round(), h),
        (None, None) => (360.0, (360.0 / aspect).round()),
    };
    let (panel_w, panel_h, bounds) = geometry(w, h, rotation)?;
    let image_name = match image::guess_format(&bytes)? {
        image::ImageFormat::Png => "poster.png",
        image::ImageFormat::Jpeg => "poster.jpg",
        _ => return Err("poster supports PNG and JPEG".into()),
    };
    let frame = serde_json::to_vec_pretty(&json!({"version":2,"elements":[{
        "bounds":bounds,"rotation":rotation,"opacity":opacity,"clip_radius":radius,
        "content":{"type":"image","path":image_name,"fit":fit,"filter":filter}
    }]}))?;
    Frame::parse(&frame)?;
    let manifest = format!(
        "version = 2\nid = \"{id}\"\nprotocol = 2\nframe = \"frame.json\"\nwidth = {panel_w}\nheight = {panel_h}\ndecoration = \"none\"\ninput = \"{input}\"\n"
    );
    // All validation happens before creating anything. Never replace a package.
    fs::create_dir(directory).map_err(|e| {
        format!(
            "{}: {e} (parent must exist; package must be new)",
            directory.display()
        )
    })?;
    let result = (|| -> Result<()> {
        fs::write(directory.join(image_name), &bytes)?;
        fs::write(directory.join("frame.json"), &frame)?;
        // Write the discoverable manifest last.
        fs::write(directory.join("widget.toml"), manifest)?;
        Ok(())
    })();
    if result.is_err() {
        for name in [image_name, "frame.json", "widget.toml"] {
            let _ = fs::remove_file(directory.join(name));
        }
        let _ = fs::remove_dir(directory);
    }
    result?;
    println!(
        "Created poster '{id}' ({}x{}, rotation {} degrees): {}",
        panel_w,
        panel_h,
        rotation,
        directory.display()
    );
    println!("Add \"{id}\" to widgets in config.toml, then restart rs-wwidget.");
    Ok(())
}

pub fn rotate(directory: &Path, angle: f64) -> Result<()> {
    let directory = fs::canonicalize(directory)?;
    let manifest_path = directory.join("widget.toml");
    let manifest = config::read_text(&manifest_path, 65_536)?;
    let mut fields: toml::Table = toml::from_str(&manifest)?;
    let mut definition = fields.clone();
    let version = definition.remove("version").and_then(|v| v.as_integer());
    if !matches!(version, Some(1 | 2)) || definition.contains_key("enabled") {
        return Err("invalid poster manifest version or enabled field".into());
    }
    let plugin: config::PluginConfig = toml::Value::Table(definition).try_into()?;
    plugin.validate()?;
    if Some(plugin.id.as_str()) != directory.file_name().and_then(|s| s.to_str())
        || !crate::registry::valid_id(&plugin.id)
        || crate::layout::NAMES.contains(&plugin.id.as_str())
    {
        return Err("poster id must match its package directory and not a built-in name".into());
    }
    if fields.contains_key("command")
        || fields.get("decoration").and_then(|v| v.as_str()) != Some("none")
    {
        return Err("rotate-poster requires a static, decoration-free image package".into());
    }
    let frame_name = fields
        .get("frame")
        .and_then(|v| v.as_str())
        .ok_or("poster manifest needs frame")?;
    let frame_path = fs::canonicalize(directory.join(frame_name))?;
    if !frame_path.starts_with(&directory) || frame_path == manifest_path {
        return Err("poster frame must be a separate file inside the package directory".into());
    }
    use std::io::Read;
    let mut original_frame = Vec::new();
    fs::File::open(&frame_path)?
        .take(65_537)
        .read_to_end(&mut original_frame)?;
    let mut frame = Frame::parse(&original_frame)?;
    if frame.version != 2
        || frame.elements.len() != 1
        || !frame.rows.is_empty()
        || !matches!(frame.elements[0].content, canvas::Content::Image { .. })
    {
        return Err("rotate-poster needs a v2 frame with exactly one image and no rows".into());
    }
    let element = &mut frame.elements[0];
    let (width, height, bounds) = geometry(element.bounds[2], element.bounds[3], angle)?;
    element.rotation = angle;
    element.origin = [0.5, 0.5];
    element.bounds = bounds;
    let updated_frame = serde_json::to_vec_pretty(&frame)?;
    Frame::parse(&updated_frame)?;
    fields.insert("width".into(), toml::Value::Integer(width.into()));
    fields.insert("height".into(), toml::Value::Integer(height.into()));
    let updated_manifest = toml::to_string_pretty(&fields)?;
    config::atomic_write(&frame_path, &updated_frame)?;
    if let Err(error) = config::atomic_write(&manifest_path, updated_manifest.as_bytes()) {
        if let Err(rollback) = config::atomic_write(&frame_path, &original_frame) {
            return Err(format!(
                "manifest update failed: {error}; frame rollback failed: {rollback}"
            )
            .into());
        }
        return Err(error.into());
    }
    println!(
        "Rotated poster to {angle} degrees ({}x{}). Restart rs-wwidget to apply.",
        width, height
    );
    Ok(())
}
