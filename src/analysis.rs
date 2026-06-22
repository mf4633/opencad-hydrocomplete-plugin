// Bridge between Open CAD Studio and the `HydroComplete` engine crate.
//
// Runs the hydrology/hydraulics engine on a network and turns the result into
// (a) acadrust entities to draw into the document and (b) a text report for the
// command line. Networks come from a `.ssn` file (see `stormsewer::parse`); a
// built-in sample backs the unit tests and the zero-argument fallbacks.

use acadrust::types::Vector3;
use acadrust::{Circle, EntityType, Line, MText};

use stormsewer::design::{check_inlet, design_review, DesignFinding, ReviewCriteria};
use stormsewer::drawing::{draw_network, DrawConfig};
use stormsewer::idf::IdfCurve;
use stormsewer::network::{Analysis, AnalysisOptions, Network, Node, NodeKind, Pipe};
use stormsewer::params::StormAnalysisParams;
use stormsewer::parse::parse_ssn;
use stormsewer::report::{format_analysis, format_multi_rp};

use super::data;

/// Default network-level analysis parameters (tests / fallbacks).
pub fn default_params() -> StormAnalysisParams {
    StormAnalysisParams::municipal()
}

/// Annotation labels only (flow + HGL), for overlaying on an already-drawn
/// network without re-drawing its geometry.
fn build_annotations(net: &Network, a: &Analysis) -> Vec<EntityType> {
    draw_network(net, a, &DrawConfig::default())
        .plan_labels
        .iter()
        .map(|l| label(l.x, l.y, l.text.clone(), l.height))
        .collect()
}

// ── Public API: analyze the network drawn in the active document ────────────

/// Reconstruct the network from drawn entities, analyze it, and return
/// annotation entities (flow/HGL labels) + the report.
pub fn analyze_doc<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<(Vec<EntityType>, String, Analysis), String> {
    let net = data::network_from_entities(entities)?;
    let a = run_analysis(&net, params.idf.design_curve(), &params.hydraulics)?;
    let report = full_report(&net, &a, params);
    Ok((build_annotations(&net, &a), report, a))
}

/// Reconstruct from drawn entities and return the HGL long-section entities.
pub fn profile_doc<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<Vec<EntityType>, String> {
    let net = data::network_from_entities(entities)?;
    let a = run_analysis(&net, params.idf.design_curve(), &params.hydraulics)?;
    Ok(build_profile(&net, &a))
}

/// Reconstruct from drawn entities and return the formatted report.
pub fn report_doc<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<String, String> {
    let net = data::network_from_entities(entities)?;
    let a = run_analysis(&net, params.idf.design_curve(), &params.hydraulics)?;
    Ok(full_report(&net, &a, params))
}

/// Reconstruct from drawn entities, analyze, and run the design-criteria review
/// (velocity / cover / slope / capacity / size-progression / surface flooding).
pub fn design_review_doc<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<Vec<DesignFinding>, String> {
    let net = data::network_from_entities(entities)?;
    let a = run_analysis(&net, params.idf.design_curve(), &params.hydraulics)?;
    Ok(design_review(&net, &a, &ReviewCriteria::default()))
}

/// Multi-return-period peak-flow comparison table.
pub fn multi_rp_report<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<String, String> {
    let net = data::network_from_entities(entities)?;
    Ok(format_multi_rp(&net, &params.idf, &params.hydraulics))
}

fn full_report(net: &Network, a: &Analysis, params: &StormAnalysisParams) -> String {
    let mut s = format_analysis(a);
    s.push_str(&inlet_section(net, a, params));
    s
}

fn inlet_section(net: &Network, a: &Analysis, params: &StormAnalysisParams) -> String {
    let mut s = String::new();
    let mut any = false;
    for nd in &net.nodes {
        if nd.kind != NodeKind::Inlet {
            continue;
        }
        let q = a
            .pipes
            .iter()
            .filter(|p| p.from == nd.id)
            .map(|p| p.design_q)
            .fold(0.0f64, f64::max);
        if q <= 0.0 {
            continue;
        }
        if !any {
            s.push_str("\n=== INLET CAPACITY (HEC-22 grate, simplified) ===\n");
            s.push_str("Node   Q(cfs)  Cap(cfs)  Status\n");
            any = true;
        }
        let chk = check_inlet(
            q,
            params.inlet_grate_length_ft,
            params.inlet_flow_depth_ft,
            params.inlet_gutter_slope,
        );
        let status = if chk.ok { "ok" } else { "BYPASS" };
        s.push_str(&format!(
            "{:<6} {:>6.2} {:>8.2}  {status}\n",
            nd.id, chk.design_q_cfs, chk.capacity_cfs
        ));
    }
    s
}

/// The built-in demonstration network (properly sized, with plan coordinates).
fn demo() -> (Network, IdfCurve, AnalysisOptions) {
    let net = Network {
        nodes: vec![
            Node::inlet("N1", 104.0, 110.0, 1.0, 0.70).with_tc_inlet(12.0).at(0.0, 0.0),
            Node::inlet("N2", 102.5, 108.5, 1.0, 0.70).with_tc_inlet(10.0).at(300.0, 0.0),
            Node::junction("N3", 101.2, 107.0, 0.5, 0.80).with_tc_inlet(8.0).at(550.0, 0.0),
            Node::outfall("OUT", 100.0, 106.0).at(730.0, 0.0),
        ],
        pipes: vec![
            Pipe::new("P1", "N1", "N2", 300.0, 1.25, 0.013),
            Pipe::new("P2", "N2", "N3", 250.0, 1.50, 0.013),
            Pipe::new("P3", "N3", "OUT", 180.0, 1.75, 0.013),
        ],
    };
    let idf = IdfCurve::new(60.0, 10.0, 0.8);
    let opts = AnalysisOptions { tailwater: Some(100.5), ..Default::default() };
    (net, idf, opts)
}

fn v3(x: f64, y: f64) -> Vector3 {
    Vector3::new(x, y, 0.0)
}

fn seg(x1: f64, y1: f64, x2: f64, y2: f64) -> EntityType {
    EntityType::Line(Line::from_points(v3(x1, y1), v3(x2, y2)))
}

fn label(x: f64, y: f64, text: String, height: f64) -> EntityType {
    EntityType::MText(MText { value: text, insertion_point: v3(x, y), height, ..Default::default() })
}

fn circle(x: f64, y: f64, r: f64) -> EntityType {
    EntityType::Circle(Circle { center: v3(x, y), radius: r, ..Default::default() })
}

fn run_analysis(net: &Network, idf: &IdfCurve, opts: &AnalysisOptions) -> Result<Analysis, String> {
    net.analyze(idf, opts).map_err(|e| e.to_string())
}

/// Run analysis with tab-level parameters (used by HC_CAPACITY and write-back).
pub fn run_analysis_on_network(
    net: &Network,
    params: &StormAnalysisParams,
) -> Result<Analysis, String> {
    run_analysis(net, params.idf.design_curve(), &params.hydraulics)
}

/// Plan-view entities (pipes, structure markers, flow/HGL labels) + the report.
fn build_plan(net: &Network, a: &Analysis) -> (Vec<EntityType>, String) {
    let d = draw_network(net, a, &DrawConfig::default());
    let mut ents = Vec::new();
    for p in &d.plan_pipes {
        ents.push(seg(p.x1, p.y1, p.x2, p.y2));
    }
    for n in &d.plan_nodes {
        ents.push(circle(n.x, n.y, n.radius));
    }
    for l in &d.plan_labels {
        ents.push(label(l.x, l.y, l.text.clone(), l.height));
    }
    (ents, format_analysis(a))
}

/// HGL long-section entities (ground / invert / HGL polylines + labels).
fn build_profile(net: &Network, a: &Analysis) -> Vec<EntityType> {
    let d = draw_network(net, a, &DrawConfig::default());
    let mut ents = Vec::new();
    for pl in &d.profile_lines {
        for w in pl.pts.windows(2) {
            ents.push(seg(w[0].0, w[0].1, w[1].0, w[1].1));
        }
    }
    for l in &d.profile_labels {
        ents.push(label(l.x, l.y, l.text.clone(), l.height));
    }
    ents
}

// ── Public API: from a `.ssn` document ──────────────────────────────────────

/// Parse a `.ssn` document, analyze it, and return (plan entities, report).
pub fn analyze_text(text: &str) -> Result<(Vec<EntityType>, String), String> {
    let p = parse_ssn(text)?;
    let a = run_analysis(&p.network, &p.idf, &p.options)?;
    Ok(build_plan(&p.network, &a))
}

/// Parse a `.ssn` document, analyze it, and return the HGL profile entities.
pub fn profile_text(text: &str) -> Result<Vec<EntityType>, String> {
    let p = parse_ssn(text)?;
    let a = run_analysis(&p.network, &p.idf, &p.options)?;
    Ok(build_profile(&p.network, &a))
}

/// Parse a `.ssn` document, analyze it, and return the formatted report.
pub fn report_text_from(text: &str) -> Result<String, String> {
    let p = parse_ssn(text)?;
    let a = run_analysis(&p.network, &p.idf, &p.options)?;
    Ok(format_analysis(&a))
}

// ── Built-in sample fallbacks (used by tests) ───────────────────────────────

/// Plan entities + report for the built-in sample network.
pub fn analyze_plan() -> Result<(Vec<EntityType>, String), String> {
    let (net, idf, opts) = demo();
    let a = run_analysis(&net, &idf, &opts)?;
    Ok(build_plan(&net, &a))
}

/// Profile entities for the built-in sample network.
pub fn analyze_profile() -> Result<Vec<EntityType>, String> {
    let (net, idf, opts) = demo();
    let a = run_analysis(&net, &idf, &opts)?;
    Ok(build_profile(&net, &a))
}

/// Formatted report for the built-in sample network.
pub fn report_text() -> String {
    let (net, idf, opts) = demo();
    match run_analysis(&net, &idf, &opts) {
        Ok(a) => format_analysis(&a),
        Err(e) => format!("storm-sewer analysis error: {e}"),
    }
}

/// Back-compat alias used by the module's integration test.
#[allow(dead_code)]
pub fn demo_report() -> String {
    report_text()
}
