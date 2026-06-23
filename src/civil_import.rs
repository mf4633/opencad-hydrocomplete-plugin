//! `HC_CIVIL_IMPORT` — bridge Civil 3D sewer plan geometry (structure blocks +
//! network lines) into HydroComplete XDATA structures and pipes.

use std::collections::{HashSet, VecDeque};

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
const STRUCT_LABEL_MATCH_FT: f64 = 75.0;
const PIPE_LABEL_MATCH_FT: f64 = 35.0;

pub fn usage() -> &'static str {
    "HC_CIVIL_IMPORT [layer] [force] [d15] [n13] [area <ac>] [c <rv>] [tc <min>]  — Civil bridge; optional catchment on headwater inlet"
}

#[derive(Clone, Debug)]
pub struct CivilImportConfig {
    pub layer: String,
    pub force: bool,
    pub diameter_ft: f64,
    pub n: f64,
    pub match_tolerance_ft: f64,
    pub catchment_area: Option<f64>,
    pub catchment_c: Option<f64>,
    pub catchment_tc: Option<f64>,
}

impl Default for CivilImportConfig {
    fn default() -> Self {
        Self {
            layer: DEFAULT_LAYER.to_string(),
            force: false,
            diameter_ft: DEFAULT_DIAMETER_FT,
            n: DEFAULT_N,
            match_tolerance_ft: DEFAULT_MATCH_FT,
            catchment_area: None,
            catchment_c: None,
            catchment_tc: None,
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
    invert_from_label: bool,
    rim_from_label: bool,
}

#[derive(Clone, Debug, Default)]
struct PipeLabel {
    length_ft: Option<f64>,
    diameter_in: Option<u32>,
    slope_pct: Option<f64>,
}

impl PipeLabel {
    fn merge(&mut self, other: &PipeLabel) {
        if let Some(d) = other.diameter_in {
            self.diameter_in = Some(self.diameter_in.map(|x| x.max(d)).unwrap_or(d));
        }
        if self.slope_pct.is_none() {
            self.slope_pct = other.slope_pct;
        }
        if self.length_ft.is_none() {
            self.length_ft = other.length_ft;
        }
    }
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
    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        let tl = token.to_ascii_lowercase();
        match tl.as_str() {
            "force" => {
                cfg.force = true;
                i += 1;
            }
            "area" => {
                if let Some(v) = tokens.get(i + 1).and_then(|s| parse_num(s)) {
                    cfg.catchment_area = Some(v);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "c" => {
                if let Some(v) = tokens.get(i + 1).and_then(|s| parse_num(s)) {
                    cfg.catchment_c = Some(v);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "tc" => {
                if let Some(v) = tokens.get(i + 1).and_then(|s| parse_num(s)) {
                    cfg.catchment_tc = Some(v);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                if let Some(rest) = tl.strip_prefix('d') {
                    if let Ok(inches) = rest.parse::<u32>() {
                        cfg.diameter_ft = inches as f64 / 12.0;
                        i += 1;
                        continue;
                    }
                }
                if let Some(rest) = tl.strip_prefix('n') {
                    if let Ok(milli) = rest.parse::<u32>() {
                        cfg.n = milli as f64 / 1000.0;
                        i += 1;
                        continue;
                    }
                }
                if !token.contains('\\') && !token.contains('/') && token.contains('-') {
                    cfg.layer = token.to_string();
                }
                i += 1;
            }
        }
    }
    cfg
}

fn pipe_graph_has_cycle(n: usize, pipe_pairs: &[(usize, usize)]) -> bool {
    if n == 0 || pipe_pairs.is_empty() {
        return false;
    }
    let mut in_d = vec![0usize; n];
    let mut adj = vec![Vec::new(); n];
    for &(from, to) in pipe_pairs {
        if from < n && to < n && from != to {
            adj[from].push(to);
            in_d[to] += 1;
        }
    }
    let mut q: VecDeque<usize> = (0..n).filter(|&i| in_d[i] == 0).collect();
    let mut visited = 0usize;
    while let Some(u) = q.pop_front() {
        visited += 1;
        for &v in &adj[u] {
            in_d[v] = in_d[v].saturating_sub(1);
            if in_d[v] == 0 {
                q.push_back(v);
            }
        }
    }
    visited < n
}

struct PipeEdge {
    from: usize,
    to: usize,
    length: f64,
    handle: u64,
}

/// Drop shortest pipes until the directed graph is acyclic (storm analysis requires a DAG).
fn acyclic_pipe_edges(mut edges: Vec<PipeEdge>, n: usize) -> (Vec<PipeEdge>, usize) {
    let mut removed = 0usize;
    while pipe_graph_has_cycle(n, &edges.iter().map(|e| (e.from, e.to)).collect::<Vec<_>>()) {
        if edges.is_empty() {
            break;
        }
        let drop_idx = edges
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.length
                    .partial_cmp(&b.length)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(edges.len() - 1);
        edges.remove(drop_idx);
        removed += 1;
    }
    (edges, removed)
}

fn pipe_topology_degrees(n: usize, pipe_pairs: &[(usize, usize)]) -> (Vec<usize>, Vec<usize>) {
    let mut in_d = vec![0usize; n];
    let mut out_d = vec![0usize; n];
    for &(from, to) in pipe_pairs {
        if from < n && to < n && from != to {
            out_d[from] += 1;
            in_d[to] += 1;
        }
    }
    (in_d, out_d)
}

fn downstream_reach(n: usize, start: usize, pipe_pairs: &[(usize, usize)]) -> usize {
    let mut adj = vec![Vec::new(); n];
    for &(f, t) in pipe_pairs {
        if f < n && t < n && f != t {
            adj[f].push(t);
        }
    }
    let mut seen = HashSet::new();
    let mut q = VecDeque::new();
    seen.insert(start);
    q.push_back(start);
    while let Some(u) = q.pop_front() {
        for &v in &adj[u] {
            if seen.insert(v) {
                q.push_back(v);
            }
        }
    }
    seen.len()
}

/// Index of the dendritic headwater structure (no incoming pipes, at least one outgoing).
pub(crate) fn headwater_inlet_index(
    structs: &[CivilStructure],
    pipe_pairs: &[(usize, usize)],
) -> Option<usize> {
    let n = structs.len();
    if n == 0 {
        return None;
    }
    let (in_d, out_d) = pipe_topology_degrees(n, pipe_pairs);
    let mut candidates: Vec<usize> = (0..n)
        .filter(|&i| in_d[i] == 0 && out_d[i] > 0 && structs[i].kind != NodeKind::Outfall)
        .collect();
    if candidates.is_empty() {
        candidates = (0..n)
            .filter(|&i| out_d[i] > 0 && structs[i].kind != NodeKind::Outfall)
            .collect();
    }
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|&a, &b| {
        let rank = |i: usize| {
            let kind_score = match structs[i].kind {
                NodeKind::Inlet => 0,
                NodeKind::Junction => 1,
                NodeKind::Outfall => 2,
            };
            (kind_score, -(downstream_reach(n, i, pipe_pairs) as i32), i)
        };
        rank(a).cmp(&rank(b))
    });
    Some(candidates[0])
}

pub fn headwater_inlet_handle_from_drawn<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
) -> Option<Handle> {
    let drawn = data::drawn_network_from_entities(entities).ok()?;
    let n = drawn.network.nodes.len();
    if n == 0 {
        return None;
    }
    let mut id_to_idx = std::collections::HashMap::new();
    for (i, node) in drawn.network.nodes.iter().enumerate() {
        id_to_idx.insert(node.id.clone(), i);
    }
    let mut in_d = vec![0usize; n];
    let mut out_d = vec![0usize; n];
    let mut pairs = Vec::new();
    for pipe in &drawn.network.pipes {
        let Some(&from) = id_to_idx.get(&pipe.from) else {
            continue;
        };
        let Some(&to) = id_to_idx.get(&pipe.to) else {
            continue;
        };
        if from != to {
            pairs.push((from, to));
            out_d[from] += 1;
            in_d[to] += 1;
        }
    }
    let structs: Vec<CivilStructure> = drawn
        .network
        .nodes
        .iter()
        .map(|node| CivilStructure {
            block_name: String::new(),
            x: node.x,
            y: node.y,
            kind: node.kind,
            invert: node.invert,
            rim: node.rim,
            invert_from_label: false,
            rim_from_label: false,
        })
        .collect();
    let idx = headwater_inlet_index(&structs, &pairs)?;
    drawn.node_handles.get(idx).copied()
}

/// Classify structure kind from a Civil plan label (e.g. `4 CB-3`, `2 UG DET OUT`).
pub fn kind_from_label_name(name: &str) -> NodeKind {
    let u = name.to_ascii_uppercase();
    if u.contains("OUTFALL")
        || u.contains("OUTLET")
        || u.contains(" DET OUT")
        || u.contains("EX MH")
        || u.contains("EX MSD")
        || u.contains("MSD")
        || u.contains("TAIL")
        || u.ends_with(" OUT")
    {
        return NodeKind::Outfall;
    }
    if u.contains("CB")
        || u.contains("HW")
        || u.contains("INLET")
        || u.contains("CURB")
        || u.contains("GRATE")
        || u.contains("CATCH")
    {
        return NodeKind::Inlet;
    }
    NodeKind::Junction
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

fn parse_inv_value(line: &str) -> Option<f64> {
    let after = line.split('=').nth(1)?;
    let num: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    parse_num(&num)
}

/// Parse a structure MText label (`SAN MH 1`, `RIM=`, `INV.IN=`, `INV.OUT=`).
pub fn parse_structure_label_text(text: &str) -> Option<(String, Option<f64>, Option<f64>)> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let name = lines[0].to_string();
    let mut rim = None;
    let mut inv_out = None;
    let mut inv_in = None;
    for line in &lines[1..] {
        let upper = line.to_ascii_uppercase();
        if upper.contains("RIM=") || upper.starts_with("RIM") && upper.contains('=') {
            rim = parse_inv_value(line).or(rim);
        } else if upper.contains("INV.OUT") {
            if let Some(v) = parse_inv_value(line) {
                inv_out = Some(inv_out.map(|o: f64| o.min(v)).unwrap_or(v));
            }
        } else if upper.contains("INV.IN") || upper.contains("INV.IN=") {
            if let Some(v) = parse_inv_value(line) {
                inv_in = Some(inv_in.map(|o: f64| o.min(v)).unwrap_or(v));
            }
        }
    }
    let invert = inv_out.or(inv_in);
    Some((name, rim, invert))
}

/// Parse a pipe Text label (`112' ~8" 0.50%`, `22' ~15"`, `1.00%`).
pub fn parse_pipe_label_text(text: &str) -> PipeLabel {
    let mut label = PipeLabel::default();
    let t = text.trim();
    if let Some(idx) = t.find('~') {
        let rest = &t[idx + 1..];
        let inches: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(n) = inches.parse::<u32>() {
            label.diameter_in = Some(n);
        }
    }
    if let Some(pos) = t.find('\'') {
        let before = &t[..pos];
        let ft: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(n) = ft.parse::<f64>() {
            label.length_ft = Some(n);
        }
    }
    for part in t.split_whitespace() {
        if let Some(num) = part.strip_suffix('%') {
            if let Ok(v) = num.parse::<f64>() {
                label.slope_pct = Some(v);
            }
        }
    }
    label
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

fn entity_xy(e: &EntityType) -> Option<(f64, f64)> {
    match e {
        EntityType::Text(t) => Some((t.insertion_point.x, t.insertion_point.y)),
        EntityType::MText(mt) => Some((mt.insertion_point.x, mt.insertion_point.y)),
        _ => None,
    }
}

fn collect_layer_annotations<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    layer: &str,
) -> (
    Vec<(f64, f64, String, Option<f64>, Option<f64>)>,
    Vec<(f64, f64, PipeLabel)>,
) {
    let mut structs = Vec::new();
    let mut pipes = Vec::new();
    for e in entities {
        if e.common().layer != layer {
            continue;
        }
        let Some((x, y)) = entity_xy(e) else {
            continue;
        };
        match e {
            EntityType::MText(mt) => {
                if let Some((name, rim, inv)) = parse_structure_label_text(&mt.value) {
                    structs.push((x, y, name, rim, inv));
                }
            }
            EntityType::Text(t) => {
                let label = parse_pipe_label_text(&t.value);
                if label.diameter_in.is_some() || label.slope_pct.is_some() || label.length_ft.is_some()
                {
                    pipes.push((x, y, label));
                }
            }
            _ => {}
        }
    }
    (structs, pipes)
}

fn nearest_structure_label<'a>(
    x: f64,
    y: f64,
    labels: &'a [(f64, f64, String, Option<f64>, Option<f64>)],
    tol_ft: f64,
) -> Option<&'a (f64, f64, String, Option<f64>, Option<f64>)> {
    let tol2 = tol_ft * tol_ft;
    labels
        .iter()
        .filter_map(|l| {
            let d2 = dist2(x, y, l.0, l.1);
            if d2 <= tol2 {
                Some((l, d2))
            } else {
                None
            }
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(l, _)| l)
}

fn apply_structure_labels(
    structs: &mut [CivilStructure],
    labels: &[(f64, f64, String, Option<f64>, Option<f64>)],
    tol_ft: f64,
) -> usize {
    let mut matched = 0usize;
    for s in structs.iter_mut() {
        let Some((_, _, name, rim, inv)) = nearest_structure_label(s.x, s.y, labels, tol_ft) else {
            continue;
        };
        matched += 1;
        s.block_name = name.clone();
        s.kind = kind_from_label_name(name);
        if let Some(r) = *rim {
            s.rim = r.max(s.invert + 0.5);
            s.rim_from_label = true;
        }
        if let Some(i) = *inv {
            s.invert = i;
            s.invert_from_label = true;
            if !s.rim_from_label {
                s.rim = s.rim.max(i + 0.5);
            }
        }
    }
    matched
}

fn pipe_label_near_line(
    pipe: &CivilPipeLine,
    labels: &[(f64, f64, PipeLabel)],
    tol_ft: f64,
) -> PipeLabel {
    let mx = (pipe.x0 + pipe.x1) * 0.5;
    let my = (pipe.y0 + pipe.y1) * 0.5;
    let mut merged = PipeLabel::default();
    let mut found = false;
    for (tx, ty, label) in labels {
        let d_line = dist_point_to_segment(*tx, *ty, pipe.x0, pipe.y0, pipe.x1, pipe.y1);
        let d_mid = dist2(*tx, *ty, mx, my).sqrt();
        if d_line <= tol_ft || d_mid <= tol_ft {
            merged.merge(label);
            found = true;
        }
    }
    if found {
        merged
    } else {
        PipeLabel::default()
    }
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
                    invert_from_label: inv_attr.is_some(),
                    rim_from_label: rim_attr.is_some(),
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

fn apply_downstream_inverts(
    handles: &[Handle],
    pipe_pairs: &[(usize, usize)],
    structs: &[CivilStructure],
    host: &mut dyn HostApi,
) {
    let n = handles.len();
    for &(from, to) in pipe_pairs {
        if from >= n || to >= n || from == to {
            continue;
        }
        if from < structs.len()
            && to < structs.len()
            && structs[to].invert_from_label
        {
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

    let entities: Vec<_> = host.document().entities().collect();
    let (civil_structs, civil_pipes) =
        collect_civil_geometry(entities.iter().copied(), &cfg.layer);
    let (struct_labels, pipe_labels) =
        collect_layer_annotations(entities.iter().copied(), &cfg.layer);

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

    let mut pipe_edges: Vec<PipeEdge> = Vec::new();
    let mut skipped_pipes = 0usize;

    for pipe in &civil_pipes {
        match match_pipe_endpoints(&civil_structs, pipe, cfg.match_tolerance_ft) {
            Some((f, t)) => {
                let len = ((pipe.x1 - pipe.x0).powi(2) + (pipe.y1 - pipe.y0).powi(2)).sqrt();
                pipe_edges.push(PipeEdge {
                    from: f,
                    to: t,
                    length: len,
                    handle: pipe.handle.value(),
                });
            }
            None => skipped_pipes += 1,
        }
    }

    if pipe_edges.is_empty() {
        return Err(format!(
            "Found {} structure(s) and {} line(s) but could not match pipe endpoints within {:.0} ft.",
            civil_structs.len(),
            civil_pipes.len(),
            cfg.match_tolerance_ft
        ));
    }

    let mut structs = civil_structs;
    let (pipe_edges, cycle_drops) = acyclic_pipe_edges(pipe_edges, structs.len());
    let pipe_pairs: Vec<(usize, usize)> = pipe_edges.iter().map(|e| (e.from, e.to)).collect();
    let active_handles: HashSet<u64> = pipe_edges.iter().map(|e| e.handle).collect();
    let struct_labels_matched =
        apply_structure_labels(&mut structs, &struct_labels, STRUCT_LABEL_MATCH_FT);
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
        if !active_handles.contains(&pipe.handle.value()) {
            continue;
        }
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
        let spec = pipe_label_near_line(pipe, &pipe_labels, PIPE_LABEL_MATCH_FT);
        let dia_ft = spec
            .diameter_in
            .map(|inches| inches as f64 / 12.0)
            .unwrap_or(cfg.diameter_ft);
        ent.common_mut()
            .extended_data
            .add_record(pipe_xdata(dia_ft, cfg.n, from_h, to_h));
        tagged += 1;
    }

    apply_downstream_inverts(&handles, &pipe_pairs, &structs, host);

    let headwater_idx = headwater_inlet_index(&structs, &pipe_pairs);
    let mut catchment_note = String::new();
    if let (Some(idx), Some(area)) = (headwater_idx, cfg.catchment_area) {
        let h = handles[idx];
        let c_val = cfg.catchment_c.unwrap_or(DEFAULT_C);
        let tc_val = cfg.catchment_tc.unwrap_or(10.0);
        if let Some(ent) = find_structure_mut(host, h) {
            if let Some(mut info) = data::read_structure_info(ent) {
                if info.kind != NodeKind::Outfall {
                    info.area = area;
                    info.c = c_val;
                    info.tc_inlet = tc_val;
                    data::write_structure_info(ent, &info);
                    catchment_note = format!(
                        " Headwater inlet {:X}: area={area:.2} ac C={c_val:.2} Tc={tc_val:.0} min.",
                        h.value()
                    );
                }
            }
        }
    } else if let Some(idx) = headwater_idx {
        catchment_note = format!(
            " Headwater inlet {:X} (use HC_EDIT or area/c/tc args to set catchment).",
            handles[idx].value()
        );
    }

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

    let labeled_pipes = civil_pipes
        .iter()
        .filter(|p| {
            pipe_label_near_line(p, &pipe_labels, PIPE_LABEL_MATCH_FT)
                .diameter_in
                .is_some()
        })
        .count();

    let mut msg = format!(
        "Civil import from \"{}\": {} structure(s) ({} inlet, {} outfall), {} MText + {} pipe Text label(s) ({} structures matched), {} line(s) XDATA-tagged ({} w/ dia label, {} matched, {} skipped), {} pipe(s) in network, default dia={:.2} ft n={:.3}.{catchment_note}",
        cfg.layer,
        structs.len(),
        inlets,
        outfalls,
        struct_labels.len(),
        pipe_labels.len(),
        struct_labels_matched,
        tagged_verify,
        labeled_pipes,
        pipe_pairs.len(),
        skipped_pipes,
        net_pipes,
        cfg.diameter_ft,
        cfg.n
    );
    if cycle_drops > 0 {
        msg.push_str(&format!(" Dropped {cycle_drops} cyclic pipe(s) for analysis."));
    }
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
    fn acyclic_pipe_edges_breaks_simple_cycle() {
        let edges = vec![
            PipeEdge { from: 0, to: 1, length: 100.0, handle: 1 },
            PipeEdge { from: 1, to: 2, length: 100.0, handle: 2 },
            PipeEdge { from: 2, to: 0, length: 10.0, handle: 3 },
        ];
        let (kept, removed) = acyclic_pipe_edges(edges, 3);
        assert_eq!(removed, 1);
        assert_eq!(kept.len(), 2);
        assert!(!pipe_graph_has_cycle(3, &kept.iter().map(|e| (e.from, e.to)).collect::<Vec<_>>()));
    }

    #[test]
    fn kind_from_block_classifies_outfall_and_inlet() {
        assert_eq!(kind_from_block_name("SPT65"), NodeKind::Junction);
        assert_eq!(kind_from_block_name("OUTFALL-1"), NodeKind::Outfall);
        assert_eq!(kind_from_block_name("CB-12"), NodeKind::Inlet);
    }

    #[test]
    fn kind_from_label_classifies_cb_and_outfall() {
        assert_eq!(kind_from_label_name("4 CB-3"), NodeKind::Inlet);
        assert_eq!(kind_from_label_name("2 UG DET OUT"), NodeKind::Outfall);
        assert_eq!(kind_from_label_name("SAN MH 1"), NodeKind::Junction);
    }

    #[test]
    fn parse_structure_mtext_24145_sample() {
        let text = "SAN MH 1\nRIM=2217.83\nINV.IN=2213.10(SAN MH 2)\nINV.OUT=2212.90";
        let (name, rim, inv) = parse_structure_label_text(text).unwrap();
        assert_eq!(name, "SAN MH 1");
        assert!((rim.unwrap() - 2217.83).abs() < 1e-6);
        assert!((inv.unwrap() - 2212.90).abs() < 1e-6);
    }

    #[test]
    fn parse_pipe_text_24145_diameter_and_slope() {
        let eight = parse_pipe_label_text("112' ~8\" 0.50%");
        assert_eq!(eight.diameter_in, Some(8));
        assert!((eight.slope_pct.unwrap() - 0.50).abs() < 1e-6);
        assert!((eight.length_ft.unwrap() - 112.0).abs() < 1e-6);
        let fifteen = parse_pipe_label_text("43' ~15\" 5.00%");
        assert_eq!(fifteen.diameter_in, Some(15));
        assert!((fifteen.slope_pct.unwrap() - 5.0).abs() < 1e-6);
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
    fn parse_config_reads_catchment_tokens() {
        let cfg = parse_config("I-SEWER-NETWORK d15 n13 area 1.5 c 0.78 tc 15");
        assert_eq!(cfg.layer, "I-SEWER-NETWORK");
        assert!((cfg.catchment_area.unwrap() - 1.5).abs() < 1e-9);
        assert!((cfg.catchment_c.unwrap() - 0.78).abs() < 1e-9);
        assert!((cfg.catchment_tc.unwrap() - 15.0).abs() < 1e-9);
    }

    #[test]
    fn headwater_picks_source_without_incoming_pipes() {
        let structs = vec![
            CivilStructure {
                block_name: "CB".into(),
                x: 0.0,
                y: 0.0,
                kind: NodeKind::Inlet,
                invert: 100.0,
                rim: 106.0,
                invert_from_label: false,
                rim_from_label: false,
            },
            CivilStructure {
                block_name: "SPT65".into(),
                x: 100.0,
                y: 0.0,
                kind: NodeKind::Junction,
                invert: 99.0,
                rim: 105.0,
                invert_from_label: false,
                rim_from_label: false,
            },
            CivilStructure {
                block_name: "OUT".into(),
                x: 200.0,
                y: 0.0,
                kind: NodeKind::Outfall,
                invert: 98.0,
                rim: 104.0,
                invert_from_label: false,
                rim_from_label: false,
            },
        ];
        let pairs = vec![(0, 1), (1, 2)];
        assert_eq!(headwater_inlet_index(&structs, &pairs), Some(0));
    }

    #[test]
    fn headwater_promotes_upstream_junction_when_no_explicit_inlet() {
        let structs = vec![
            CivilStructure {
                block_name: "SPT65".into(),
                x: 0.0,
                y: 0.0,
                kind: NodeKind::Junction,
                invert: 100.0,
                rim: 106.0,
                invert_from_label: false,
                rim_from_label: false,
            },
            CivilStructure {
                block_name: "SPT65".into(),
                x: 100.0,
                y: 0.0,
                kind: NodeKind::Junction,
                invert: 99.0,
                rim: 105.0,
                invert_from_label: false,
                rim_from_label: false,
            },
        ];
        let pairs = vec![(0, 1)];
        assert_eq!(headwater_inlet_index(&structs, &pairs), Some(0));
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
            invert_from_label: false,
            rim_from_label: false,
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
                invert_from_label: false,
                rim_from_label: false,
            },
            CivilStructure {
                block_name: "SPT65".into(),
                x: 100.0,
                y: 50.0,
                kind: NodeKind::Junction,
                invert: 99.0,
                rim: 105.0,
                invert_from_label: false,
                rim_from_label: false,
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