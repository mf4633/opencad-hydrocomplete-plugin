// Storm-sewer data carried on drawing entities via XDATA, and reconstruction
// of a `stormsewer::Network` from the drawn entities.
//
// Structures are circles tagged with the `HYDROCOMPLETE_STRUCT` app record
// [kind, invert, rim, area, C, tc_inlet?]; pipes are lines tagged with
// `HYDROCOMPLETE_PIPE` [diameter, n, from-handle, to-handle]. Catchments are
// closed LwPolylines tagged with `HYDROCOMPLETE_CATCHMENT`
// [C, flow_length_ft, slope, inlet_handle (0 = auto)]. Connectivity is by
// entity handle, so the drawn network round-trips to DWG/DXF and is analyzable
// directly.

use std::collections::HashMap;

use acadrust::entities::LwPolyline;
use acadrust::xdata::{ExtendedDataRecord, XDataValue};
use acadrust::{EntityType, Handle};

use stormsewer::catchment::{catchment_tc_minutes, default_flow_length_ft, polygon_centroid, shoelace_area_sqft, sqft_to_acres};
use stormsewer::network::{Network, Node, NodeKind, Pipe};

/// A storm network reconstructed from drawing entities, with entity handles
/// for round-tripping sizing edits back to the document.
#[derive(Debug)]
pub struct DrawnNetwork {
    pub network: Network,
    /// Structure entity handles, same order as `network.nodes`.
    pub node_handles: Vec<Handle>,
    /// Pipe entity handles, same order as `network.pipes`.
    pub pipe_handles: Vec<Handle>,
}

pub const APP_STRUCT: &str = "HYDROCOMPLETE_STRUCT";
pub const APP_PIPE: &str = "HYDROCOMPLETE_PIPE";
pub const APP_CATCHMENT: &str = "HYDROCOMPLETE_CATCHMENT";

pub fn kind_str(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Inlet => "inlet",
        NodeKind::Junction => "junction",
        NodeKind::Outfall => "outfall",
    }
}

fn parse_kind(s: &str) -> NodeKind {
    match s {
        "outfall" => NodeKind::Outfall,
        "junction" => NodeKind::Junction,
        _ => NodeKind::Inlet,
    }
}

/// XDATA record for a structure marker.
pub fn structure_xdata(kind: NodeKind, invert: f64, rim: f64, area: f64, c: f64) -> ExtendedDataRecord {
    structure_xdata_tc(kind, invert, rim, area, c, 10.0)
}

/// XDATA record for a structure marker including inlet Tc (minutes).
pub fn structure_xdata_tc(
    kind: NodeKind,
    invert: f64,
    rim: f64,
    area: f64,
    c: f64,
    tc_inlet: f64,
) -> ExtendedDataRecord {
    let mut r = ExtendedDataRecord::new(APP_STRUCT);
    r.add_value(XDataValue::String(kind_str(kind).to_string()));
    r.add_value(XDataValue::Real(invert));
    r.add_value(XDataValue::Real(rim));
    r.add_value(XDataValue::Real(area));
    r.add_value(XDataValue::Real(c));
    r.add_value(XDataValue::Real(tc_inlet));
    r
}

/// XDATA record for a pipe, linking the two structures it connects by handle.
pub fn pipe_xdata(diameter: f64, n: f64, from: Handle, to: Handle) -> ExtendedDataRecord {
    let mut r = ExtendedDataRecord::new(APP_PIPE);
    r.add_value(XDataValue::Real(diameter));
    r.add_value(XDataValue::Real(n));
    r.add_value(XDataValue::Handle(from));
    r.add_value(XDataValue::Handle(to));
    r
}

/// XDATA for a catchment drainage polygon.
/// `inlet_handle` = `Handle::NULL` for auto-assign to nearest inlet/junction.
pub fn catchment_xdata(c: f64, flow_length_ft: f64, slope: f64, inlet_handle: Handle) -> ExtendedDataRecord {
    let mut r = ExtendedDataRecord::new(APP_CATCHMENT);
    r.add_value(XDataValue::Real(c));
    r.add_value(XDataValue::Real(flow_length_ft));
    r.add_value(XDataValue::Real(slope));
    r.add_value(XDataValue::Handle(inlet_handle));
    r
}

/// In-place replace (or insert) of an XDATA app record by application name.
/// Avoids full records list clones/filters in hot paths (apply_tc, set_dia).
/// Unifies the two apply_tc paths and eliminates .cloned() on record for replace.
/// Small N (1-3 records/entity) so tiny Vec clone of records is acceptable; main win
/// is avoiding full-entity Vec clones in dispatch.
pub(crate) fn replace_xdata_record(
    xd: &mut acadrust::xdata::ExtendedData,
    app_name: &str,
    new_rec: ExtendedDataRecord,
) {
    // Proper replace by app name using acadrust API (clear + re-add kept + new).
    // Clones only the (tiny) kept XDATA records per entity (N<=1 for our HYDROCOMPLETE_* apps).
    // This eliminates the prior filter+clone patterns for record replacement (Issue 9).
    // Unifies apply paths; callers no longer need .cloned() on the found old record.
    let kept: Vec<ExtendedDataRecord> = xd.records()
        .iter()
        .filter(|r| r.application_name != app_name)
        .cloned()
        .collect();
    xd.clear();
    for k in kept {
        xd.add_record(k);
    }
    xd.add_record(new_rec);
}

fn real(v: &XDataValue) -> Option<f64> {
    if let XDataValue::Real(x) = v {
        Some(*x)
    } else {
        None
    }
}

fn handle(v: &XDataValue) -> Option<Handle> {
    if let XDataValue::Handle(h) = v {
        Some(*h)
    } else {
        None
    }
}

/// Public structure fields for placement / edit / validation.
#[derive(Clone, Debug)]
pub struct StructureInfo {
    pub handle: Handle,
    pub kind: NodeKind,
    pub invert: f64,
    pub rim: f64,
    pub area: f64,
    pub c: f64,
    pub tc_inlet: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug)]
pub struct PipeInfo {
    pub handle: Handle,
    pub diameter: f64,
    pub n: f64,
    pub from: Handle,
    pub to: Handle,
    pub length: f64,
}

struct StructRec {
    handle: Handle,
    kind: NodeKind,
    invert: f64,
    rim: f64,
    area: f64,
    c: f64,
    tc_inlet: f64,
    x: f64,
    y: f64,
}

impl From<StructRec> for StructureInfo {
    fn from(s: StructRec) -> Self {
        Self {
            handle: s.handle,
            kind: s.kind,
            invert: s.invert,
            rim: s.rim,
            area: s.area,
            c: s.c,
            tc_inlet: s.tc_inlet,
            x: s.x,
            y: s.y,
        }
    }
}

#[derive(Clone)]
struct PipeRec {
    diameter: f64,
    n: f64,
    from: Handle,
    to: Handle,
    length: f64,
}

struct CatchmentRec {
    c: f64,
    flow_length_ft: f64,
    slope: f64,
    inlet_handle: Option<Handle>,
    area_ac: f64,
    centroid: (f64, f64),
}

/// True when the entity is a tagged storm-sewer structure circle.
pub fn is_structure_entity(e: &EntityType) -> bool {
    read_structure(e).is_some()
}

/// Storm structure resolved from a plan click (center + kind).
#[derive(Clone, Debug)]
pub struct StructurePick {
    pub handle: Handle,
    pub kind: NodeKind,
    pub x: f64,
    pub y: f64,
}

impl StructurePick {
    pub fn label(&self) -> &'static str {
        match self.kind {
            NodeKind::Inlet => "Inlet",
            NodeKind::Junction => "Junction",
            NodeKind::Outfall => "Outfall",
        }
    }
}

/// Nearest storm structure within click tolerance of `(x, y)` (ft).
/// `pick_padding_ft` is added to each marker circle's radius.
pub fn structure_at_point<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    x: f64,
    y: f64,
    pick_padding_ft: f64,
    include_outfalls: bool,
) -> Option<StructurePick> {
    let mut best: Option<(StructurePick, f64)> = None;
    for e in entities {
        let s = read_structure(e)?;
        if !include_outfalls && s.kind == NodeKind::Outfall {
            continue;
        }
        let radius = match e {
            EntityType::Circle(c) => c.radius,
            _ => 3.0,
        };
        let dx = s.x - x;
        let dy = s.y - y;
        let dist = (dx * dx + dy * dy).sqrt();
        let limit = radius + pick_padding_ft;
        if dist <= limit {
            if best.as_ref().map(|(_, d)| dist < *d).unwrap_or(true) {
                best = Some((
                    StructurePick {
                        handle: s.handle,
                        kind: s.kind,
                        x: s.x,
                        y: s.y,
                    },
                    dist,
                ));
            }
        }
    }
    best.map(|(p, _)| p)
}

pub fn nearest_structure_at_point<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    x: f64,
    y: f64,
    pick_padding_ft: f64,
    include_outfalls: bool,
) -> Option<Handle> {
    structure_at_point(entities, x, y, pick_padding_ft, include_outfalls).map(|p| p.handle)
}

/// Nearest inlet/junction only (for catchment drainage targets).
pub fn nearest_drainage_structure_at_point<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    x: f64,
    y: f64,
    pick_padding_ft: f64,
) -> Option<Handle> {
    nearest_structure_at_point(entities, x, y, pick_padding_ft, false)
}

pub fn read_structure_info(e: &EntityType) -> Option<StructureInfo> {
    read_structure(e).map(StructureInfo::from)
}

pub fn read_pipe_info(e: &EntityType) -> Option<PipeInfo> {
    let p = read_pipe(e)?;
    Some(PipeInfo {
        handle: e.common().handle,
        diameter: p.diameter,
        n: p.n,
        from: p.from,
        to: p.to,
        length: p.length,
    })
}

pub fn write_structure_info(e: &mut EntityType, info: &StructureInfo) {
    let EntityType::Circle(c) = e else {
        return;
    };
    c.center.x = info.x;
    c.center.y = info.y;
    let (area, c_val) = if info.kind == NodeKind::Outfall {
        (0.0, 0.0)
    } else {
        (info.area, info.c)
    };
    let xd = &mut e.common_mut().extended_data;
    replace_xdata_record(
        xd,
        APP_STRUCT,
        structure_xdata_tc(info.kind, info.invert, info.rim, area, c_val, info.tc_inlet),
    );
}

pub(crate) fn replace_pipe_xdata(xd: &mut acadrust::xdata::ExtendedData, record: ExtendedDataRecord) {
    replace_xdata_record(xd, APP_PIPE, record);
}

fn read_structure(e: &EntityType) -> Option<StructRec> {
    let rec = e.common().extended_data.get_record(APP_STRUCT)?;
    if rec.values.len() < 5 {
        return None;
    }
    let kind = match &rec.values[0] {
        XDataValue::String(s) => parse_kind(s),
        _ => return None,
    };
    let (x, y) = match e {
        EntityType::Circle(c) => (c.center.x, c.center.y),
        _ => return None,
    };
    let tc_inlet = if rec.values.len() >= 6 {
        real(&rec.values[5]).unwrap_or(10.0)
    } else {
        10.0
    };
    // NOTE (review Issue 13): lenient default Tc=10.0 (or elev=0.0 below) for robustness on missing/hand-edited XDATA.
    // Callers in analysis/dispatch can surface via host.push_info("using default Tc...") or a validation result.
    // See network_from_entities + PLUGIN.md.
    Some(StructRec {
        handle: e.common().handle,
        kind,
        invert: real(&rec.values[1])?,
        rim: real(&rec.values[2])?,
        area: real(&rec.values[3])?,
        c: real(&rec.values[4])?,
        tc_inlet,
        x,
        y,
    })
}

fn read_pipe(e: &EntityType) -> Option<PipeRec> {
    let rec = e.common().extended_data.get_record(APP_PIPE)?;
    if rec.values.len() < 4 {
        return None;
    }
    let length = match e {
        EntityType::Line(l) => {
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            (dx * dx + dy * dy).sqrt()
        }
        _ => return None,
    };
    Some(PipeRec {
        diameter: real(&rec.values[0])?,
        n: real(&rec.values[1])?,
        from: handle(&rec.values[2])?,
        to: handle(&rec.values[3])?,
        length,
    })
}

fn polyline_vertices(pl: &LwPolyline) -> Vec<(f64, f64)> {
    pl.vertices.iter().map(|v| (v.location.x, v.location.y)).collect()
}

fn read_catchment(e: &EntityType) -> Option<CatchmentRec> {
    let EntityType::LwPolyline(pl) = e else {
        return None;
    };
    if !pl.is_closed || pl.vertices.len() < 3 {
        return None;
    }
    let rec = e.common().extended_data.get_record(APP_CATCHMENT)?;
    if rec.values.len() < 4 {
        return None;
    }
    let verts = polyline_vertices(pl);
    let area_ac = sqft_to_acres(shoelace_area_sqft(&verts));
    let inlet = handle(&rec.values[3]).filter(|h| !h.is_null());
    Some(CatchmentRec {
        c: real(&rec.values[0])?,
        flow_length_ft: real(&rec.values[1])?,
        slope: real(&rec.values[2])?,
        inlet_handle: inlet,
        area_ac,
        centroid: polygon_centroid(&verts),
    })
}

/// Replace the diameter on a storm-sewer pipe line entity.
pub fn set_pipe_diameter(e: &mut EntityType, new_dia: f64) -> bool {
    let EntityType::Line(_) = e else {
        return false;
    };
    let xd = &mut e.common_mut().extended_data;
    let Some(old) = xd.records().iter().find(|r| r.application_name == APP_PIPE) else {
        return false;
    };
    if old.values.len() < 4 {
        return false;
    };
    let Some(n) = real(&old.values[1]) else {
        return false;
    };
    let Some(from) = handle(&old.values[2]) else {
        return false;
    };
    let Some(to) = handle(&old.values[3]) else {
        return false;
    };
    replace_xdata_record(xd, APP_PIPE, pipe_xdata(new_dia, n, from, to));
    true
}

/// Recompute Tc from catchments and write onto structure entities in the document.
pub fn apply_tc_in_document<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    entities_mut: impl Iterator<Item = &'a mut EntityType>,
) -> Result<usize, String> {
    let drawn = drawn_network_from_entities(entities)?;
    let mut tc_by_handle: HashMap<Handle, f64> = HashMap::new();
    for (node, &h) in drawn.network.nodes.iter().zip(drawn.node_handles.iter()) {
        if node.kind != NodeKind::Outfall {
            tc_by_handle.insert(h, node.tc_inlet);
        }
    }
    let mut updated = 0;
    for ent in entities_mut {
        let h = ent.common().handle;
        let Some(&tc) = tc_by_handle.get(&h) else {
            continue;
        };
        let EntityType::Circle(_) = ent else { continue };
        let xd = &mut ent.common_mut().extended_data;
        let Some(old) = xd.records().iter().find(|r| r.application_name == APP_STRUCT) else {
            continue;
        };
        if old.values.len() < 5 {
            continue;
        }
        let kind = match &old.values[0] {
            XDataValue::String(s) => parse_kind(s),
            _ => continue,
        };
        let invert = real(&old.values[1]).unwrap_or(0.0);
        let rim = real(&old.values[2]).unwrap_or(0.0);
        let area = real(&old.values[3]).unwrap_or(0.0);
        let c = real(&old.values[4]).unwrap_or(0.0);
        replace_xdata_record(xd, APP_STRUCT, structure_xdata_tc(kind, invert, rim, area, c, tc));
        updated += 1;
    }
    Ok(updated)
}

/// Write computed inlet Tc back onto structure circle entities (by handle order).
pub fn apply_tc_to_structures(entities: &mut [EntityType], drawn: &DrawnNetwork) -> usize {
    let mut updated = 0;
    for (node, &h) in drawn.network.nodes.iter().zip(drawn.node_handles.iter()) {
        if node.kind == NodeKind::Outfall {
            continue;
        }
        let Some(ent) = entities.iter_mut().find(|e| e.common().handle == h) else {
            continue;
        };
        let EntityType::Circle(_) = ent else { continue };
        let xd = &mut ent.common_mut().extended_data;
        let Some(old) = xd.records().iter().find(|r| r.application_name == APP_STRUCT) else {
            continue;
        };
        if old.values.len() < 5 {
            continue;
        }
        let kind = match &old.values[0] {
            XDataValue::String(s) => parse_kind(s),
            _ => continue,
        };
        let invert = real(&old.values[1]).unwrap_or(node.invert);
        let rim = real(&old.values[2]).unwrap_or(node.rim);
        let area = real(&old.values[3]).unwrap_or(node.area_ac);
        let c = real(&old.values[4]).unwrap_or(node.c);
        replace_xdata_record(xd, APP_STRUCT, structure_xdata_tc(kind, invert, rim, area, c, node.tc_inlet));
        updated += 1;
    }
    updated
}

fn nearest_drainage_structure(structs: &[StructRec], point: (f64, f64)) -> Option<usize> {
    structs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind != NodeKind::Outfall)
        .min_by(|(_, a), (_, b)| {
            let da = (a.x - point.0).powi(2) + (a.y - point.1).powi(2);
            let db = (b.x - point.0).powi(2) + (b.y - point.1).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
}

fn apply_catchments(structs: &mut [StructRec], catchments: &[CatchmentRec]) {
    for cat in catchments {
        let target = if let Some(h) = cat.inlet_handle {
            structs.iter().position(|s| s.handle == h)
        } else {
            nearest_drainage_structure(structs, cat.centroid)
        };
        let Some(idx) = target else { continue };
        let s = &mut structs[idx];
        let local_ca = s.c * s.area;
        let add_ca = cat.c * cat.area_ac;
        let total_area = s.area + cat.area_ac;
        if total_area > 0.0 {
            s.area = total_area;
            s.c = (local_ca + add_ca) / total_area;
        }
        let flow_len = if cat.flow_length_ft > 0.0 {
            cat.flow_length_ft
        } else {
            default_flow_length_ft(cat.centroid, (s.x, s.y))
        };
        let slope = if cat.slope > 0.0 { cat.slope } else { 0.01 };
        let tc = catchment_tc_minutes(flow_len, slope);
        s.tc_inlet = s.tc_inlet.max(tc);
    }
}

/// Build a [`DrawnNetwork`] from entities. Structures become nodes (N1, N2, …);
/// pipes become links mapped by structure handles in XDATA.
pub fn drawn_network_from_entities<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<DrawnNetwork, String> {
    let mut structs: Vec<StructRec> = Vec::new();
    let mut pipes_raw: Vec<(Handle, PipeRec)> = Vec::new();
    let mut catchments: Vec<CatchmentRec> = Vec::new();
    for e in entities {
        if let Some(s) = read_structure(e) {
            structs.push(s);
        } else if let Some(p) = read_pipe(e) {
            pipes_raw.push((e.common().handle, p));
        } else if let Some(c) = read_catchment(e) {
            catchments.push(c);
        }
    }
    apply_catchments(&mut structs, &catchments);
    let network = assemble_network(&structs, &pipes_raw.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>())?;
    Ok(DrawnNetwork {
        network,
        node_handles: structs.iter().map(|s| s.handle).collect(),
        pipe_handles: pipes_raw.iter().map(|(h, _)| *h).collect(),
    })
}

/// Build a `stormsewer::Network` from drawn entities. Structures become nodes
/// (named N1, N2, … in encounter order); pipes become links, mapped to nodes
/// by the handles stored in their XDATA.
pub fn network_from_entities<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Network, String> {
    Ok(drawn_network_from_entities(entities)?.network)
}

/// Apply precomputed Tc values (by structure handle) to matching circle entities' XDATA.
/// Uses the replace helper (no record clones for update). Unifies apply paths.
/// Called after building Tc map from a read pass (avoids simultaneous borrow at call sites).
pub fn apply_tc_map<'a>(
    entities_mut: impl Iterator<Item = &'a mut EntityType>,
    tc_by_handle: &HashMap<Handle, f64>,
) -> usize {
    let mut updated = 0;
    for ent in entities_mut {
        let h = ent.common().handle;
        let Some(&tc) = tc_by_handle.get(&h) else { continue; };
        let EntityType::Circle(_) = ent else { continue };
        let xd = &mut ent.common_mut().extended_data;
        let Some(old) = xd.records().iter().find(|r| r.application_name == APP_STRUCT) else { continue; };
        if old.values.len() < 5 { continue; }
        let kind = match &old.values[0] {
            XDataValue::String(s) => parse_kind(s),
            _ => continue,
        };
        let invert = real(&old.values[1]).unwrap_or(0.0);
        let rim = real(&old.values[2]).unwrap_or(0.0);
        let area = real(&old.values[3]).unwrap_or(0.0);
        let c = real(&old.values[4]).unwrap_or(0.0);
        replace_xdata_record(xd, APP_STRUCT, structure_xdata_tc(kind, invert, rim, area, c, tc));
        updated += 1;
    }
    updated
}

fn assemble_network(structs: &[StructRec], pipes_raw: &[PipeRec]) -> Result<Network, String> {
    if structs.is_empty() {
        return Err("No storm-sewer structures in the drawing — place Inlet/Junction/Outfall first.".into());
    }

    let mut id_of: HashMap<u64, String> = HashMap::new();
    let mut nodes = Vec::with_capacity(structs.len());
    for (idx, s) in structs.iter().enumerate() {
        let id = format!("N{}", idx + 1);
        id_of.insert(s.handle.value(), id.clone());
        let node = match s.kind {
            NodeKind::Inlet => Node::inlet(&id, s.invert, s.rim, s.area, s.c),
            NodeKind::Junction => Node::junction(&id, s.invert, s.rim, s.area, s.c),
            NodeKind::Outfall => Node::outfall(&id, s.invert, s.rim),
        }
        .with_tc_inlet(s.tc_inlet)
        .at(s.x, s.y);
        nodes.push(node);
    }

    let mut pipes = Vec::new();
    let mut dropped = 0;
    for (k, p) in pipes_raw.iter().enumerate() {
        match (id_of.get(&p.from.value()), id_of.get(&p.to.value())) {
            (Some(f), Some(t)) => {
                pipes.push(Pipe::new(&format!("P{}", k + 1), f, t, p.length, p.diameter, p.n));
            }
            _ => dropped += 1,
        }
    }
    if pipes.is_empty() {
        return Err(format!(
            "No connected storm-sewer pipes ({} structure(s) found, {} dangling pipe(s)). Use Pipe Run to connect structures.",
            structs.len(), dropped
        ));
    }
    Ok(Network { nodes, pipes })
}

// ── HydroComplete.Civil3D parity helpers (network summary, pipe segments) ──

#[derive(Debug, Clone)]
pub struct NetworkSummary {
    pub network_name: String,
    pub pipe_count: usize,
    pub structure_count: usize,
    pub total_length_ft: f64,
    pub min_diameter_in: f64,
    pub max_diameter_in: f64,
    pub has_pipes: bool,
}

#[derive(Debug, Clone)]
pub struct DrawnPipe {
    pub name: String,
    pub segment: hydrocomplete::models::PipeSegment,
    pub design_q_cfs: f64,
    pub mid_x: f64,
    pub mid_y: f64,
}

#[derive(Debug, Clone)]
pub struct CatchmentInfo {
    pub name: String,
    pub area_acres: f64,
    pub runoff_c: f64,
    pub curve_number: f64,
    pub tc_minutes: f64,
}

pub fn network_summaries<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
) -> Result<Vec<NetworkSummary>, String> {
    let drawn = drawn_network_from_entities(entities)?;
    Ok(vec![NetworkSummary {
        network_name: "default".to_string(),
        pipe_count: drawn.network.pipes.len(),
        structure_count: drawn.network.nodes.len(),
        total_length_ft: drawn.network.pipes.iter().map(|p| p.length).sum(),
        min_diameter_in: drawn
            .network
            .pipes
            .iter()
            .map(|p| p.diameter * 12.0)
            .fold(f64::INFINITY, f64::min)
            .min(f64::INFINITY)
            .max(0.0),
        max_diameter_in: drawn
            .network
            .pipes
            .iter()
            .map(|p| p.diameter * 12.0)
            .fold(0.0_f64, f64::max),
        has_pipes: !drawn.network.pipes.is_empty(),
    }])
}

pub fn pipe_segments<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
) -> Result<Vec<DrawnPipe>, String> {
    let drawn = drawn_network_from_entities(entities)?;
    pipe_segments_from_drawn(&drawn, None)
}

pub fn pipe_segments_from_drawn(
    drawn: &DrawnNetwork,
    analysis: Option<&stormsewer::network::Analysis>,
) -> Result<Vec<DrawnPipe>, String> {
    let mut id_to_xy: std::collections::HashMap<String, (f64, f64)> = std::collections::HashMap::new();
    for (node, &h) in drawn.network.nodes.iter().zip(drawn.node_handles.iter()) {
        if let Some(s) = drawn
            .network
            .nodes
            .iter()
            .find(|n| n.id == node.id)
        {
            id_to_xy.insert(node.id.clone(), (s.x, s.y));
        }
        let _ = h;
    }
    for node in &drawn.network.nodes {
        id_to_xy.insert(node.id.clone(), (node.x, node.y));
    }

    let mut out = Vec::new();
    for (idx, pipe) in drawn.network.pipes.iter().enumerate() {
        let (x1, y1) = id_to_xy.get(&pipe.from).copied().unwrap_or((0.0, 0.0));
        let (x2, y2) = id_to_xy.get(&pipe.to).copied().unwrap_or((0.0, 0.0));
        let slope = if pipe.length > 0.0 {
            let up = drawn
                .network
                .nodes
                .iter()
                .find(|n| n.id == pipe.from)
                .map(|n| n.invert)
                .unwrap_or(0.0);
            let down = drawn
                .network
                .nodes
                .iter()
                .find(|n| n.id == pipe.to)
                .map(|n| n.invert)
                .unwrap_or(0.0);
            ((up - down) / pipe.length).max(0.0001)
        } else {
            0.001
        };
        let q_des = analysis
            .and_then(|a| a.pipes.iter().find(|p| p.id == pipe.id))
            .map(|p| p.design_q)
            .unwrap_or(0.0);
        let up_inv = drawn
            .network
            .nodes
            .iter()
            .find(|n| n.id == pipe.from)
            .map(|n| n.invert)
            .unwrap_or(0.0);
        let down_inv = drawn
            .network
            .nodes
            .iter()
            .find(|n| n.id == pipe.to)
            .map(|n| n.invert)
            .unwrap_or(0.0);
        out.push(DrawnPipe {
            name: format!("P{}", idx + 1),
            segment: hydrocomplete::models::PipeSegment {
                name: pipe.id.clone(),
                shape: hydrocomplete::models::PipeShape::Circular,
                diameter_ft: pipe.diameter,
                width_ft: 0.0,
                height_ft: 0.0,
                span_ft: 0.0,
                rise_ft: 0.0,
                slope,
                manning_n: pipe.n,
                design_flow_cfs: q_des,
                length_ft: pipe.length,
                start_invert_ft: up_inv,
                end_invert_ft: down_inv,
            },
            design_q_cfs: q_des,
            mid_x: (x1 + x2) / 2.0,
            mid_y: (y1 + y2) / 2.0,
        });
    }
    Ok(out)
}

pub fn catchments_from_entities<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Vec<CatchmentInfo> {
    let mut out = Vec::new();
    let mut idx = 0;
    for e in entities {
        if let EntityType::LwPolyline(poly) = e {
            let xd = &e.common().extended_data;
            let Some(rec) = xd.get_record(APP_CATCHMENT) else { continue };
            if rec.values.len() < 3 {
                continue;
            }
            let c = real(&rec.values[0]).unwrap_or(0.7);
            let flow_len = real(&rec.values[1]).unwrap_or(300.0);
            let slope = real(&rec.values[2]).unwrap_or(0.01);
            let verts = polyline_vertices(poly);
            let area_sqft = shoelace_area_sqft(&verts);
            let area_ac = sqft_to_acres(area_sqft);
            let tc = catchment_tc_minutes(flow_len, slope);
            idx += 1;
            out.push(CatchmentInfo {
                name: format!("C{idx}"),
                area_acres: area_ac,
                runoff_c: c,
                curve_number: 0.0,
                tc_minutes: tc,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use acadrust::entities::LwVertex;
    use acadrust::types::{Vector2, Vector3};
    use acadrust::{Circle, Line, LwPolyline};

    fn structure(h: u64, kind: NodeKind, x: f64, invert: f64) -> EntityType {
        let mut e = EntityType::Circle(Circle { center: Vector3::new(x, 0.0, 0.0), radius: 3.0, ..Default::default() });
        e.common_mut().handle = Handle::new(h);
        e.common_mut().extended_data.add_record(structure_xdata(kind, invert, invert + 6.0, 1.0, 0.7));
        e
    }
    fn pipe(from: u64, to: u64, x1: f64, x2: f64) -> EntityType {
        let mut e = EntityType::Line(Line::from_points(Vector3::new(x1, 0.0, 0.0), Vector3::new(x2, 0.0, 0.0)));
        e.common_mut().extended_data.add_record(pipe_xdata(1.5, 0.013, Handle::new(from), Handle::new(to)));
        e
    }

    fn catchment_poly(h: u64, c: f64, inlet: u64) -> EntityType {
        let mut pl = LwPolyline::default();
        pl.is_closed = true;
        pl.vertices = vec![
            LwVertex::new(Vector2::new(40.0, -20.0)),
            LwVertex::new(Vector2::new(60.0, -20.0)),
            LwVertex::new(Vector2::new(60.0, 20.0)),
            LwVertex::new(Vector2::new(40.0, 20.0)),
        ];
        let mut e = EntityType::LwPolyline(pl);
        e.common_mut().handle = Handle::new(h);
        e.common_mut().extended_data.add_record(catchment_xdata(
            c,
            2500.0,
            0.02,
            if inlet == 0 { Handle::NULL } else { Handle::new(inlet) },
        ));
        e
    }

    #[test]
    fn reconstructs_network_from_tagged_entities() {
        let ents = vec![
            structure(1, NodeKind::Inlet, 0.0, 104.0),
            structure(2, NodeKind::Outfall, 100.0, 100.0),
            pipe(1, 2, 0.0, 100.0),
        ];
        let net = network_from_entities(ents.iter()).unwrap();
        assert_eq!(net.nodes.len(), 2);
        assert_eq!(net.pipes.len(), 1);
        assert_eq!(net.pipes[0].from, "N1");
        assert_eq!(net.pipes[0].to, "N2");
        assert!((net.pipes[0].length - 100.0).abs() < 1e-6);
        assert!((net.pipes[0].diameter - 1.5).abs() < 1e-9);
    }

    #[test]
    fn catchment_adds_area_and_tc_to_inlet() {
        let ents = vec![
            structure(1, NodeKind::Inlet, 0.0, 104.0),
            structure(2, NodeKind::Outfall, 100.0, 100.0),
            pipe(1, 2, 0.0, 100.0),
            catchment_poly(10, 0.8, 1),
        ];
        let net = network_from_entities(ents.iter()).unwrap();
        let n1 = &net.nodes[0];
        assert!(n1.area_ac > 1.0, "catchment should add area, got {}", n1.area_ac);
        assert!(n1.tc_inlet > 10.0, "Kirpich tc should exceed default 10 min");
    }

    #[test]
    fn errors_when_no_structures() {
        let ents: Vec<EntityType> = vec![];
        assert!(network_from_entities(ents.iter()).is_err());
    }

    #[test]
    fn set_pipe_diameter_updates_xdata() {
        let mut e = pipe(1, 2, 0.0, 100.0);
        assert!(set_pipe_diameter(&mut e, 2.0));
        let rec = e.common().extended_data.get_record(APP_PIPE).unwrap();
        assert!((real(&rec.values[0]).unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn nearest_structure_respects_click_tolerance() {
        let ents = vec![
            structure(1, NodeKind::Inlet, 0.0, 104.0),
            structure(2, NodeKind::Outfall, 100.0, 100.0),
        ];
        assert_eq!(
            nearest_structure_at_point(ents.iter(), 1.0, 0.0, 5.0, true),
            Some(Handle::new(1))
        );
        assert!(nearest_drainage_structure_at_point(ents.iter(), 100.0, 0.0, 5.0).is_none());
        assert_eq!(
            nearest_drainage_structure_at_point(ents.iter(), 1.0, 0.0, 5.0),
            Some(Handle::new(1))
        );
    }

    #[test]
    fn dangling_pipe_is_reported() {
        let ents = vec![
            structure(1, NodeKind::Inlet, 0.0, 104.0),
            pipe(1, 99, 0.0, 100.0), // 99 doesn't exist
        ];
        assert!(network_from_entities(ents.iter()).is_err());
    }
}