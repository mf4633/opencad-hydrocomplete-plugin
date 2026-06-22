//! `HC_NETWORK_EDIT` — text-based pipe override editor (design Q, Manning n).

use acadrust::EntityType;

use crate::data;
use crate::network_override::{self, PipeOverride};

pub fn usage() -> &'static str {
    "HC_NETWORK_EDIT [LIST | SET <handle> Q <cfs> [N <n>] [NOTES <text>] | CLEAR [handle]]"
}

fn pipe_key_from_handle(handle: acadrust::Handle) -> String {
    format!("H{}", handle.value())
}

pub fn run<'a>(
    drawing: &str,
    entities: impl Iterator<Item = &'a EntityType>,
    args: &str,
) -> Result<Vec<String>, String> {
    let drawn = data::drawn_network_from_entities(entities).map_err(|e| e.to_string())?;
    if drawn.network.pipes.is_empty() {
        return Err("No pipe networks found in this drawing.".into());
    }

    let tokens: Vec<&str> = args.split_whitespace().collect();
    if tokens.is_empty() || tokens[0].eq_ignore_ascii_case("LIST") {
        return list_pipes(&drawn, &drawing);
    }

    if tokens[0].eq_ignore_ascii_case("CLEAR") {
        let mut overrides = network_override::load(&drawing);
        if tokens.len() == 1 {
            overrides.clear();
        } else {
            let key = resolve_pipe_key(&drawn, tokens[1])?;
            overrides.retain(|o| !o.pipe_key.eq_ignore_ascii_case(&key));
        }
        let path = network_override::save(&drawing, &overrides)?;
        return Ok(vec![
            "--- HydroComplete: network overrides cleared ---".into(),
            format!("  Pipes: {}", overrides.len()),
            format!("  File: {}", path.display()),
        ]);
    }

    if tokens[0].eq_ignore_ascii_case("SET") {
        if tokens.len() < 4 {
            return Err(usage().into());
        }
        let key = resolve_pipe_key(&drawn, tokens[1])?;
        let (pipe_name, network_name) = pipe_meta(&drawn, &key);
        let mut q: Option<f64> = None;
        let mut n: Option<f64> = None;
        let mut notes = String::new();
        let mut i = 2;
        while i < tokens.len() {
            match tokens[i].to_ascii_uppercase().as_str() {
                "Q" => {
                    i += 1;
                    q = Some(parse_num(tokens.get(i).copied().ok_or("missing Q value")?)?);
                }
                "N" => {
                    i += 1;
                    n = Some(parse_num(tokens.get(i).copied().ok_or("missing N value")?)?);
                }
                "NOTES" => {
                    i += 1;
                    notes = tokens[i..].join(" ");
                    break;
                }
                other => return Err(format!("Unknown field: {other}")),
            }
            i += 1;
        }
        let mut overrides = network_override::load(&drawing);
        if let Some(slot) = overrides.iter_mut().find(|o| o.pipe_key.eq_ignore_ascii_case(&key)) {
            if q.is_some() {
                slot.design_flow_cfs = q;
            }
            if n.is_some() {
                slot.manning_n = n;
            }
            if !notes.is_empty() {
                slot.notes = notes;
            }
        } else {
            overrides.push(PipeOverride {
                pipe_key: key.clone(),
                pipe_name,
                network_name,
                design_flow_cfs: q,
                manning_n: n,
                notes,
            });
        }
        let path = network_override::save(&drawing, &overrides)?;
        return Ok(vec![
            "--- HydroComplete: network overrides saved ---".into(),
            format!("  Pipe: {key}"),
            format!(
                "  Q = {} cfs   n = {}",
                q.map(|v| format!("{v:.3}")).unwrap_or_else(|| "(unchanged)".into()),
                n.map(|v| format!("{v:.4}")).unwrap_or_else(|| "(unchanged)".into())
            ),
            format!("  File: {}", path.display()),
            "  Overrides apply to HC_CAPACITY / HC_HGL / HC_ANALYZE on next run.".into(),
        ]);
    }

    Err(usage().into())
}

fn list_pipes(drawn: &data::DrawnNetwork, drawing: &str) -> Result<Vec<String>, String> {
    let overrides = network_override::load(drawing);
    let mut by_key: std::collections::HashMap<String, &PipeOverride> = std::collections::HashMap::new();
    for o in &overrides {
        by_key.insert(o.pipe_key.to_lowercase(), o);
    }
    let mut lines = vec![
        "--- HydroComplete: network editor (pipe overrides) ---".into(),
        format!("  Drawing: {drawing}"),
        format!("  Store: {}", network_override::file_path_for_drawing(drawing).display()),
        "  Pipe list (use HC_NETWORK_EDIT SET <handle> Q <cfs> N <n>):".into(),
    ];
    for (idx, handle) in drawn.pipe_handles.iter().enumerate() {
        let key = pipe_key_from_handle(*handle);
        let pipe = &drawn.network.pipes[idx];
        let o = by_key.get(&key.to_lowercase());
        let q = o.and_then(|o| o.design_flow_cfs)
            .map(|v| format!("{v:.3}"))
            .unwrap_or_else(|| "-".into());
        let n = o.and_then(|o| o.manning_n)
            .map(|v| format!("{v:.4}"))
            .unwrap_or_else(|| format!("{:.4}", pipe.n));
        lines.push(format!(
            "    H{}  P{}  {} -> {}  dia={:.2}  Q={q}  n={n}",
            handle.value(),
            idx + 1,
            pipe.from,
            pipe.to,
            pipe.diameter
        ));
    }
    Ok(lines)
}

fn resolve_pipe_key(drawn: &data::DrawnNetwork, token: &str) -> Result<String, String> {
    if let Ok(h) = token.trim().parse::<u64>() {
        let handle = acadrust::Handle::new(h);
        if drawn.pipe_handles.iter().any(|ph| *ph == handle) {
            return Ok(pipe_key_from_handle(handle));
        }
    }
    if token.starts_with('H') || token.starts_with('h') {
        if let Ok(h) = token[1..].parse::<u64>() {
            let handle = acadrust::Handle::new(h);
            if drawn.pipe_handles.iter().any(|ph| *ph == handle) {
                return Ok(pipe_key_from_handle(handle));
            }
        }
    }
    Err(format!("Pipe handle not found: {token}"))
}

fn pipe_meta(drawn: &data::DrawnNetwork, key: &str) -> (String, String) {
    let handle_val = key
        .trim_start_matches('H')
        .trim_start_matches('h')
        .parse::<u64>()
        .ok();
    if let Some(hv) = handle_val {
        if let Some((idx, _)) = drawn
            .pipe_handles
            .iter()
            .enumerate()
            .find(|(_, h)| h.value() == hv)
        {
            return (format!("P{}", idx + 1), "default".into());
        }
    }
    (key.into(), "default".into())
}

fn parse_num(s: &str) -> Result<f64, String> {
    s.trim()
        .replace(',', ".")
        .parse::<f64>()
        .map_err(|_| format!("Invalid number: {s}"))
}

