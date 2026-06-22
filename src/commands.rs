//! Extended `HC_*` commands mirroring HydroComplete.Civil3D (drawing → engine → output).

use acadrust::EntityType;
use hydrocomplete::about::ABOUT_LINES;
use hydrocomplete::manning;
use hydrocomplete::models::{Catchment, PipeSegment, PipeShape};
use hydrocomplete::scs_runoff;
use ocs_plugin_api::host::HostApi;
use stormsewer::params::StormAnalysisParams;

use crate::analysis;
use crate::data::{self, DrawnPipe, NetworkSummary};

pub fn about_lines() -> Vec<&'static str> {
    ABOUT_LINES.to_vec()
}

pub fn network_summary_lines<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
) -> Result<Vec<String>, String> {
    let summaries = data::network_summaries(entities)?;
    if summaries.is_empty() {
        return Ok(vec!["No pipe networks found in this drawing.".into()]);
    }
    let mut lines = vec![
        format!(
            "--- HydroComplete: pipe network summary ({} network(s)) ---",
            summaries.len()
        ),
        "Network                 Pipes  Structs  Length(ft)   Dia(in) min-max".into(),
    ];
    for s in summaries {
        lines.push(format_summary_line(&s));
    }
    Ok(lines)
}

fn format_summary_line(s: &NetworkSummary) -> String {
    let dia = if s.has_pipes {
        format!("{:5.1} - {:5.1}", s.min_diameter_in, s.max_diameter_in)
    } else {
        "    —".to_string()
    };
    format!(
        "{:<22} {:5}  {:7}  {:10.1}  {}",
        trim(&s.network_name, 22),
        s.pipe_count,
        s.structure_count,
        s.total_length_ft,
        dia
    )
}

pub fn pipes_capacity_lines<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
) -> Result<Vec<String>, String> {
    let pipes = data::pipe_segments(entities)?;
    if pipes.is_empty() {
        return Ok(vec!["No pipes found in this drawing.".into()]);
    }
    let mut lines = vec![
        format!(
            "--- HydroComplete: Manning capacity ({} pipes) ---",
            pipes.len()
        ),
        "Pipe                    Shape     Dia/Span   Slope    Q_full(cfs)  V_full(fps)".into(),
    ];
    for p in &pipes {
        lines.push(format_pipe_capacity(p));
    }
    Ok(lines)
}

fn format_pipe_capacity(p: &DrawnPipe) -> String {
    let cap = manning::capacity(&p.segment);
    format!(
        "{:<22} {:9} {:>9}  {:7.4}  {:10.2}  {:10.2}",
        trim(&p.name, 22),
        shape_label(p.segment.shape),
        dimension_label(&p.segment),
        p.segment.slope,
        cap.full_flow_cfs,
        cap.full_velocity_fps
    )
}

pub fn capacity_check_lines<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<Vec<String>, String> {
    let drawn = data::drawn_network_from_entities(entities)?;
    let analysis = analysis::run_analysis_on_network(&drawn.network, params)?;
    let pipes = data::pipe_segments_from_drawn(&drawn, Some(&analysis))?;
    if pipes.is_empty() {
        return Ok(vec!["No pipes found in this drawing.".into()]);
    }
    let mut lines = vec![
        format!(
            "--- HydroComplete: design capacity check (i={:.2} in/hr, {} pipes) ---",
            params.idf.design_intensity(10.0),
            pipes.len()
        ),
        "Pipe                    Q_full   Q_des   Q_des/Q   d/D   SURCH".into(),
    ];
    let mut overloaded = 0;
    for p in &pipes {
        let cap = manning::capacity(&p.segment);
        let nd = manning::normal_depth(&p.segment, p.design_q_cfs);
        if nd.surcharged {
            overloaded += 1;
        }
        let ratio = if cap.peak_flow_cfs > 0.0 {
            p.design_q_cfs / cap.peak_flow_cfs
        } else {
            0.0
        };
        lines.push(format!(
            "{:<22} {:6.1}  {:6.1}  {:7.2}  {:5.2}  {:>5}",
            trim(&p.name, 22),
            cap.peak_flow_cfs,
            p.design_q_cfs,
            ratio,
            nd.relative_depth,
            if nd.surcharged { "*" } else { "" }
        ));
    }
    lines.push(format!(
        "  {overloaded} pipe(s) surcharged (Q > peak open-channel capacity)."
    ));
    Ok(lines)
}

pub fn rational_peak_lines<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<Vec<String>, String> {
    let catchments = data::catchments_from_entities(entities);
    if catchments.is_empty() {
        return Ok(vec![
            "No catchments found — tag closed polylines with HYDROCOMPLETE_CATCHMENT XDATA.".into(),
        ]);
    }
    let i = params.idf.design_intensity(10.0);
    let mut lines = vec![
        format!("--- HydroComplete: Rational peak Q (i={i:.3} in/hr) ---"),
        "Catchment               Area(ac)   C      Tc(min)  Q(cfs)".into(),
    ];
    let mut total = 0.0;
    for c in &catchments {
        let q = i * c.runoff_c * c.area_acres * 1.008;
        total += q;
        lines.push(format!(
            "{:<22} {:8.3}  {:5.2}  {:7.1}  {:7.2}",
            trim(&c.name, 22),
            c.area_acres,
            c.runoff_c,
            c.tc_minutes,
            q
        ));
    }
    lines.push(format!("  System total Q = {total:.2} cfs"));
    Ok(lines)
}

pub fn scs_runoff_lines<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    rainfall_in: f64,
) -> Result<Vec<String>, String> {
    let catchments = data::catchments_from_entities(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let mut lines = vec![format!(
        "--- HydroComplete: SCS runoff (P={rainfall_in:.2} in) ---"
    )];
    for c in &catchments {
        let hc = Catchment {
            name: c.name.clone(),
            area_acres: c.area_acres,
            runoff_c: c.runoff_c,
            curve_number: c.curve_number,
            tc_minutes: c.tc_minutes,
            outfall_structure_id: None,
            outfall_structure_name: None,
        };
        let r = scs_runoff::catchment_runoff(&hc, rainfall_in);
        lines.push(format!(
            "{}: CN={:.0}  Ia={:.2}\"  Q_depth={:.3}\"  Vol={:.0} cf",
            r.catchment_name,
            r.curve_number,
            r.initial_abstraction_inches,
            r.runoff_depth_inches,
            r.runoff_volume_cf
        ));
    }
    Ok(lines)
}

pub fn atlas14_lines() -> Vec<String> {
    let mut lines = vec![
        "--- HydroComplete: NOAA Atlas 14 IDF presets (embedded v0.2) ---".into(),
    ];
    const PRESETS: &[(&str, &str, f64, f64, f64)] = &[
        ("charlotte-nc", "Charlotte, NC (Atlas 14 Vol 8)", 81.0, 11.0, 0.82),
        ("raleigh-nc", "Raleigh, NC (Atlas 14 Vol 8)", 78.0, 10.5, 0.81),
        ("atlanta-ga", "Atlanta, GA (Atlas 14 Vol 9)", 96.0, 12.0, 0.80),
        ("nashville-tn", "Nashville, TN (Atlas 14 Vol 5)", 72.0, 10.0, 0.79),
        ("denver-co", "Denver, CO (Atlas 14 Vol 11)", 55.0, 8.0, 0.85),
    ];
    for (key, label, a, b, c) in PRESETS {
        lines.push(format!("  {key:<18} {label} — a={a:.1}, b={b:.1}, c={c:.3}"));
    }
    lines.push("  Use HC_PARAMS to set a/b/c, or HC_RATIONAL after HC_PARAMS.".into());
    lines
}

pub fn license_lines() -> Vec<String> {
    vec![
        "=== HydroComplete License ===".into(),
        "  Mode: Free (Open CAD Studio edition)".into(),
        "  Pro features (HC_REPORT_PDF) — use HC_ACTIVATE when licensing is wired.".into(),
    ]
}

pub fn stub_message(cmd: &str) -> String {
    format!(
        "{cmd}: planned — engine module port from HydroComplete.Engine in progress. \
         Core network commands (HC_PIPES, HC_CAPACITY, HC_ANALYZE, HC_HGL, HC_VALIDATE) are available now."
    )
}

pub fn emit_lines(host: &mut dyn HostApi, lines: Result<Vec<String>, String>) {
    match lines {
        Ok(lines) => {
            for line in lines {
                if line.starts_with("No ") {
                    host.push_info(&line);
                } else {
                    host.push_output(&line);
                }
            }
        }
        Err(e) => host.push_error(&e),
    }
}

fn shape_label(s: PipeShape) -> &'static str {
    match s {
        PipeShape::Circular => "circular",
        PipeShape::Box => "box",
        PipeShape::Arch => "arch",
    }
}

fn dimension_label(seg: &PipeSegment) -> String {
    match seg.shape {
        PipeShape::Circular => format!("{:.2} ft", seg.diameter_ft),
        PipeShape::Box => format!("{:.1}x{:.1}", seg.width_ft, seg.height_ft),
        PipeShape::Arch => format!(
            "{:.1}x{:.1}",
            seg.effective_span_ft(),
            seg.effective_rise_ft()
        ),
    }
}

fn trim(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s[..max].to_string()
    }
}