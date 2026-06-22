//! Output folder helpers — mirrors Civil3D `Documents/HydroComplete`.

use std::path::{Path, PathBuf};

/// `Documents/HydroComplete` (Windows: `%USERPROFILE%/Documents/HydroComplete`).
pub fn output_folder() -> PathBuf {
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        return PathBuf::from(home).join("Documents").join("HydroComplete");
    }
    PathBuf::from("HydroComplete")
}

pub fn sanitize_file_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "drawing".into();
    }
    trimmed
        .chars()
        .map(|c| {
            if r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// `report-{drawing}-{timestamp}.{extension}` under [`output_folder`].
pub fn build_report_path(drawing_name: &str, extension: &str) -> PathBuf {
    let folder = output_folder();
    std::fs::create_dir_all(&folder).ok();
    let stamp = chrono_like_stamp();
    let safe = sanitize_file_name(drawing_name);
    folder.join(format!("report-{safe}-{stamp}.{extension}"))
}

/// Write bytes to path, creating parent directories.
pub fn write_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

fn chrono_like_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn trim_label(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}~", &s[..max.saturating_sub(1)])
    }
}