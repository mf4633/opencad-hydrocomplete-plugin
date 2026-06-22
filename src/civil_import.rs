//! `HC_CIVIL_IMPORT` — bridge Civil 3D sewer plan geometry (structure blocks +
//! network lines) into HydroComplete XDATA structures and pipes.

use std::collections::HashSet;

use acadrust::entities::Insert;
use acadrust::types::Vector3;
use acadrust::{Circle, EntityType, Handle, Line};
use ocs_plugin_api::host::HostApi;

use stormsewer::network::NodeKind;

use super::data::{self, pipe_xdata, structure_xdata};

const DEFAULT_LAYER: &str = "I-SEWER-NETWORK";
const HC_STRUCT_LAYER: &str = "HC-STRUCT";
const DEFAULT_INVERT: f64 = 100.0;
const DEFAULT_RIM: f64 = 106.0;
const DEFAULT_AREA: f64 = 1.0;
const DEFAULT_C: f64 = 0.78;
const DEFAULT_DIAMETER_FT: f64 = 1.25; // 15 in
const DEFAULT_N: f64 = 0.013;
const DEFAULT_MATCH_FT: f64 = 120.0;
const DEFAULT_PIPE_SLOPE: f64 = 0.01;

pub fn usage() -> &'static str {
    "HC_CIVIL_IMPORT [layer] [force] [d15] [n13]  — Civil 3D I-SEWER-NETWORK blocks+lines → HC network"
}

#[derive(Clone, Debug)]
pub struct CivilImportConfig {
    pub layer: String,
    pub force: bool,
    pub diameter_ft: f64,
    pub n: f64,
    pub match_tolerance_ft: f64,
}

impl Default for CivilImportConfig {
    fn default() -> Self {
        Self {
            layer: DEFAULT_LAYER.to_string(),
            force: false,
            diameter_ft: DEFAULT_DIAMETER_FT,
            n: DEFAULT_N,
            match_tolerance_ft: DEFAULT_MATCH_FT,
        }
    }
}

#[derive(Clone, Debug)]
struct CivilStructure {
    block_name: String,
    x: f64,
    y: f64,
    kind: NodeKind,
    invert: f64,
    rim: f64,
}

#[derive(Clone, Debug)]
struct CivilPipeLine {
    handle: Handle,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    length: f64,
}

fn parse_num(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok()
}

pub fn parse_config(args: &str) -> CivilImportConfig {
    let mut cfg = CivilImportConfig::default();
    for token in args.split_whitespace() {
        let tl = token.to_ascii_lowercase();
        if tl == "force" {
            cfg.force = true;
            continue;
        }
        if let Some(rest) = tl.strip_prefix('d') {
            if let Ok(inches) = rest.parse::<u32>() {
                cfg.diameter_ft = inches as f64 / 12.0;
                continue;
            }
        }
        if let Some(rest) = tl.strip_prefix('n') {
            if let Ok(milli) = rest.parse::<u32>() {
                cfg.n = milli as f64 / 1000.0;
                continue;
            }
        }
        if !token.contains('\\') && !token.contains('/') && token.contains('-') {
            cfg.layer = token.to_string();
        }
    }
    cfg
}

/// Classify structure kind from a Civil 3D block name (e.g. SPT65, CB-12).
pub fn kind_from_block_name(name: &str) -> NodeKind {
    let u = name.to_ascii_uppercase();
    if u.contains("OUTFALL")
        || u.contains("OUTLET")
        || u.starts_with("OF")
        || u.contains("TAIL")
        || u.ends_with("FO")
    {
        return NodeKind::Outfall;
    }
    if u.contains("INLET")
        || u.starts_with("IN")
        || u.contains("CURB")
        || u.contains("GRATE")
        || u.starts_with("CB")
        || u.contains("CATCH")
        || u.starts_with("DI")
    {
        return NodeKind::Inlet;
    }
    NodeKind::Junction
}

fn parse_elevation_token(tag: &str, value: &str) -> Option<(bool, f64)> {
    let tag_u = tag.to_ascii_uppercase();
    let is_rim = tag_u.contains("RIM");
    let is_inv = tag_u.contains("INV") || tag_u.contains("INVERT") || tag_u == "IE";
    if !is_rim && !is_inv {
        return None;
    }
    let cleaned: String = value
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let v = parse_num(&cleaned)?;
    Some((is_rim, v))
}

fn elevations_from_insert(ins: &Insert) -> (Option<f64>, Option<f64>) {
    let mut rim = None;
    let mut invert = None;
    for attr in &ins.attributes {
        let Some((is_rim, v)) = parse_elevation_token(&attr.tag, &attr.value) else {
            continue;
        };
        if is_rim {
            rim = Some(v);
        } else {
            invert = Some(v);
        }
    }
    (rim, invert)
}

fn default_radius(kind: NodeKind) -> f64 {
    match kind {
        NodeKind::Inlet => 3.0,
        NodeKind::Junction => 4.0,
        NodeKind::Outfall => 6.0,
    }
}

fn structure_circle(kind: NodeKind, x: f64, y: f64, invert: f64, rim: f64) -> EntityType {
    let mut e = EntityType::Circle(Circle {
        center: Vector3::new(x, y, 0.0),
        radius: default_radius(kind),
        ..Default::default()
    });
    let (area, c) = if kind == NodeKind::Outfall {
        (0.0, 0.0)
    } else {
        (DEFAULT_AREA, DEFAULT_C)
    };
    e.common_mut().layer = HC_STRUCT_LAYER.to_string();
    e.common_mut()
        .extended_data
        .add_record(structure_xdata(kind, invert, rim, area, c));
    e
}

fn dist2(x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x0 - x1;
    let dy = y0 - y1;
    dx * dx + dy * dy
}

fn dist_point_to_segment(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return dist2(px, py, x0, y0).sqrt();
    }
    let t = ((px - x0) * dx + (py - y0) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    let cx = x0 + t * dx;
    let cy = y0 + t * dy;
    dist2(px, py, cx, cy).sqrt()
}

fn structure_near_line(s: &CivilStructure, pipe: &CivilPipeLine, tol_ft: f64) -> bool {
    dist2(s.x, s.y, pipe.x0, pipe.y0).sqrt() <= tol_ft
        || dist2(s.x, s.y, pipe.x1, pipe.y1).sqrt() <= tol_ft
        || dist_point_to_segment(s.x, s.y, pipe.x0, pipe.y0, pipe.x1, pipe.y1) <= tol_ft
}

fn nearest_structure(
    structs: &[CivilStructure],
    x: f64,
    y: f64,
    tol_ft: f64,
    exclude: Option<usize>,
) -> Option<usize> {
    let tol2 = tol_ft * tol_ft;
    structs
        .iter()
        .enumerate()
        .filter(|(i, _)| exclude.map(|e| e != *i).unwrap_or(true))
        .filter_map(|(i, s)| {
            let d2 = dist2(s.x, s.y, x, y);
            if d2 <= tol2 {
                Some((i, d2))
            } else {
                None
            }
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
}

fn line_parameter(s: &CivilStructure, pipe: &CivilPipeLine) -> f64 {
    let dx = pipe.x1 - pipe.x0;
    let dy = pipe.y1 - pipe.y0;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return 0.0;
    }
    ((s.x - pipe.x0) * dx + (s.y - pipe.y0) * dy) / len2
}

/// Match a Civil pipe line to two structures that span the segment (Civil plan graphics
/// often stop short of structure centers).
fn match_pipe_endpoints(
    structs: &[CivilStructure],
    pipe: &CivilPipeLine,
    tol_ft: f64,
) -> Option<(usize, usize)> {
    let mut near: Vec<(usize, f64, f64)> = structs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            (
                i,
                dist_point_to_segment(s.x, s.y, pipe.x0, pipe.y0, pipe.x1, pipe.y1),
                line_parameter(s, pipe),
            )
        })
        .filter(|(_, d, _)| *d <= tol_ft)
        .collect();

    if near.len() >= 2 {
        near.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut best: Option<(usize, usize, f64)> = None;
        for x in 0..near.len() {
            for y in (x + 1)..near.len() {
                let (i, _, ti) = near[x];
                let (j, _, tj) = near[y];
                if i == j {
                    continue;
                }
                let span = (ti - tj).abs();
                if span < 0.05 {
                    continue;
                }
                let score = near[x].1 + near[y].1;
                if best.as_ref().map(|(_, _, s)| score < *s).unwrap_or(true) {
                    let (from, to) = if ti <= tj { (i, j) } else { (j, i) };
                    best = Some((from, to, score));
                }
            }
        }
        if let Some((from, to, _)) = best {
            return Some((from, to));
        }
    }

    let near_endpoint: Vec<usize> = structs
        .iter()
        .enumerate()
        .filter(|(_, s)| structure_near_line(s, pipe, tol_ft))
        .map(|(i, _)| i)
        .collect();
    if near_endpoint.len() >= 2 {
        let mut best: Option<(usize, usize, f64)> = None;
        for &i in &near_endpoint {
            for &j in &near_endpoint {
                if i == j {
                    continue;
                }
                for (from, to, score) in [
                    (
                        i,
                        j,
                        dist2(structs[i].x, structs[i].y, pipe.x0, pipe.y0).sqrt()
                            + dist2(structs[j].x, structs[j].y, pipe.x1, pipe.y1).sqrt(),
                    ),
                    (
                        j,
                        i,
                        dist2(structs[j].x, structs[j].y, pipe.x0, pipe.y0).sqrt()
                            + dist2(structs[i].x, structs[i].y, pipe.x1, pipe.y1).sqrt(),
                    ),
                ] {
                    if best.as_ref().map(|(_, _, s)| score < *s).unwrap_or(true) {
                        best = Some((from, to, score));
                    }
                }
            }
        }
        if let Some((from, to, _)) = best {
            return Some((from, to));
        }
    }

    let from_i = nearest_structure(structs, pipe.x0, pipe.y0, tol_ft, None)?;
    let to_i = nearest_structure(structs, pipe.x1, pipe.y1, tol_ft, Some(from_i))?;
    if from_i != to_i {
        Some((from_i, to_i))
    } else {
        None
    }
}

fn existing_hc_network<'a>(entities: impl IntoIterator<Item = &'a EntityType>) -> bool {
    entities
        .into_iter()
        .any(|e| data::is_structure_entity(e))
}

fn collect_civil_geometry<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    layer: &str,
) -> (Vec<CivilStructure>, Vec<CivilPipeLine>) {
    let mut structs = Vec::new();
    let mut pipes = Vec::new();

    for e in entities {
        let ent_layer = &e.common().layer;
        if ent_layer != layer {
            continue;
        }
        match e {
            EntityType::Insert(ins) => {
                let kind = kind_from_block_name(&ins.block_name);
                let (rim_attr, inv_attr) = elevations_from_insert(ins);
                let invert = inv_attr.unwrap_or(DEFAULT_INVERT);
                let rim = rim_attr.unwrap_or(DEFAULT_RIM).max(invert + 0.5);
                structs.push(CivilStructure {
                    block_name: ins.block_name.clone(),
                    x: ins.insert_point.x,
                    y: ins.insert_point.y,
                    kind,
                    invert,
                    rim,
                });
            }
            EntityType::Line(line) => {
                if e.common().extended_data.get_record(data::APP_PIPE).is_some() {
                    continue;
                }
                let dx = line.end.x - line.start.x;
                let dy = line.end.y - line.start.y;
                let length = (dx * dx + dy * dy).sqrt();
                if length < 0.5 {
                    continue;
                }
                pipes.push(CivilPipeLine {
                    handle: e.common().handle,
                    x0: line.start.x,
                    y0: line.start.y,
                    x1: line.end.x,
                    y1: line.end.y,
                    length,
                });
            }
            _ => {}
        }
    }
    (structs, pipes)
}

fn refine_outfall_kind(structs: &mut [CivilStructure], pipe_pairs: &[(usize, usize)]) {
    if structs.is_empty() {
        return;
    }
    let n = structs.len();
    let mut out_degree = vec![0usize; n];
    let mut in_degree = vec![0usize; n];
    for &(from, to) in pipe_pairs {
        if from < n && to < n && from != to {
            out_degree[from] += 1;
            in_degree[to] += 1;
        }
    }
    if let Some(idx) = structs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind != NodeKind::Outfall)
        .filter(|(i, _)| out_degree[*i] == 0 && in_degree[*i] > 0)
        .min_by_key(|(i, _)| in_degree[*i])
        .map(|(i, _)| i)
    {
        structs[idx].kind = NodeKind::Outfall;
        structs[idx].invert = structs[idx].invert.min(DEFAULT_INVERT);
    }
    if structs.iter().all(|s| s.kind != NodeKind::Outfall) {
        if let Some(idx) = (0..n).max_by_key(|&i| in_degree[i]) {
            structs[idx].kind = NodeKind::Outfall;
        }
    }
    for (i, s) in structs.iter_mut().enumerate() {
        if s.kind == NodeKind::Junction && out_degree[i] > 0 && in_degree[i] == 0 {
            s.kind = NodeKind::Inlet;
        }
    }
}

fn apply_downstream_inverts(handles: &[Handle], pipe_pairs: &[(usize, usize)], host: &mut dyn HostApi) {
    let n = handles.len();
    for &(from, to) in pipe_pairs {
        if from >= n || to >= n || from == to {
            continue;
        }
        let from_h = handles[from];
        let to_h = handles[to];
        let from_info = match structure_info(host, from_h) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let length = pipe_length_between(host, from_h, to_h).unwrap_or(0.0);
        if length <= 0.0 {
            continue;
        }
        let target = from_info.invert - DEFAULT_PIPE_SLOPE * length;
        if let Ok(mut to_info) = structure_info(host, to_h) {
            if to_info.invert > target + 1e-6 {
                to_info.invert = target;
                if to_info.rim <= to_info.invert {
                    to_info.rim = to_info.invert + (DEFAULT_RIM - DEFAULT_INVERT);
                }
                if let Some(ent) = find_structure_mut(host, to_h) {
                    data::write_structure_info(ent, &to_info);
                }
            }
        }
    }
}

fn structure_info(host: &dyn HostApi, handle: Handle) -> Result<data::StructureInfo, String> {
    for e in host.document().entities() {
        if e.common().handle == handle {
            return data::read_structure_info(e)
                .ok_or_else(|| format!("Handle {} is not a structure", handle.value()));
        }
    }
    Err(format!("Structure handle {} not found", handle.value()))
}

fn find_structure_mut<'a>(
    host: &'a mut dyn HostApi,
    handle: Handle,
) -> Option<&'a mut EntityType> {
    host.document_mut()
        .entities_mut()
        .find(|e| e.common().handle == handle)
}

fn pipe_length_between(host: &dyn HostApi, from: Handle, to: Handle) -> Option<f64> {
    let mut from_xy = None;
    let mut to_xy = None;
    for e in host.document().entities() {
        let h = e.common().handle;
        if let Some(s) = data::read_structure_info(e) {
            if h == from {
                from_xy = Some((s.x, s.y));
            }
            if h == to {
                to_xy = Some((s.x, s.y));
            }
        }
    }
    let (x0, y0) = from_xy?;
    let (x1, y1) = to_xy?;
    Some(((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt())
}

/// Import Civil 3D sewer geometry from the active drawing into HC XDATA.
pub fn import_civil_sewer(host: &mut dyn HostApi, args: &str) -> Result<String, String> {
    let cfg = parse_config(args);
    if !cfg.force && existing_hc_network(host.document().entities()) {
        return Err(
            "Drawing already has HydroComplete structures. Re-run with HC_CIVIL_IMPORT force.".into(),
        );
    }

    let (civil_structs, civil_pipes) =
        collect_civil_geometry(host.document().entities(), &cfg.layer);

    if civil_structs.is_empty() {
        return Err(format!(
            "No structure blocks on layer \"{}\". Expected Civil 3D inserts (e.g. SPT65).",
            cfg.layer
        ));
    }
    if civil_pipes.is_empty() {
        return Err(format!(
            "No pipe lines on layer \"{}\" without HC XDATA.",
            cfg.layer
        ));
    }

    let mut pipe_pairs: Vec<(usize, usize)> = Vec::new();
    let mut skipped_pipes = 0usize;

    for pipe in &civil_pipes {
        match match_pipe_endpoints(&civil_structs, pipe, cfg.match_tolerance_ft) {
            Some((f, t)) => pipe_pairs.push((f, t)),
            None => skipped_pipes += 1,
        }
    }

    if pipe_pairs.is_empty() {
        return Err(format!(
            "Found {} structure(s) and {} line(s) but could not match pipe endpoints within {:.0} ft.",
            civil_structs.len(),
            civil_pipes.len(),
            cfg.match_tolerance_ft
        ));
    }

    let mut structs = civil_structs;
    refine_outfall_kind(&mut structs, &pipe_pairs);

    host.push_undo("HC_CIVIL_IMPORT");

    let mut handles: Vec<Handle> = Vec::with_capacity(structs.len());
    for s in &structs {
        let ent = structure_circle(s.kind, s.x, s.y, s.invert, s.rim);
        handles.push(host.add_entity(ent));
    }

    let mut tagged = 0usize;
    let mut used_lines: HashSet<u64> = HashSet::new();
    for pipe in &civil_pipes {
        let Some((from_i, to_i)) = match_pipe_endpoints(&structs, pipe, cfg.match_tolerance_ft) else {
            continue;
        };
        if !used_lines.insert(pipe.handle.value()) {
            continue;
        }
        let from_h = handles[from_i];
        let to_h = handles[to_i];
        let Some(ent) = host
            .document_mut()
            .entities_mut()
            .find(|e| e.common().handle == pipe.handle)
        else {
            continue;
        };
        let EntityType::Line(_) = ent else {
            continue;
        };
        ent.common_mut()
            .extended_data
            .add_record(pipe_xdata(cfg.diameter_ft, cfg.n, from_h, to_h));
        tagged += 1;
    }

    apply_downstream_inverts(&handles, &pipe_pairs, host);

    host.bump_geometry();
    host.set_dirty();

    let tagged_verify = host
        .document()
        .entities()
        .filter(|e| e.common().extended_data.get_record(data::APP_PIPE).is_some())
        .count();

    let outfalls = structs.iter().filter(|s| s.kind == NodeKind::Outfall).count();
    let inlets = structs.iter().filter(|s| s.kind == NodeKind::Inlet).count();
    let net_pipes = data::network_from_entities(host.document().entities())
        .map(|n| n.pipes.len())
        .unwrap_or(0);

    let msg = format!(
        "Civil import from \"{}\": {} structure(s) ({} inlet, {} outfall), {} line(s) XDATA-tagged ({} matched, {} skipped), {} pipe(s) in network, default dia={:.2} ft n={:.3}.",
        cfg.layer,
        structs.len(),
        inlets,
        outfalls,
        tagged_verify,
        pipe_pairs.len(),
        skipped_pipes,
        net_pipes,
        cfg.diameter_ft,
        cfg.n
    );
    if let Some(dir) = std::env::var_os("APPDATA") {
        let log = std::path::PathBuf::from(dir)
            .join("HydroComplete")
            .join("civil-import-last.txt");
        let _ = std::fs::write(&log, &msg);
    }
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use acadrust::entities::AttributeEntity;

    #[test]
    fn kind_from_block_classifies_outfall_and_inlet() {
        assert_eq!(kind_from_block_name("SPT65"), NodeKind::Junction);
        assert_eq!(kind_from_block_name("OUTFALL-1"), NodeKind::Outfall);
        assert_eq!(kind_from_block_name("CB-12"), NodeKind::Inlet);
    }

    #[test]
    fn parse_config_reads_layer_force_and_hydraulics() {
        let cfg = parse_config("VG-SSWR-PIPE force d18 n15");
        assert_eq!(cfg.layer, "VG-SSWR-PIPE");
        assert!(cfg.force);
        assert!((cfg.diameter_ft - 1.5).abs() < 1e-9);
        assert!((cfg.n - 0.015).abs() < 1e-9);
    }

    #[test]
    fn elevations_from_block_attributes() {
        let mut ins = Insert::new("SPT65", Vector3::new(0.0, 0.0, 0.0));
        ins.attributes.push(AttributeEntity::simple("RIM_ELEV", "2450.5"));
        ins.attributes
            .push(AttributeEntity::simple("INV_IN", "2438.2"));
        let (rim, inv) = elevations_from_insert(&ins);
        assert!((rim.unwrap() - 2450.5).abs() < 1e-6);
        assert!((inv.unwrap() - 2438.2).abs() < 1e-6);
    }

    #[test]
    fn match_24145_sample_network_pairs_most_lines() {
        let structs: Vec<CivilStructure> = [
            (939269.803, 642323.073),
            (939356.790, 642213.763),
            (939470.933, 642283.178),
            (939542.547, 642310.553),
            (939614.700, 642301.396),
            (939616.410, 642189.325),
            (939419.361, 642138.483),
            (939586.383, 642066.241),
            (939614.406, 642117.708),
            (939260.285, 642117.887),
            (939325.338, 642178.555),
            (939347.143, 642178.693),
            (939390.079, 642160.787),
            (939394.080, 642200.142),
            (939341.208, 642220.841),
            (939380.174, 642217.190),
            (939189.064, 642237.488),
            (939238.755, 642353.376),
        ]
        .iter()
        .map(|&(x, y)| CivilStructure {
            block_name: "SPT65".into(),
            x,
            y,
            kind: NodeKind::Junction,
            invert: 100.0,
            rim: 106.0,
        })
        .collect();

        let pipes = [
            (939235.734, 642105.901, 939172.855, 642121.460),
            (939259.033, 642118.712, 939221.234, 642143.605),
            (939323.851, 642178.755, 939291.584, 642183.093),
            (939268.327, 642322.808, 939248.052, 642319.172),
            (939469.781, 642284.138, 939446.503, 642303.536),
            (939541.862, 642311.887, 939530.615, 642333.774),
            (939616.016, 642302.116, 939632.753, 642311.269),
            (939617.901, 642189.493, 939643.612, 642192.387),
            (939615.902, 642117.807, 939638.985, 642119.344),
            (939587.769, 642065.667, 939622.065, 642051.468),
            (939420.674, 642139.209, 939440.913, 642150.396),
            (939395.523, 642200.552, 939434.719, 642211.681),
            (939388.980, 642159.766, 939372.853, 642144.787),
            (939346.222, 642179.876, 939319.920, 642213.639),
            (939339.974, 642221.694, 939306.523, 642244.796),
            (939357.032, 642215.244, 939365.264, 642265.624),
            (939381.542, 642217.805, 939417.693, 642234.054),
            (939187.632, 642237.040, 939145.089, 642223.718),
            (939264.417, 642314.734, 939272.948, 642339.654),
        ];
        let mut matched = 0;
        for (i, &(x0, y0, x1, y1)) in pipes.iter().enumerate() {
            let pipe = CivilPipeLine {
                handle: Handle::new(i as u64 + 1),
                x0,
                y0,
                x1,
                y1,
                length: ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt(),
            };
            if match_pipe_endpoints(&structs, &pipe, DEFAULT_MATCH_FT).is_some() {
                matched += 1;
            }
        }
        assert!(
            matched >= 15,
            "expected most 24-145 sewer lines to match, got {matched}/{}",
            pipes.len()
        );
    }

    #[test]
    fn match_pipe_uses_segment_proximity() {
        let structs = vec![
            CivilStructure {
                block_name: "SPT65".into(),
                x: 0.0,
                y: 0.0,
                kind: NodeKind::Junction,
                invert: 100.0,
                rim: 106.0,
            },
            CivilStructure {
                block_name: "SPT65".into(),
                x: 100.0,
                y: 50.0,
                kind: NodeKind::Junction,
                invert: 99.0,
                rim: 105.0,
            },
        ];
        let pipe = CivilPipeLine {
            handle: Handle::new(99),
            x0: 0.0,
            y0: 0.0,
            x1: 100.0,
            y1: 0.0,
            length: 100.0,
        };
        let pair = match_pipe_endpoints(&structs, &pipe, 60.0);
        assert!(pair.is_some());
        let (a, b) = pair.unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn collect_geometry_finds_layer_entities() {
        let mut ins_ent = EntityType::Insert(Insert::new("SPT65", Vector3::new(10.0, 20.0, 0.0)));
        ins_ent.common_mut().layer = "I-SEWER-NETWORK".to_string();
        let mut line_ent = EntityType::Line(Line::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(50.0, 0.0, 0.0),
        ));
        line_ent.common_mut().layer = "I-SEWER-NETWORK".to_string();
        let ents = vec![ins_ent, line_ent];
        let (structs, pipes) = collect_civil_geometry(ents.iter(), "I-SEWER-NETWORK");
        assert_eq!(structs.len(), 1);
        assert_eq!(pipes.len(), 1);
    }
}