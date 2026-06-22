//! Per-drawing pipe design overrides (Q, Manning n) — mirrors `NetworkOverrideStore.cs`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use hydrocomplete::models::NetworkAnalysisPipe;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeOverride {
    pub pipe_key: String,
    pub pipe_name: String,
    pub network_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub design_flow_cfs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manning_n: Option<f64>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OverrideFile {
    pipes: Vec<PipeOverride>,
}

pub fn store_folder() -> PathBuf {
    if let Some(base) = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    {
        base.join("HydroComplete").join("overrides")
    } else {
        PathBuf::from(".hydrocomplete-overrides")
    }
}

pub fn file_path_for_drawing(drawing_path: &str) -> PathBuf {
    let key = if drawing_path.trim().is_empty() {
        "untitled".to_string()
    } else {
        drawing_path.trim().to_string()
    };
    let mut hasher = Sha256::new();
    hasher.update(key.to_lowercase().as_bytes());
    let hash = hasher.finalize();
    let id: String = hash.iter().take(8).map(|b| format!("{b:02x}")).collect();
    store_folder().join(format!("overrides-{id}.json"))
}

pub fn load(drawing_path: &str) -> Vec<PipeOverride> {
    let path = file_path_for_drawing(drawing_path);
    let Ok(json) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<OverrideFile>(&json)
        .map(|f| f.pipes)
        .unwrap_or_default()
}

pub fn save(drawing_path: &str, pipes: &[PipeOverride]) -> Result<PathBuf, String> {
    let folder = store_folder();
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = file_path_for_drawing(drawing_path);
    let file = OverrideFile {
        pipes: pipes.to_vec(),
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn apply_to_pipes(pipes: &mut [NetworkAnalysisPipe], overrides: &[PipeOverride]) {
    if overrides.is_empty() {
        return;
    }
    let mut by_key: HashMap<String, &PipeOverride> = HashMap::new();
    for o in overrides {
        if !o.pipe_key.trim().is_empty() {
            by_key.insert(o.pipe_key.to_lowercase(), o);
        }
    }
    for pipe in pipes.iter_mut() {
        let key = if pipe.pipe_name.is_empty() {
            pipe.pipe_key.clone()
        } else {
            pipe.pipe_name.clone()
        };
        let Some(o) = by_key.get(&key.to_lowercase()) else {
            continue;
        };
        if let Some(q) = o.design_flow_cfs.filter(|q| *q > 0.0) {
            pipe.segment.design_flow_cfs = q;
        }
        if let Some(n) = o.manning_n.filter(|n| *n > 0.0) {
            pipe.segment.manning_n = n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_path_stable() {
        let a = file_path_for_drawing("Drawing1.dwg");
        let b = file_path_for_drawing("drawing1.dwg");
        assert_eq!(a, b);
    }
}