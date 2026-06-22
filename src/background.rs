//! `HC_BACKGROUND` — attach georeferenced raster on layer `HC-BACKGROUND`.

use std::path::Path;

use acadrust::entities::{ImageDisplayFlags, RasterImage};
use acadrust::types::Vector3;
use acadrust::EntityType;
use ocs_plugin_api::host::HostApi;

pub const LAYER_NAME: &str = "HC-BACKGROUND";

pub fn usage() -> &'static str {
    "HC_BACKGROUND <image-path>  — click insertion point and width\n  HC_BACKGROUND <path> <x> <y> <width_ft>"
}

/// Read pixel dimensions from PNG, JPEG, GIF, or BMP headers.
pub fn image_pixel_size(path: &Path) -> Result<(f64, f64), String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read image: {e}"))?;
    if data.len() >= 24 && data.starts_with(&[0x89, b'P', b'N', b'G']) {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as f64;
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as f64;
        return Ok((w.max(1.0), h.max(1.0)));
    }
    if data.len() >= 26 && data.starts_with(&[b'B', b'M']) {
        let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]).unsigned_abs() as f64;
        let h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]).unsigned_abs() as f64;
        return Ok((w.max(1.0), h.max(1.0)));
    }
    if data.len() >= 10 && data.starts_with(&[b'G', b'I', b'F']) {
        let w = u16::from_le_bytes([data[6], data[7]]) as f64;
        let h = u16::from_le_bytes([data[8], data[9]]) as f64;
        return Ok((w.max(1.0), h.max(1.0)));
    }
    // JPEG: scan for SOF marker
    if data.len() > 4 && data.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2usize;
        while i + 9 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            if matches!(marker, 0xC0 | 0xC1 | 0xC2) {
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as f64;
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as f64;
                return Ok((w.max(1.0), h.max(1.0)));
            }
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            i += 2 + len;
        }
    }
    Ok((1920.0, 1080.0))
}

pub fn attach_image(
    image_path: &str,
    insertion: [f64; 3],
    width_drawing_units: f64,
) -> Result<EntityType, String> {
    let path = Path::new(image_path);
    if !path.is_file() {
        return Err(format!("File not found: {image_path}"));
    }
    if width_drawing_units <= 0.0 {
        return Err("Width must be positive.".into());
    }
    let full_path = std::fs::canonicalize(path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| image_path.to_string());
    let (px_w, px_h) = image_pixel_size(path)?;
    let scale = width_drawing_units / px_w.max(1e-6);
    let world_height = px_h * scale;

    let mut img = RasterImage::with_size(
        &full_path,
        Vector3::new(insertion[0], insertion[1], insertion[2]),
        px_w,
        px_h,
        width_drawing_units,
        world_height,
    );
    img.flags = ImageDisplayFlags::SHOW_IMAGE | ImageDisplayFlags::USE_CLIPPING_BOUNDARY;
    let mut ent = EntityType::RasterImage(img);
    ent.common_mut().layer = LAYER_NAME.to_string();
    Ok(ent)
}

pub fn attach_direct(
    host: &mut dyn HostApi,
    image_path: &str,
    x: f64,
    y: f64,
    width: f64,
) -> Result<String, String> {
    host.push_undo("HC_BACKGROUND");
    let ent = attach_image(image_path, [x, y, 0.0], width)?;
    host.add_entity(ent);
    host.bump_geometry();
    host.set_dirty();
    Ok(format!(
        "--- HydroComplete: background image ---\n  File: {image_path}\n  Layer: {LAYER_NAME}\n  Width: {width:.2} drawing units"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_header_dimensions() {
        // minimal 1x1 PNG
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x08, 0x06, 0x00, 0x00,
            0x00,
        ];
        let dir = std::env::temp_dir().join("hc-bg-test.png");
        std::fs::write(&dir, png).unwrap();
        let (w, h) = image_pixel_size(&dir).unwrap();
        assert_eq!(w, 2.0);
        assert_eq!(h, 3.0);
        let _ = std::fs::remove_file(dir);
    }
}