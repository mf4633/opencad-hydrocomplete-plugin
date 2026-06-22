//! Extended `HC_*` commands mirroring HydroComplete.Civil3D (drawing → engine → output).

use std::collections::HashMap;
use std::path::PathBuf;

use acadrust::EntityType;
use hydrocomplete::about::ABOUT_LINES;
use hydrocomplete::culvert_hydraulics::{self, CulvertParameters};
use hydrocomplete::gradually_varied_flow::{self, ChannelParameters, GvfBoundaryType, Station};
use hydrocomplete::hydrograph_convolution::{self, UnitHydrographMethod};
use hydrocomplete::hydrograph_router::{self, HydrographRouterOptions, RouterPipe};
use hydrocomplete::landxml_writer::{self, LandXmlPipeRecord, LandXmlPipeShape, LandXmlStructureRecord};
use hydrocomplete::compliance::{ComplianceReport, ComplianceStatus};
use hydrocomplete::manning;
use hydrocomplete::models::{Catchment, PipeSegment, PipeShape};
use hydrocomplete::network_analysis::{self, NetworkAnalysisResult};
use hydrocomplete::sediment;
use hydrocomplete::network_diagram::{self, DiagramNode, DiagramPipe, PipeDiagramStats};
use hydrocomplete::output_paths::{self, sanitize_file_name};
use hydrocomplete::pipe_cost_catalog;
use hydrocomplete::profile_dxf_writer::{self, ProfileDxfData, ProfileDxfOptions, ProfilePoint, ProfileStation};
use hydrocomplete::pump_station;
use hydrocomplete::bmp::{self, land_use, pollutant};
use hydrocomplete::bmp_optimizer::{self, SiteData};
use hydrocomplete::detention::{self, default_nc_detention_outlets, build_prismatic_storage_indication_curve, inflow_from_unit_hydrograph, route};
use hydrocomplete::prepost::{self, default_detention_pond};
use hydrocomplete::scs_runoff;
use hydrocomplete::scs_unit_hydrograph;
use hydrocomplete::state_compliance::{self, peak_storm_suite};
use hydrocomplete::time_of_concentration::{self, TcSegment};
use hydrocomplete::water_quality;
use ocs_plugin_api::host::HostApi;
use stormsewer::design::{check_inlet_geom, InletGeometry, InletKind};
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

pub fn atlas14_lines(args: &str) -> Result<Vec<String>, String> {
    let tokens: Vec<&str> = args.split_whitespace().collect();
    if tokens.first().map(|t| t.eq_ignore_ascii_case("LIVE")).unwrap_or(false) {
        return atlas14_live_lines(&tokens[1..]);
    }
    if tokens.first().map(|t| t.eq_ignore_ascii_case("APPLY")).unwrap_or(false) {
        return atlas14_apply_lines(&tokens[1..]);
    }
    Ok(atlas14_list_lines())
}

fn atlas14_list_lines() -> Vec<String> {
    let mut lines = vec![
        "--- HydroComplete: NOAA Atlas 14 IDF ---".into(),
        "  i = a/(t+b)^c   (default 10-yr curve; t in minutes)".into(),
        "  i@10m tabular intensities shown for 2 / 10 / 25 / 100-yr return periods.".into(),
        "  Live PFDS: HC_ATLAS14 LIVE <lat> <lon> [rp]  (cached 30 days under %APPDATA%/HydroComplete/idf-cache)".into(),
        "  Apply preset: HC_PARAMS PRESET <key> [rp]  or  HC_ATLAS14 APPLY <key> [rp]".into(),
        String::new(),
        "  embedded presets (10-yr a/b/c + multi-RP i@10m):".into(),
    ];
    for p in hydrocomplete::atlas14_presets::list() {
        lines.push(format!(
            "  {:<16} {:<20} a={:5.1} b={:4.1} c={:.2}  {}",
            p.key,
            p.display_name,
            p.a(),
            p.b(),
            p.c(),
            p.multi_return_period_10min_label(),
        ));
    }
    lines.push("  Use preset key with HC_PARAMS PRESET, then HC_RATIONAL / HC_ANALYZE.".into());
    lines
}

fn atlas14_live_lines(tokens: &[&str]) -> Result<Vec<String>, String> {
    let lat: f64 = tokens
        .first()
        .ok_or("HC_ATLAS14 LIVE <lat> <lon> [return_period]")?
        .parse()
        .map_err(|_| "Invalid latitude")?;
    let lon: f64 = tokens
        .get(1)
        .ok_or("HC_ATLAS14 LIVE <lat> <lon> [return_period]")?
        .parse()
        .map_err(|_| "Invalid longitude")?;
    let rp: i32 = tokens
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let fetcher = hydrocomplete::atlas14_fetcher::Atlas14Fetcher::new(Some(
        hydrocomplete::atlas14_fetcher::default_cache_directory(),
    ));
    let res = fetcher.resolve_with_fallback(lat, lon, rp);
    Ok(vec![
        format!(
            "--- HydroComplete: NOAA Atlas 14 @ {lat:.4}, {lon:.4} ({rp}-yr) ---"
        ),
        format!("  Source: {}", res.source.as_str()),
        format!("  Label: {}", res.display_label),
        format!("  IDF: i = {:.2}/(t+{:.2})^{:.3}", res.a, res.b, res.c),
        format!(
            "  i@10min = {:.2} in/hr",
            res.to_curve().intensity(10.0)
        ),
        "  Apply to tab: HC_PARAMS LIVE {lat} {lon} {rp}".into(),
    ])
}

fn atlas14_apply_lines(tokens: &[&str]) -> Result<Vec<String>, String> {
    let key = tokens
        .first()
        .ok_or("HC_ATLAS14 APPLY <preset-key> [return_period]")?;
    let rp: i32 = tokens.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let preset = hydrocomplete::atlas14_presets::find(key)
        .ok_or_else(|| format!("Unknown preset '{key}'. Run HC_ATLAS14 for keys."))?;
    let res = hydrocomplete::atlas14_fetcher::Atlas14Resolution::from_preset(preset, rp)?;
    Ok(vec![
        format!("--- HydroComplete: Atlas 14 preset '{key}' ({rp}-yr) ---"),
        format!("  {} ({})", preset.display_name, preset.state),
        format!("  IDF: i = {:.2}/(t+{:.2})^{:.3}", res.a, res.b, res.c),
        format!(
            "  i@10min = {:.2} in/hr  |  run HC_PARAMS PRESET {key} {rp} to apply to this tab",
            res.to_curve().intensity(10.0)
        ),
    ])
}

pub fn license_lines() -> Vec<String> {
    vec![
        "=== HydroComplete License ===".into(),
        format!("  Status: {}", hydrocomplete::license::status_label()),
        format!(
            "  Validation mode: {}",
            hydrocomplete::license::validation_mode_label()
        ),
        format!(
            "  Last validated: {}",
            hydrocomplete::license::last_validated_label()
        ),
        format!(
            "  Network: {}",
            hydrocomplete::license::online_offline_label()
        ),
        format!(
            "  License file: {}",
            hydrocomplete::license::license_file_path().display()
        ),
        "  Activate: HC_ACTIVATE <email> <token>  |  https://hydrocomplete.com/civil3d".into(),
        "  Pro unlocks PDF export (HC_REPORT_PDF). HTML reports (HC_REPORT) stay free.".into(),
    ]
}

pub fn activate_lines(args: &str) -> Vec<String> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return vec![
            "=== HydroComplete Pro Activation ===".into(),
            "  Usage: HC_ACTIVATE <email> <hc_live_token>".into(),
            "  Or paste both on one line: email@domain.com hc_live_...".into(),
            "  Get a beta token at https://hydrocomplete.com/civil3d".into(),
        ];
    }
    let (email, token) = if let Some((e, t)) = hydrocomplete::license::try_parse_combined_input(trimmed) {
        (e, t)
    } else {
        let mut parts = trimmed.split_whitespace();
        let email = parts.next().unwrap_or("").to_string();
        let token = parts.next().unwrap_or("").to_string();
        if email.is_empty() || token.is_empty() {
            return vec![
                "Activation failed: provide email and token.".into(),
                "  Usage: HC_ACTIVATE user@example.com hc_live_...".into(),
            ];
        }
        (email, token)
    };
    let activator = hydrocomplete::license::LicenseActivator::new();
    let path = hydrocomplete::license::license_file_path();
    let result = activator.activate(&email, &token, &path);
    let mut lines = vec!["=== HydroComplete Pro Activation ===".into()];
    if result.success {
        lines.push(format!("  {}", result.message));
        lines.push(format!("  Mode: {:?}", result.mode));
        if !result.expires.is_empty() {
            if let Some(d) = result.expires.get(..10) {
                lines.push(format!("  Expires: {d}"));
            }
        }
        lines.push(format!(
            "  Status: {}",
            hydrocomplete::license::status_label()
        ));
        lines.push("  Pro unlocks HC_REPORT_PDF. Run HC_LICENSE for details.".into());
    } else {
        lines.push(format!("  Activation failed: {}", result.message));
    }
    lines
}

pub fn gvf_lines(args: &str) -> Result<Vec<String>, String> {
    let (q, boundary, known_depth) = parse_gvf_args(args);
    let channel = ChannelParameters {
        bottom_width_ft: 10.0,
        side_slope_z: 2.0,
        manning_n: 0.03,
        bed_slope_ft_per_ft: 0.001,
    };
    let stations = vec![
        Station { distance_ft: 0.0, invert_elev_ft: 100.0 },
        Station { distance_ft: 100.0, invert_elev_ft: 99.9 },
        Station { distance_ft: 200.0, invert_elev_ft: 99.8 },
    ];
    let result = gradually_varied_flow::compute_water_surface_profile(
        q, &channel, boundary, known_depth, &stations,
        gradually_varied_flow::DEFAULT_EDDY_LOSS_COEFFICIENT,
    )?;
    let mut lines = vec![
        format!("--- HydroComplete: GVF profile ({}) ---", result.profile_type),
        format!(
            "  y_n = {:.4} ft   y_c = {:.4} ft   march = {}",
            result.normal_depth_ft, result.critical_depth_ft,
            if result.is_subcritical { "upstream (subcritical)" } else { "downstream (supercritical)" }
        ),
        "  Station      Invert     Depth      WSE        V        Fr       Regime".into(),
    ];
    for pt in &result.profile {
        lines.push(format!(
            "  {:8.2}   {:8.2}   {:8.4}   {:8.2}   {:6.2}   {:6.3}   {}",
            pt.station_ft, pt.invert_elev_ft, pt.depth_ft, pt.water_surface_elev_ft,
            pt.velocity_fps, pt.froude_number, pt.flow_regime
        ));
    }
    Ok(lines)
}

pub fn culvert_lines(args: &str) -> Vec<String> {
    let (q, dia_in, tailwater) = parse_culvert_args(args);
    let culvert = CulvertParameters { diameter_in: dia_in, ..Default::default() };
    let result = culvert_hydraulics::headwater(q, &culvert, tailwater);
    let control = match result.control {
        culvert_hydraulics::ControlType::Inlet => "Inlet",
        culvert_hydraulics::ControlType::Outlet => "Outlet",
        culvert_hydraulics::ControlType::None => "None",
    };
    vec![
        "--- HydroComplete: culvert headwater (FHWA HDS-5) ---".into(),
        format!("  D = {dia_in:.0} in   L = {:.0} ft   S = {:.5}   n = {:.3}",
            culvert.length_ft, culvert.slope_ft_per_ft, culvert.manning_n),
        format!("  Q = {q:.2} cfs   V = {:.2} ft/s   TW = {tailwater:.2} ft", result.velocity_fps),
        format!("  HW_inlet = {:.2} ft   HW_outlet = {:.2} ft   HW = {:.2} ft ({control} control)",
            result.headwater_inlet_ft, result.headwater_outlet_ft, result.headwater_ft),
    ]
}

pub fn tc_lines(_args: &str) -> Vec<String> {
    let segments = time_of_concentration::default_tr55_worksheet();
    let mut lines = vec!["--- HydroComplete: TR-55 time of concentration ---".into()];
    for seg in &segments {
        let r = segment_tc(seg);
        lines.push(format!("  {} ({})  L={:.0} ft  Tc={:.2} min", seg.name, seg.segment_type, seg.length_ft, r.tc_minutes));
    }
    let composite = time_of_concentration::from_tr55_segments(&segments);
    lines.push(format!("  Composite Tc = {:.2} min ({:.2} hr)", composite.tc_minutes, composite.tc_minutes / 60.0));
    lines
}

pub fn inlets_lines<'a>(entities: impl Iterator<Item = &'a EntityType>, params: &StormAnalysisParams, args: &str) -> Result<Vec<String>, String> {
    let (design_q, kind) = parse_inlet_args(args);
    let geom = InletGeometry {
        kind,
        grate_length_ft: params.inlet_grate_length_ft,
        curb_opening_length_ft: params.inlet_curb_length_ft,
        flow_depth_ft: params.inlet_flow_depth_ft,
        gutter_slope: params.inlet_gutter_slope,
    };
    let chk = check_inlet_geom(design_q, &geom);
    let mut lines = vec![
        format!("--- HydroComplete: inlet capacity ({}) ---", kind.label()),
        format!("  Q_design = {:.2} cfs   Q_cap = {:.2} cfs", chk.design_q_cfs, chk.capacity_cfs),
        format!("  Status: {}", if chk.ok { "OK" } else { "BYPASS" }),
    ];
    if design_q <= 0.0 {
        let drawn = data::drawn_network_from_entities(entities)?;
        if let Ok(analysis) = analysis::run_analysis_on_network(&drawn.network, params) {
            for nd in &drawn.network.nodes {
                if nd.kind != stormsewer::network::NodeKind::Inlet { continue; }
                let q = analysis.pipes.iter().filter(|p| p.from == nd.id).map(|p| p.design_q).fold(0.0_f64, f64::max);
                if q <= 0.0 { continue; }
                let c = check_inlet_geom(q, &geom);
                lines.push(format!("    {}  Q={:.1}  cap={:.1}  {}", nd.id, c.design_q_cfs, c.capacity_cfs, if c.ok { "ok" } else { "BYPASS" }));
            }
        }
    }
    Ok(lines)
}

pub fn hydrograph_lines<'a>(entities: impl Iterator<Item = &'a EntityType>, args: &str) -> Result<Vec<String>, String> {
    let catchments = data::catchments_from_entities(entities);
    if catchments.is_empty() { return Ok(vec!["No catchments found.".into()]); }
    let (storm_depth, method) = parse_hydrograph_args(args);
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let system_tc = catchments.iter().map(|c| c.tc_minutes).fold(0.0_f64, f64::max).max(10.0);
    let cms: Vec<Catchment> = catchments.iter().map(|c| Catchment {
        name: c.name.clone(), area_acres: c.area_acres, runoff_c: c.runoff_c,
        curve_number: c.curve_number, tc_minutes: c.tc_minutes,
        outfall_structure_id: None, outfall_structure_name: None,
    }).collect();
    let cn = hydrograph_convolution::cn_from_catchments(&cms);
    let hydro = hydrograph_convolution::generate_tr20_hydrograph(total_area, cn, system_tc, storm_depth, 0.25, method);
    Ok(vec![
        format!("--- HydroComplete: design hydrograph (A={total_area:.3} ac, CN={cn:.0}, P={storm_depth:.2} in) ---"),
        format!("  Excess rainfall = {:.3} in   Peak Q = {:.1} cfs at t = {:.2} hr",
            hydro.total_excess_rainfall_in, hydro.peak_flow_cfs, hydro.time_to_peak_hours),
        format!("  Runoff volume = {:.3} ac-ft", hydro.volume_acre_ft),
    ])
}

pub fn route_hydro_lines<'a>(entities: impl IntoIterator<Item = &'a EntityType>, params: &StormAnalysisParams, args: &str) -> Result<Vec<String>, String> {
    let ents: Vec<_> = entities.into_iter().collect();
    let catchments = data::catchments_from_entities(ents.iter().copied());
    if catchments.is_empty() { return Ok(vec!["No catchments found.".into()]); }
    let drawn = data::drawn_network_from_entities(ents.iter().copied())?;
    if drawn.network.pipes.is_empty() { return Ok(vec!["No pipes found.".into()]); }
    let storm_depth = parse_storm_depth(args);
    let cms: Vec<Catchment> = catchments.iter().map(|c| Catchment {
        name: c.name.clone(), area_acres: c.area_acres, runoff_c: c.runoff_c,
        curve_number: c.curve_number, tc_minutes: c.tc_minutes,
        outfall_structure_id: None, outfall_structure_name: None,
    }).collect();
    let pipes = router_pipes_from_drawn(&drawn, params)?;
    let result = hydrograph_router::route(&cms, &pipes, &HydrographRouterOptions { storm_depth_in: storm_depth, ..Default::default() });
    let mut lines = vec![format!("--- HydroComplete: routed hydrographs ({} pipes) ---", result.pipe_hydrographs.len())];
    for (_, p) in result.pipe_hydrographs.iter().take(10) {
        lines.push(format!("  Pipe {}  Q_peak={:.1} cfs  t_peak={:.0} min", p.pipe_name, p.peak_flow_cfs, p.time_to_peak_minutes));
    }
    Ok(lines)
}

pub fn pump_lines(args: &str) -> Vec<String> {
    let (q, static_lift, fm_len, fm_dia) = parse_pump_args(args);
    let duty = pump_station::check_duty(q, 100.0, 100.0 + static_lift, fm_len, fm_dia, 0.013, &pump_station::default_curve());
    vec![
        "--- HydroComplete: pump station duty check ---".into(),
        format!("  Design Q = {q:.1} cfs   System = {:.1} ft   Pump = {:.1} ft   {}",
            duty.system_head_ft, duty.pump_head_ft, if duty.ok { "OK" } else { "UNDERSIZED" }),
    ]
}

pub fn cost_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let drawn = data::drawn_network_from_entities(entities)?;
    if drawn.network.pipes.is_empty() { return Ok(vec!["No pipes found.".into()]); }
    let items: Vec<_> = drawn.network.pipes.iter().map(|p| ("default".into(), p.id.clone(), p.length, p.diameter, "RCP".into())).collect();
    let rollups = pipe_cost_catalog::rollup_by_network(&items);
    let mut lines = vec!["--- HydroComplete: pipe cost estimate ---".into()];
    for net in &rollups {
        lines.push(format!("  {}  {:.0} ft  ${:.0}", net.network_name, net.total_length_ft, net.total_cost));
    }
    Ok(lines)
}

pub fn profile_dxf_export<'a>(entities: impl Iterator<Item = &'a EntityType>, params: &StormAnalysisParams, drawing_name: &str, args: &str) -> Result<Vec<String>, String> {
    let drawn = data::drawn_network_from_entities(entities)?;
    if drawn.network.pipes.is_empty() { return Ok(vec!["No pipes found.".into()]); }
    let analysis = analysis::run_analysis_on_network(&drawn.network, params).ok();
    let dxf_data = build_profile_dxf_data(&drawn, analysis.as_ref());
    let path = output_paths::output_folder().join(format!("{}_default_profile.dxf", sanitize_file_name(drawing_name)));
    profile_dxf_writer::write_file(&path, &dxf_data, &ProfileDxfOptions { include_hgl: !args.contains("nohgl"), ..Default::default() }).map_err(|e| e.to_string())?;
    Ok(vec!["--- HydroComplete: profile DXF export ---".into(), format!("  File: {}", path.display())])
}

pub fn landxml_export<'a>(entities: impl Iterator<Item = &'a EntityType>, drawing_name: &str, args: &str) -> Result<Vec<String>, String> {
    let drawn = data::drawn_network_from_entities(entities)?;
    if drawn.network.pipes.is_empty() { return Ok(vec!["No pipe networks found.".into()]); }
    let (pipes, structures) = landxml_records_from_drawn(&drawn);
    let path = parse_output_path(args).unwrap_or_else(|| output_paths::output_folder().join(format!("{}_network.xml", sanitize_file_name(drawing_name))));
    landxml_writer::write_file(&path, &pipes, Some(&structures), Some(drawing_name)).map_err(|e| e.to_string())?;
    Ok(vec!["--- HydroComplete: LandXML export ---".into(), format!("  Pipes: {}  Structures: {}", pipes.len(), structures.len()), format!("  File: {}", path.display())])
}

pub fn network_diagram_export<'a>(entities: impl Iterator<Item = &'a EntityType>, params: &StormAnalysisParams, drawing_name: &str) -> Result<Vec<String>, String> {
    let drawn = data::drawn_network_from_entities(entities)?;
    if drawn.network.pipes.is_empty() { return Ok(vec!["No pipe networks found.".into()]); }
    let (pipes, nodes, stats) = diagram_from_drawn(&drawn, Some(params))?;
    let path = network_diagram::write_network_diagram(drawing_name, "default", &pipes, &nodes, stats.as_ref()).map_err(|e| e.to_string())?;
    Ok(vec!["--- HydroComplete: network diagram ---".into(), format!("  HTML: {}", path.display())])
}

fn build_profile_dxf_data(drawn: &data::DrawnNetwork, analysis: Option<&stormsewer::network::Analysis>) -> ProfileDxfData {
    let mut data = ProfileDxfData { network_name: "default".into(), ..Default::default() };
    let mut chainage = 0.0;
    for pipe in &drawn.network.pipes {
        let up = drawn.network.nodes.iter().find(|n| n.id == pipe.from);
        let down = drawn.network.nodes.iter().find(|n| n.id == pipe.to);
        let up_inv = up.map(|n| n.invert).unwrap_or(0.0);
        let down_inv = down.map(|n| n.invert).unwrap_or(0.0);
        data.invert_points.push(ProfilePoint { chainage_ft: chainage, elevation_ft: up_inv });
        data.crown_points.push(ProfilePoint { chainage_ft: chainage, elevation_ft: up_inv + pipe.diameter });
        if let Some(a) = analysis.and_then(|a| a.pipes.iter().find(|p| p.id == pipe.id)) {
            if let Some(hgl) = a.hgl_up {
                data.hgl_points.push(ProfilePoint { chainage_ft: chainage, elevation_ft: hgl });
            }
        }
        chainage += pipe.length;
        data.invert_points.push(ProfilePoint { chainage_ft: chainage, elevation_ft: down_inv });
        data.crown_points.push(ProfilePoint { chainage_ft: chainage, elevation_ft: down_inv + pipe.diameter });
        if let Some(a) = analysis.and_then(|a| a.pipes.iter().find(|p| p.id == pipe.id)) {
            if let Some(hgl) = a.hgl_dn {
                data.hgl_points.push(ProfilePoint { chainage_ft: chainage, elevation_ft: hgl });
            }
        }
    }
    data
}

fn landxml_records_from_drawn(drawn: &data::DrawnNetwork) -> (Vec<LandXmlPipeRecord>, Vec<LandXmlStructureRecord>) {
    let pipes: Vec<_> = drawn.network.pipes.iter().map(|pipe| {
        let up = drawn.network.nodes.iter().find(|n| n.id == pipe.from);
        let down = drawn.network.nodes.iter().find(|n| n.id == pipe.to);
        let slope = if pipe.length > 0.0 { (up.map(|n| n.invert).unwrap_or(0.0) - down.map(|n| n.invert).unwrap_or(0.0)) / pipe.length } else { 0.001 };
        LandXmlPipeRecord {
            name: pipe.id.clone(), network_name: "default".into(), length_ft: pipe.length,
            diameter_ft: pipe.diameter, slope: slope.max(0.0001),
            start_invert_ft: up.map(|n| n.invert).unwrap_or(0.0), end_invert_ft: down.map(|n| n.invert).unwrap_or(0.0),
            manning_n: pipe.n, design_flow_cfs: None, start_structure_name: pipe.from.clone(),
            end_structure_name: pipe.to.clone(), shape: LandXmlPipeShape::Circular, width_ft: 0.0, height_ft: 0.0,
        }
    }).collect();
    let structures: Vec<_> = drawn.network.nodes.iter().map(|n| LandXmlStructureRecord {
        name: n.id.clone(), network_name: "default".into(), rim_ft: Some(n.rim), invert_ft: Some(n.invert),
        northing_ft: Some(n.y), easting_ft: Some(n.x), diameter_ft: Some(4.0),
    }).collect();
    (pipes, structures)
}

fn diagram_from_drawn(drawn: &data::DrawnNetwork, params: Option<&StormAnalysisParams>) -> Result<(Vec<DiagramPipe>, Vec<DiagramNode>, Option<HashMap<String, PipeDiagramStats>>), String> {
    let nodes: Vec<_> = drawn.network.nodes.iter().zip(drawn.node_handles.iter()).map(|(node, h)| DiagramNode {
        id: format!("{h:?}"), label: node.id.clone(), x: node.x, y: node.y,
    }).collect();
    let id_to_handle: HashMap<_, _> = drawn.network.nodes.iter().zip(drawn.node_handles.iter())
        .map(|(n, h)| (n.id.clone(), format!("{h:?}"))).collect();
    let pipes: Vec<_> = drawn.network.pipes.iter().enumerate().map(|(idx, pipe)| {
        let up = drawn.network.nodes.iter().find(|n| n.id == pipe.from);
        let down = drawn.network.nodes.iter().find(|n| n.id == pipe.to);
        DiagramPipe {
            key: drawn.pipe_handles.get(idx).map(|h| format!("{h:?}")).unwrap_or_else(|| format!("pipe{idx}")),
            name: pipe.id.clone(),
            upstream_id: id_to_handle.get(&pipe.from).cloned().unwrap_or(pipe.from.clone()),
            downstream_id: id_to_handle.get(&pipe.to).cloned().unwrap_or(pipe.to.clone()),
            x1: up.map(|n| n.x).unwrap_or(0.0), y1: up.map(|n| n.y).unwrap_or(0.0),
            x2: down.map(|n| n.x).unwrap_or(0.0), y2: down.map(|n| n.y).unwrap_or(0.0),
            diameter_in: pipe.diameter * 12.0,
        }
    }).collect();
    let mut stats = None;
    if let Some(params) = params {
        if let Ok(analysis) = analysis::run_analysis_on_network(&drawn.network, params) {
            let mut m = HashMap::new();
            for (idx, pipe) in drawn.network.pipes.iter().enumerate() {
                let seg = PipeSegment::circular(&pipe.id, pipe.diameter, 0.01, pipe.n);
                let cap = manning::capacity(&seg);
                if let Some(pa) = analysis.pipes.iter().find(|p| p.id == pipe.id) {
                    let nd = manning::normal_depth(&seg, pa.design_q);
                    m.insert(drawn.pipe_handles.get(idx).map(|h| format!("{h:?}")).unwrap_or_else(|| format!("pipe{idx}")),
                        PipeDiagramStats { design_flow_cfs: pa.design_q, flow_ratio: if cap.full_flow_cfs > 0.0 { pa.design_q / cap.full_flow_cfs } else { 0.0 }, surcharged: nd.surcharged });
                }
            }
            if !m.is_empty() { stats = Some(m); }
        }
    }
    Ok((pipes, nodes, stats))
}

fn router_pipes_from_drawn(drawn: &data::DrawnNetwork, _params: &StormAnalysisParams) -> Result<Vec<RouterPipe>, String> {
    Ok(drawn.network.pipes.iter().enumerate().map(|(idx, pipe)| {
        let up = drawn.network.nodes.iter().find(|n| n.id == pipe.from);
        let down = drawn.network.nodes.iter().find(|n| n.id == pipe.to);
        let slope = if pipe.length > 0.0 { (up.map(|n| n.invert).unwrap_or(0.0) - down.map(|n| n.invert).unwrap_or(0.0)) / pipe.length } else { 0.001 };
        RouterPipe {
            pipe_key: format!("{idx}"), network_name: "default".into(), pipe_name: pipe.id.clone(),
            upstream_node_id: pipe.from.clone(), downstream_node_id: pipe.to.clone(),
            segment: PipeSegment { name: pipe.id.clone(), shape: PipeShape::Circular, diameter_ft: pipe.diameter,
                width_ft: 0.0, height_ft: 0.0, span_ft: 0.0, rise_ft: 0.0, slope: slope.max(0.0001),
                manning_n: pipe.n, design_flow_cfs: 0.0, length_ft: pipe.length,
                start_invert_ft: up.map(|n| n.invert).unwrap_or(0.0), end_invert_ft: down.map(|n| n.invert).unwrap_or(0.0) },
            length_ft: pipe.length,
        }
    }).collect())
}

fn segment_tc(seg: &TcSegment) -> time_of_concentration::TcResult {
    match seg.segment_type.trim().to_ascii_lowercase().as_str() {
        "sheet" => time_of_concentration::sheet_flow(seg.manning_n, seg.length_ft, seg.rainfall_2year_in, seg.slope),
        "shallow" => time_of_concentration::shallow_concentrated(seg.length_ft, seg.slope, seg.surface_type),
        _ => time_of_concentration::from_tr55_segments(std::slice::from_ref(seg)),
    }
}

fn parse_gvf_args(args: &str) -> (f64, GvfBoundaryType, f64) {
    let mut q = 100.0; let mut boundary = GvfBoundaryType::Normal; let mut known = 2.0;
    for tok in args.split_whitespace() {
        if let Ok(v) = tok.parse::<f64>() { q = v; }
        else if tok.eq_ignore_ascii_case("critical") { boundary = GvfBoundaryType::Critical; }
        else if tok.eq_ignore_ascii_case("known") { boundary = GvfBoundaryType::Known; }
    }
    (q, boundary, known)
}

fn parse_culvert_args(args: &str) -> (f64, f64, f64) {
    let nums: Vec<f64> = args.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    (nums.first().copied().unwrap_or(10.0), nums.get(1).copied().unwrap_or(24.0), nums.get(2).copied().unwrap_or(0.0))
}

fn parse_inlet_args(args: &str) -> (f64, InletKind) {
    let mut q = 0.0; let mut kind = InletKind::GrateOnGrade;
    for tok in args.split_whitespace() {
        if let Ok(v) = tok.parse::<f64>() { q = v; }
        else if let Some(k) = InletKind::from_str_loose(tok) { kind = k; }
    }
    (q, kind)
}

fn parse_hydrograph_args(args: &str) -> (f64, UnitHydrographMethod) {
    let mut depth = 5.0; let mut method = UnitHydrographMethod::Scs;
    for tok in args.split_whitespace() {
        if let Ok(v) = tok.parse::<f64>() { depth = v; }
        else if tok.eq_ignore_ascii_case("snyder") { method = UnitHydrographMethod::Snyder; }
        else if tok.eq_ignore_ascii_case("clark") { method = UnitHydrographMethod::Clark; }
    }
    (depth, method)
}

fn parse_storm_depth(args: &str) -> f64 { args.split_whitespace().find_map(|t| t.parse().ok()).unwrap_or(5.0) }

fn parse_pump_args(args: &str) -> (f64, f64, f64, f64) {
    let nums: Vec<f64> = args.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    (nums.first().copied().unwrap_or(30.0), nums.get(1).copied().unwrap_or(30.0), nums.get(2).copied().unwrap_or(500.0), nums.get(3).copied().unwrap_or(1.0))
}

fn parse_output_path(args: &str) -> Option<PathBuf> {
    let t = args.trim();
    if t.is_empty() { None } else { Some(PathBuf::from(t)) }
}

pub fn analyze_summary_lines(result: &NetworkAnalysisResult, idf_summary: &str) -> Vec<String> {
    let mut lines = vec![
        format!("--- HydroComplete: full network analysis ({}) ---", result.state_code),
        format!("  Overall: {}", if result.overall_pass { "PASS" } else { "FAIL" }),
    ];
    if !idf_summary.is_empty() {
        lines.push(format!("  IDF: {idf_summary}"));
    }
    lines.push(String::new());
    lines.push("  Catchment          Area(ac)  C      Tc(min)  Q(cfs)   SCS Q(in)".into());
    for hydro in &result.hydrology {
        lines.push(format!(
            "  {:<18} {:7.3}  {:4.2}  {:7.1}  {:6.2}  {:6.3}",
            trim(&hydro.catchment.name, 18),
            hydro.catchment.area_acres,
            hydro.catchment.runoff_c,
            hydro.catchment.tc_minutes,
            hydro.rational.peak_flow_cfs,
            hydro.scs.runoff_depth_inches,
        ));
    }
    if let Some(routing) = &result.routing {
        lines.push(String::new());
        lines.push(format!(
            "  Routed Q: {:.2} cfs total ({})",
            routing.total_peak_cfs,
            network_analysis::describe_assignment(routing.assignment_method),
        ));
    }
    if !result.capacity.is_empty() {
        lines.push(String::new());
        lines.push("  Network / Pipe            Q(cfs)  Q/Qfull  d/D   SURCH".into());
        let mut rows = result.capacity.clone();
        rows.sort_by(|a, b| {
            a.pipe.network_name.to_ascii_lowercase().cmp(&b.pipe.network_name.to_ascii_lowercase())
                .then_with(|| a.pipe.pipe_name.to_ascii_lowercase().cmp(&b.pipe.pipe_name.to_ascii_lowercase()))
        });
        for row in rows {
            let label = if row.pipe.network_name.is_empty() {
                row.pipe.pipe_name.clone()
            } else {
                format!("{}/{}", row.pipe.network_name, row.pipe.pipe_name)
            };
            let d_over_d = if row.surcharged() { "SURCH".into() } else { format!("{:.2}", row.normal_depth.relative_depth) };
            lines.push(format!(
                "  {:<24} {:6.1}  {:7.2}  {:>5}  {:>5}",
                trim(&label, 24), row.design_flow_cfs, row.flow_ratio(), d_over_d,
                if row.surcharged() { "*" } else { "" },
            ));
        }
    }
    if !result.sediment.is_empty() {
        lines.push(String::new());
        lines.push(format!("  RUSLE avg soil loss: {:.2} tons/ac/yr", sediment::weighted_average_soil_loss(&result.sediment)));
    }
    if let Some(wqv) = &result.wqv {
        lines.push(format!("  WQV required: {:.0} cf ({:.2} ac-ft)", wqv.wqv_cf, wqv.wqv_acre_ft));
    }
    if let Some(train) = &result.treatment_train {
        if let Some(tss) = train.overall_removal_efficiency.get("TSS") {
            lines.push(format!("  Placeholder BMP train TSS removal: {:.1}%", tss * 100.0));
        }
    }
    append_compliance_table(&mut lines, result.compliance.as_ref());
    append_design_review(&mut lines, &result.design_review, 8);
    for w in &result.warnings { lines.push(format!("  Warning: {w}")); }
    for e in &result.errors { lines.push(format!("  Error: {e}")); }
    lines
}

pub fn review_summary_lines<'a>(
    entities: impl IntoIterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
    compliance: &ComplianceReport,
    state_name: &str,
    development_type: &str,
) -> Result<Vec<String>, String> {
    let ents: Vec<_> = entities.into_iter().collect();
    let mut lines = vec![format!("--- HydroComplete: design review ({state_name}, {development_type}) ---")];
    let findings = crate::analysis::design_review_doc(ents.iter().copied(), params).unwrap_or_default();
    if !findings.is_empty() {
        let errors = findings.iter().filter(|f| f.severity == stormsewer::design::Severity::Error).count();
        let warnings = findings.iter().filter(|f| f.severity == stormsewer::design::Severity::Warning).count();
        lines.push(format!("  Design criteria: {errors} error(s), {warnings} warning(s)"));
        let mut sorted = findings.clone();
        sorted.sort_by(|a, b| severity_rank(a.severity).cmp(&severity_rank(b.severity)).then_with(|| a.id.cmp(&b.id)));
        for f in &sorted {
            let tag = if f.severity == stormsewer::design::Severity::Error { "ERROR" } else { "WARN " };
            lines.push(format!("    [{tag}] {}", f.message));
        }
    } else if crate::data::drawn_network_from_entities(ents.iter().copied()).is_ok() {
        lines.push("  Design criteria: all checked rules passed.".into());
    }
    lines.push(format!("  Regulatory compliance: {}", if compliance.overall_pass { "COMPLIANT" } else { "NON-COMPLIANT" }));
    append_compliance_table(&mut lines, Some(compliance));
    if !compliance.recommendations.is_empty() {
        lines.push("  Recommendations:".into());
        for rec in &compliance.recommendations { lines.push(format!("    - {rec}")); }
    }
    Ok(lines)
}

fn append_compliance_table(lines: &mut Vec<String>, compliance: Option<&ComplianceReport>) {
    let Some(comp) = compliance else { return };
    lines.push(String::new());
    lines.push(format!("  Compliance: {}", if comp.overall_pass { "COMPLIANT" } else { "NON-COMPLIANT" }));
    lines.push("  Criterion                         Required          Actual            Status".into());
    for c in &comp.criteria {
        lines.push(format!(
            "  {:<32} {:<16} {:<16} {}",
            trim(&c.name, 32), trim(&c.required, 16), trim(&c.actual, 16), status_label(c.status),
        ));
    }
}

fn append_design_review(lines: &mut Vec<String>, findings: &[stormsewer::design::DesignFinding], limit: usize) {
    let errors = findings.iter().filter(|f| f.severity == stormsewer::design::Severity::Error).count();
    let warnings = findings.iter().filter(|f| f.severity == stormsewer::design::Severity::Warning).count();
    if errors == 0 && warnings == 0 { return; }
    lines.push(String::new());
    lines.push(format!("  Design review: {errors} error(s), {warnings} warning(s)"));
    let mut sorted = findings.to_vec();
    sorted.sort_by(|a, b| severity_rank(a.severity).cmp(&severity_rank(b.severity)).then_with(|| a.id.cmp(&b.id)));
    for f in sorted.iter().take(limit) {
        let tag = if f.severity == stormsewer::design::Severity::Error { "ERROR" } else { "WARN " };
        lines.push(format!("    [{tag}] {}", f.message));
    }
    if findings.len() > limit {
        lines.push(format!("    ... and {} more finding(s).", findings.len() - limit));
    }
}

fn severity_rank(s: stormsewer::design::Severity) -> u8 {
    match s { stormsewer::design::Severity::Error => 0, stormsewer::design::Severity::Warning => 1 }
}

fn status_label(status: ComplianceStatus) -> String {
    match status {
        ComplianceStatus::Pass => "Pass".into(),
        ComplianceStatus::Fail => "Fail".into(),
        ComplianceStatus::Review => "Review".into(),
        ComplianceStatus::Incomplete => "Incomplete".into(),
        ComplianceStatus::Info => "Info".into(),
    }
}

fn engine_catchments<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Vec<Catchment> {
    data::catchments_from_entities(entities)
        .into_iter()
        .map(|c| Catchment {
            name: c.name,
            area_acres: c.area_acres,
            runoff_c: c.runoff_c,
            curve_number: c.curve_number,
            tc_minutes: c.tc_minutes,
            outfall_structure_id: c.outfall_structure_id,
            outfall_structure_name: None,
        })
        .collect()
}

fn default_state() -> hydrocomplete::state_compliance::StateComplianceConfig {
    state_compliance::get(crate::analyze_full::default_state_code())
}

pub fn unit_hydro_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let system_tc = catchments.iter().map(|c| c.tc_minutes).fold(0.0_f64, f64::max).max(10.0);
    let uh = scs_unit_hydrograph::generate(total_area, system_tc, None, None);
    let mut lines = vec![
        format!("--- HydroComplete: SCS unit hydrograph (A={total_area:.3} ac, Tc={system_tc:.1} min) ---"),
        format!(
            "  Tl = {:.3} hr   Tp = {:.3} hr ({:.1} min)   qp = {:.1} cfs (1 in runoff)",
            uh.lag_hours, uh.time_to_peak_hours, uh.time_to_peak_minutes, uh.peak_flow_cfs
        ),
        "  t(min)   t/Tp    q/qp    Q(cfs)".into(),
    ];
    for ord in &uh.ordinates {
        lines.push(format!(
            "  {:6.1}  {:5.2}  {:5.2}  {:8.1}",
            ord.time_minutes, ord.t_ratio, ord.q_ratio, ord.flow_cfs
        ));
    }
    Ok(lines)
}

pub fn sediment_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = data::catchments_from_entities(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let slope_pct = 5.0;
    let length_ft = 300.0;
    let r_factor = state.default_r_factor;
    let rows: Vec<_> = catchments
        .iter()
        .map(|c| sediment::rusle(c.area_acres, slope_pct, length_ft, c.runoff_c, r_factor, 0.32, 1.0, &c.name))
        .collect();
    let weighted = sediment::weighted_average_soil_loss(&rows);
    let mut lines = vec![
        format!(
            "--- HydroComplete: RUSLE soil loss ({}, R={r_factor:.0}, {} catchments) ---",
            state.name,
            catchments.len()
        ),
        "Catchment              A(ac)   C      LS      A(tons/ac/yr)  Risk".into(),
    ];
    for row in &rows {
        lines.push(format!(
            "{:<22} {:6.3}  {:5.2}  {:7.2}  {:13.2}  {}",
            trim(&row.name, 22),
            row.area_acres,
            row.c_factor,
            row.ls_factor,
            row.soil_loss_tons_per_ac_yr,
            row.risk_level
        ));
    }
    lines.push(format!(
        "  Area-weighted soil loss = {weighted:.2} tons/ac/yr (tolerable T = {:.1})",
        state.tolerable_soil_loss_tons_per_ac_yr
    ));
    Ok(lines)
}

pub fn wqv_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let wqv = water_quality::compute_wqv_from_catchments(&catchments, state.wq_volume_factor_inches);
    Ok(vec![
        format!(
            "--- HydroComplete: water quality volume ({}, {} catchments) ---",
            state.name,
            catchments.len()
        ),
        format!(
            "  Area = {:.3} ac   I = {:.1}%   Rv = {:.3}",
            wqv.total_area_acres, wqv.impervious_percent, wqv.runoff_coefficient_rv
        ),
        format!(
            "  Design storm = {:.2} in   WQV = {:.0} cf ({:.3} ac-ft, {:.0} gal)",
            wqv.design_storm_inches, wqv.wqv_cf, wqv.wqv_acre_ft, wqv.wqv_gallons
        ),
    ])
}

pub fn detention_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let rainfall = state.design_storm_inches;
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let system_tc = catchments.iter().map(|c| c.tc_minutes).fold(0.0_f64, f64::max).max(10.0);
    let runoff = scs_runoff::compute_composite(&catchments, rainfall);
    let uh = scs_unit_hydrograph::generate(total_area, system_tc, None, None);
    let inflow = inflow_from_unit_hydrograph(&uh, runoff.composite_runoff_depth_inches);
    let outlets = default_nc_detention_outlets();
    let curve = build_prismatic_storage_indication_curve(50_000.0, &outlets, 8.0);
    let routing = route(&inflow, &curve, detention::DEFAULT_TIMESTEP_HOURS);
    let continuity = detention::continuity_error_percent(&routing);
    Ok(vec![
        format!(
            "--- HydroComplete: detention routing ({}, {} catchments) ---",
            state.name,
            catchments.len()
        ),
        "  Storage: prismatic pond (V_max=50000 ft³, avg depth=8 ft)".into(),
        format!(
            "  Inflow: SCS UH (A={total_area:.3} ac, Tc={system_tc:.1} min, Q_depth={:.3} in)",
            runoff.composite_runoff_depth_inches
        ),
        format!(
            "  Q_in,peak = {:.2} cfs   Q_out,peak = {:.2} cfs   attenuation = {:.1}%",
            routing.peak_inflow_cfs, routing.peak_outflow_cfs, routing.reduction_percent
        ),
        format!(
            "  S_max = {:.0} ft³   peak elev = {:.2} ft   continuity error = {:.2}%",
            routing.peak_storage_ft3, routing.peak_elevation_ft, continuity
        ),
    ])
}

pub fn bmp_size_lines<'a>(entities: impl Iterator<Item = &'a EntityType>, bmp_type_key: &str) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let wqv = water_quality::compute_wqv_from_catchments(&catchments, state.wq_volume_factor_inches);
    let sizing = bmp::size_bmp(
        bmp_type_key,
        state.wq_volume_factor_inches,
        total_area,
        wqv.impervious_percent,
    )?;
    let mut lines = vec![
        format!("--- HydroComplete: BMP sizing ({}, {}) ---", sizing.bmp_name, state.name),
        format!(
            "  Area = {:.3} ac   I = {:.1}%   WQ storm = {:.2} in",
            total_area, wqv.impervious_percent, state.wq_volume_factor_inches
        ),
        format!(
            "  WQV = {:.0} cf   treated volume = {:.0} cf",
            sizing.total_wqv_cf, sizing.treated_volume_cf
        ),
        format!(
            "  Surface area = {:.0} sf ({:.2}% site footprint)",
            sizing.surface_area_sf, sizing.footprint_percent
        ),
    ];
    if let Some(len) = sizing.length_ft {
        lines.push(format!(
            "  Length = {:.1} ft   width = {:.1} ft",
            len,
            sizing.width_ft.unwrap_or(0.0)
        ));
    }
    Ok(lines)
}

pub fn wq_train_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let rainfall = state.wq_volume_factor_inches;
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let runoff = scs_runoff::compute_composite(&catchments, rainfall);
    let loads = bmp::calculate_event_pollutant_loads(
        runoff.composite_runoff_depth_inches,
        total_area,
        land_use::RESIDENTIAL,
    );
    let chain = bmp::default_treatment_train();
    let train = bmp::apply_treatment_train(&loads, &chain)?;
    let mut lines = vec![
        format!(
            "--- HydroComplete: BMP treatment train ({}, {} BMPs, {} catchments) ---",
            state.name,
            chain.len(),
            catchments.len()
        ),
        format!(
            "  Land use = {}   runoff depth = {:.3} in   area = {:.3} ac",
            land_use::RESIDENTIAL,
            runoff.composite_runoff_depth_inches,
            total_area
        ),
        "  Pollutant        Influent(lbs)  Effluent(lbs)  Removed(lbs)  eta".into(),
    ];
    for p in pollutant::CORE {
        let influent = train.initial_loads_lbs.get(p).copied().unwrap_or(0.0);
        let effluent = train.final_effluent_lbs.get(p).copied().unwrap_or(0.0);
        let removed = train.total_removed_lbs.get(p).copied().unwrap_or(0.0);
        let eta = train.overall_removal_efficiency.get(p).copied().unwrap_or(0.0);
        lines.push(format!(
            "  {:<14} {:13.4} {:13.4} {:13.4} {:6.1}%",
            p, influent, effluent, removed, eta * 100.0
        ));
    }
    lines.push("  BMP chain:".into());
    for (i, step) in train.bmp_steps.iter().enumerate() {
        let def = bmp::get_bmp(&step.bmp_type)?;
        lines.push(format!("    {}. {} ({})", i + 1, def.name, step.bmp_type));
    }
    Ok(lines)
}

pub fn sediment_basin_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let system_tc = catchments.iter().map(|c| c.tc_minutes).fold(0.0_f64, f64::max).max(10.0);
    let runoff = scs_runoff::compute_composite(&catchments, state.design_storm_inches);
    let uh = scs_unit_hydrograph::generate(total_area, system_tc, None, None);
    let peak_q = uh.peak_flow_cfs * runoff.composite_runoff_depth_inches;
    let rusle_rows: Vec<_> = catchments
        .iter()
        .map(|c| sediment::rusle(c.area_acres, 5.0, 300.0, c.runoff_c, state.default_r_factor, 0.32, 1.0, &c.name))
        .collect();
    let sediment_yield = sediment::weighted_average_soil_loss(&rusle_rows);
    let design = bmp::design_sediment_basin(peak_q, total_area, sediment_yield);
    Ok(vec![
        format!("--- HydroComplete: sediment basin ({}, Q={peak_q:.2} cfs) ---", state.name),
        format!(
            "  Drainage area = {:.3} ac   sediment yield = {sediment_yield:.2} tons/ac/yr",
            total_area
        ),
        format!(
            "  Surface area = {:.0} sf   L = {:.1} ft   W = {:.1} ft   depth = {:.1} ft",
            design.surface_area_sf, design.length_ft, design.width_ft, design.depth_ft
        ),
        format!(
            "  Pool volume = {:.0} cf   sediment storage = {:.0} cf   total = {:.0} cf",
            design.pool_volume_cf, design.sediment_storage_cf, design.total_volume_cf
        ),
        format!(
            "  Forebay = {:.0} cf ({:.1} x {:.1} ft)   trapping = {:.1}%",
            design.forebay_volume_cf, design.forebay_length_ft, design.forebay_width_ft, design.trapping_efficiency_pct
        ),
        format!("  Dewatering time = {:.0} hr", design.dewatering_time_hr),
    ])
}

pub fn prepost_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let (pre, post) = prepost::watershed_from_catchments(&catchments, None);
    let storms: HashMap<String, f64> = peak_storm_suite(state.code)
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    let pond = default_detention_pond();
    let result = prepost::run(&pre, &post, &storms, Some(&pond));
    let mut lines = vec![
        format!(
            "--- HydroComplete: pre/post peak comparison ({}, {:.3} ac) ---",
            state.name, post.area_acres
        ),
        format!(
            "  Pre-dev CN = {:.1}   Post-dev CN = {:.1}   Tc = {:.2} hr",
            pre.curve_number, post.curve_number, post.tc_hours
        ),
        "  Storm          P(in)  Q_pre(cfs)  Q_post unr(cfs)  Q_post rt(cfs)  PASS  Margin(cfs)".into(),
    ];
    let pass_count = result.rows.iter().filter(|r| r.pass).count();
    for row in &result.rows {
        lines.push(format!(
            "  {:<12} {:5.2} {:11.1} {:16.1} {:15.1} {:>5} {:10.1}",
            row.return_period,
            row.rainfall_in,
            row.pre_development.peak_flow_cfs,
            row.post_development.peak_unrouted_cfs,
            row.post_development.peak_routed_cfs,
            if row.pass { "OK" } else { "FAIL" },
            row.margin_cfs
        ));
    }
    lines.push(format!(
        "  Overall: {} ({} of {} storms pass, tolerance ×{:.2})",
        if result.all_pass { "PASS" } else { "FAIL" },
        pass_count,
        result.rows.len(),
        prepost::PASS_TOLERANCE_FACTOR
    ));
    Ok(lines)
}

pub fn optimize_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let wqv = water_quality::compute_wqv_from_catchments(&catchments, state.wq_volume_factor_inches);
    let site = SiteData {
        area_acres: total_area,
        impervious_percent: wqv.impervious_percent,
        rainfall_depth_in: state.wq_volume_factor_inches,
        ..Default::default()
    };
    let mut targets = HashMap::from([(pollutant::TSS.to_string(), state.tss_removal_percent / 100.0)]);
    if state.tn_removal_percent > 0.0 {
        targets.insert(pollutant::TN.to_string(), state.tn_removal_percent / 100.0);
    }
    if state.tp_removal_percent > 0.0 {
        targets.insert(pollutant::TP.to_string(), state.tp_removal_percent / 100.0);
    }
    let trains = bmp_optimizer::optimize_treatment_train(&site, &targets);
    let mut lines = vec![
        format!("--- HydroComplete: BMP treatment-train optimization ({}) ---", state.name),
        format!(
            "  Site = {:.3} ac   I = {:.1}%   WQV storm = {:.2} in",
            total_area, wqv.impervious_percent, state.wq_volume_factor_inches
        ),
        format!("  Targets: TSS {:.1}%", state.tss_removal_percent),
        format!(
            "  Evaluated {} valid train(s). Top 3 by lifecycle NPV:",
            trains.total_evaluated
        ),
    ];
    for (rank, train) in trains.all_trains.iter().take(3).enumerate() {
        let chain = train.names.join(" → ");
        let tss_eta = train.combined_removal.get(pollutant::TSS).copied().unwrap_or(0.0) * 100.0;
        lines.push(format!(
            "  #{}  {}  (${:.0} NPV, TSS η={tss_eta:.1}%)",
            rank + 1,
            chain,
            train.total_cost
        ));
    }
    if let Some(best) = &trains.best_train {
        lines.push(format!(
            "  Best: {} (${:.0} NPV)",
            best.names.join(" → "),
            best.total_cost
        ));
    } else {
        lines.push("  No train met all pollutant targets.".into());
    }
    Ok(lines)
}

pub fn bioretention_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let rainfall = state.design_storm_inches;
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let (_, weighted_cn, runoff_depth) = bmp::composite_runoff_depth(&catchments, rainfall);
    let design_volume = bmp::runoff_volume_cf(runoff_depth, total_area);
    let surface_area = total_area * 43560.0 * 0.05;
    let config = bmp::BioretentionConfig {
        ksat_in_per_hr: 1.0,
        media_depth_ft: 2.5,
        ponding_depth_ft: 1.0,
    };
    let result = bmp::route_bioretention(&config, design_volume, surface_area);
    let mut lines = vec![
        format!("--- HydroComplete: bioretention routing ({}, P={rainfall:.2} in) ---", state.name),
        format!(
            "  A = {:.0} ac   CN = {:.1}   Q = {runoff_depth:.3} in   V_design = {:.0} cf",
            total_area, weighted_cn, design_volume
        ),
        format!(
            "  V_treated = {:.0} cf   V_bypass = {:.0} cf ({:.1}% bypass)",
            result.treated_volume_cf, result.overflow_volume_cf, result.bypass_fraction_percent
        ),
        format!(
            "  t_res = {:.1} hr   drawdown = {:.1} hr",
            result.residence_time_hr, result.drawdown_time_hr
        ),
    ];
    for p in [pollutant::TSS, pollutant::TN, pollutant::TP] {
        if let Some(e) = result.removal_efficiency.get(p) {
            lines.push(format!(
                "  E_{p} = {:.1}% treated / {:.1}% blended",
                e.treated_percent, e.blended_percent
            ));
        }
    }
    Ok(lines)
}

pub fn wetland_lines<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Result<Vec<String>, String> {
    let catchments = engine_catchments(entities);
    if catchments.is_empty() {
        return Ok(vec!["No catchments found.".into()]);
    }
    let state = default_state();
    let rainfall = state.design_storm_inches;
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let (_, _, runoff_depth) = bmp::composite_runoff_depth(&catchments, rainfall);
    let design_volume = bmp::runoff_volume_cf(runoff_depth, total_area);
    let surface_area = (total_area * 43560.0 * 0.08).max(10_000.0);
    let result = bmp::route_constructed_wetland(design_volume, surface_area);
    let mut lines = vec![
        format!("--- HydroComplete: constructed wetland routing ({}, P={rainfall:.2} in) ---", state.name),
        format!(
            "  V_design = {:.0} cf   A_wetland = {:.0} sf   zones = {}",
            design_volume, surface_area, result.zone_count
        ),
        format!("  Method: {}", result.method),
    ];
    for p in [pollutant::TSS, pollutant::TN, pollutant::TP] {
        if let Some(e) = result.removal_efficiency.get(p) {
            lines.push(format!("  E_{p} = {:.1}%", e.treated_percent));
        }
    }
    Ok(lines)
}

use hydrocomplete::soil_database::BmpSuitability;

pub fn soil_lines(args: &str) -> Result<Vec<String>, String> {
    use hydrocomplete::soil_database;
    use hydrocomplete::ssurgo::{SsugroFetcher, SsugroResolution, SsugroSource};

    let tokens: Vec<&str> = args.split_whitespace().collect();
    let (resolution, live) = if tokens.first().map(|t| t.eq_ignore_ascii_case("LIVE")).unwrap_or(false) {
        let lat: f64 = tokens
            .get(1)
            .ok_or("HC_SOIL LIVE <lat> <lon> [BMP Bioretention]")?
            .parse()
            .map_err(|_| "Invalid latitude")?;
        let lon: f64 = tokens
            .get(2)
            .ok_or("HC_SOIL LIVE <lat> <lon> [BMP Bioretention]")?
            .parse()
            .map_err(|_| "Invalid longitude")?;
        let fetcher = SsugroFetcher::new(Some(hydrocomplete::ssurgo::default_cache_directory()));
        let res = fetcher.resolve(lat, lon);
        (res, true)
    } else if tokens.first().map(|t| t.eq_ignore_ascii_case("NAME")).unwrap_or(false) {
        let name = tokens.get(1).ok_or("HC_SOIL NAME <soil series> [BMP Bioretention]")?;
        let res = SsugroResolution::embedded(name, 0.0, 0.0)?;
        (res, false)
    } else if tokens.is_empty() {
        let res = SsugroResolution::embedded("cecil-sandy-loam", 35.23, -80.84)?;
        (res, false)
    } else {
        let name = tokens[0];
        let res = SsugroResolution::embedded(name, 0.0, 0.0)?;
        (res, false)
    };

    let bmp_type = parse_soil_bmp_arg(&tokens);
    let soil = resolution.to_soil_properties();
    let suggestion = soil_database::suggest_bmp(&soil, &bmp_type);

    let mut lines = vec!["--- HydroComplete: soil lookup ---".into()];
    if live || resolution.source != SsugroSource::Embedded {
        let fallback = if resolution.map_unit.is_fallback { " (fallback)" } else { "" };
        lines.push(format!(
            "  Source: {}{fallback}",
            resolution.source.as_str()
        ));
        if let Some(w) = &resolution.map_unit.warning {
            lines.push(format!("  Warning: {w}"));
        }
        if let Some(hz) = &resolution.map_unit.surface_horizon {
            if let (Some(s), Some(si), Some(c)) = (hz.pct_sand, hz.pct_silt, hz.pct_clay) {
                lines.push(format!("  PSD: sand {s:.1}%  silt {si:.1}%  clay {c:.1}%"));
            }
        }
    }
    lines.push(format!("  {} ({})", soil.name, soil.key));
    lines.push(format!("  Region: {}   Texture: {}", soil.region, soil.texture));
    lines.push(format!(
        "  HSG: {}   K-factor: {:.2}   fc: {:.2} in/hr",
        soil.hydrologic_soil_group, soil.k_factor, soil.infiltration_rate_in_per_hr
    ));
    lines.push(format!("  Drainage: {}", soil.drainage));
    lines.push(format!(
        "  BMP '{}' suitability: {}",
        suggestion.bmp_type,
        suitability_label(suggestion.suitability)
    ));
    lines.push(format!("  {}", suggestion.rationale));
    if !suggestion.alternatives.is_empty() {
        lines.push(format!("  Alternatives: {}", suggestion.alternatives.join(", ")));
    }
    Ok(lines)
}

fn parse_soil_bmp_arg(tokens: &[&str]) -> String {
    if let Some(pos) = tokens.iter().position(|t| t.eq_ignore_ascii_case("BMP")) {
        if let Some(bmp) = tokens.get(pos + 1) {
            return normalize_soil_bmp_keyword(bmp);
        }
    }
    hydrocomplete::bmp::bmp_type::BIORETENTION.to_string()
}

fn normalize_soil_bmp_keyword(bmp: &str) -> String {
    match bmp.to_ascii_lowercase().as_str() {
        "wetpond" | "wet-pond" => hydrocomplete::bmp::bmp_type::WET_POND.into(),
        "wetland" | "constructed-wetland" => "constructed-wetland".into(),
        "bioretention" => hydrocomplete::bmp::bmp_type::BIORETENTION.into(),
        other => other.into(),
    }
}

fn suitability_label(s: BmpSuitability) -> &'static str {
    match s {
        BmpSuitability::Excellent => "Excellent",
        BmpSuitability::Good => "Good",
        BmpSuitability::Marginal => "Marginal",
        BmpSuitability::Poor => "Poor",
        BmpSuitability::NotRecommended => "NotRecommended",
    }
}

pub fn stub_message(cmd: &str) -> String {
    format!(
        "{cmd}: planned — see HC_ABOUT for available commands."
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