use std::collections::HashMap;

use ocs_plugin_api::host::{ensure_plugin_state, HostApi};

use acadrust::EntityType;
use acadrust::Handle;

use stormsewer::network::NodeKind;

use super::analysis;
use super::commands;
use super::edit;
use super::landxml_import;
use super::manifest::PLUGIN_ID;
use super::params_cmd;
use super::background;
use super::interactive::{AttachBackgroundInteractive, PlacePipeInteractive, PlaceStructureInteractive};
use super::network_edit;
use super::placement;
use super::report_export;
use super::sizing;
use super::state::HydroTabState;
use super::validation;
use super::{data, style, write_labels};

fn tab_params(host: &mut dyn HostApi) -> stormsewer::params::StormAnalysisParams {
    ensure_plugin_state(host, PLUGIN_ID, HydroTabState::default)
        .params()
        .clone()
}

fn entities<'a>(host: &'a dyn HostApi) -> impl Iterator<Item = &'a EntityType> {
    host.document().entities()
}

fn entities_mut<'a>(host: &'a mut dyn HostApi) -> impl Iterator<Item = &'a mut EntityType> {
    host.document_mut().entities_mut()
}

/// Everything after the first token (preserves spaces in file paths).
fn command_arg(cmd: &str) -> Option<&str> {
    let mut parts = cmd.splitn(2, char::is_whitespace);
    parts.next()?;
    parts.next().map(str::trim).filter(|s| !s.is_empty())
}

fn drawing_key(host: &dyn HostApi) -> String {
    format!("tab-{}", host.tab_index())
}

fn run_validation(host: &mut dyn HostApi, block_on_error: bool) -> bool {
    let report = validation::validate_entities(entities(host));
    report.emit_to_host(host);
    if block_on_error && !report.ok() {
        return false;
    }
    true
}

/// Handle any `HC_*` command. Returns true when consumed.
pub fn handle(host: &mut dyn HostApi, cmd: &str) -> bool {
    if !cmd.starts_with("HC_") {
        return false;
    }

    match cmd {
        "HC_VALIDATE" => {
            // Integrity checks (XDATA well-formed, handles resolve) ...
            let mut report = validation::validate_entities(entities(host));
            // ... then design-criteria review on the analyzed network. Best-effort:
            // if the network can't be built/analyzed, the integrity report above
            // already explains why, so we just skip the design pass.
            let params = tab_params(host);
            if let Ok(findings) = analysis::design_review_doc(entities(host), &params) {
                for f in findings {
                    match f.severity {
                        stormsewer::design::Severity::Error => report.errors.push(f.message),
                        stormsewer::design::Severity::Warning => report.warnings.push(f.message),
                    }
                }
            }
            report.emit_to_host(host);
            true
        }
        "HC_ANALYZE" => {
            if !run_validation(host, false) {}
            let report = validation::validate_entities(entities(host));
            if !report.ok() {
                return true;
            }
            let params = tab_params(host);
            let state = crate::analyze_full::default_state_code();
            if crate::analyze_full::has_catchments(entities(host)) {
                let dk = drawing_key(host);
                match crate::analyze_full::run_full_analysis(
                    entities(host), &params, state, "residential", Some(&dk),
                ) {
                    Ok(full) => {
                        let idf = format!(
                            "RP {}yr  a={:.1} b={:.1} c={:.2}",
                            params.idf.design_rp,
                            params.idf.design_curve().a,
                            params.idf.design_curve().b,
                            params.idf.design_curve().c,
                        );
                        for line in commands::analyze_summary_lines(&full, &idf) {
                            host.push_output(&line);
                        }
                        if let Ok(drawn) = data::drawn_network_from_entities(entities(host)) {
                            if let Ok(analysis) =
                                analysis::run_analysis_on_network(&drawn.network, &params)
                            {
                                host.push_undo("HC_STYLE");
                                let (sur, flood) =
                                    style::apply_analysis_style(entities_mut(host), &drawn, &analysis);
                                if sur > 0 || flood > 0 {
                                    host.set_dirty();
                                    host.push_info(&format!(
                                        "Styled {sur} surcharged pipe(s), {flood} flooded structure(s)."
                                    ));
                                }
                            }
                        }
                        host.bump_geometry();
                        host.push_info(&format!(
                            "HydroComplete full analysis ({state}, {}).",
                            params.summary()
                        ));
                    }
                    Err(e) => host.push_error(&e),
                }
            } else {
                match analysis::analyze_doc(entities(host), &params) {
                    Ok((ents, report, analysis)) => {
                        for e in ents {
                            let _ = host.add_entity(e);
                        }
                        if let Ok(drawn) = data::drawn_network_from_entities(entities(host)) {
                            host.push_undo("HC_STYLE");
                            let (sur, flood) =
                                style::apply_analysis_style(entities_mut(host), &drawn, &analysis);
                            if sur > 0 || flood > 0 {
                                host.set_dirty();
                                host.push_info(&format!(
                                    "Styled {sur} surcharged pipe(s), {flood} flooded structure(s)."
                                ));
                            }
                        }
                        host.bump_geometry();
                        host.push_info(&format!(
                            "HydroComplete analyzed ({}). Tag catchments for full pipeline.",
                            params.summary()
                        ));
                        for line in report.lines() {
                            host.push_output(line);
                        }
                    }
                    Err(e) => host.push_error(&e),
                }
            }
            true
        }
        "HC_REPORT" => {
            let params = tab_params(host);
            let drawing_name = format!("tab-{}", host.tab_index());
            match report_export::export_hydraulic_report(entities(host), &params, &drawing_name) {
                Ok((path, design_q)) => {
                    host.push_output("--- HydroComplete: HTML report written ---");
                    host.push_output(&format!(
                        "  Manning capacity + steady HGL (Q={design_q:.1} cfs) -> {}",
                        path.display()
                    ));
                    host.push_info("Open the HTML file in a browser (KaTeX formulas load from CDN).");
                }
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_REPORT_PDF" => {
            let params = tab_params(host);
            let drawing_name = format!("tab-{}", host.tab_index());
            if !hydrocomplete::license::is_pro_enabled() {
                for line in report_export::pro_required_lines() {
                    host.push_output(&line);
                }
                return true;
            }
            match report_export::export_hydraulic_report_pdf(entities(host), &params, &drawing_name) {
                Ok((path, design_q)) => {
                    host.push_output("--- HydroComplete: PDF report written ---");
                    host.push_output(&format!(
                        "  Manning capacity + steady HGL (Q={design_q:.1} cfs) -> {}",
                        path.display()
                    ));
                    host.push_info("Open the PDF from Documents/HydroComplete.");
                }
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_MULTIRP" => {
            let params = tab_params(host);
            match analysis::multi_rp_report(entities(host), &params) {
                Ok(report) => {
                    for line in report.lines() {
                        host.push_output(line);
                    }
                }
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_PROFILE" => {
            let params = tab_params(host);
            match analysis::profile_doc(entities(host), &params) {
                Ok(ents) => {
                    for e in ents {
                        let _ = host.add_entity(e);
                    }
                    host.bump_geometry();
                    host.push_info("HydroComplete HGL profile drawn.");
                }
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_SIZE" => {
            let params = tab_params(host);
            match sizing::plan_size_updates(entities(host), &params) {
                Ok((updates, report, pending)) => {
                    for line in report.lines() {
                        host.push_output(line);
                    }
                    if pending == 0 {
                        host.push_info("HydroComplete: all pipes already meet sizing criteria.");
                    } else {
                        host.push_undo("HC_SIZE");
                        let applied = sizing::apply_updates(entities_mut(host), &updates);
                        host.bump_geometry();
                        host.set_dirty();
                        host.push_info(&format!(
                            "HydroComplete: applied {applied} pipe diameter update(s)."
                        ));
                    }
                }
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_APPLYTC" => {
            host.push_undo("HC_APPLYTC");
            let tc_by_handle: HashMap<Handle, f64> =
                match data::drawn_network_from_entities(entities(host)) {
                    Ok(drawn) => drawn
                        .network
                        .nodes
                        .iter()
                        .zip(drawn.node_handles.iter())
                        .filter(|(node, _)| node.kind != NodeKind::Outfall)
                        .map(|(node, &h)| (h, node.tc_inlet))
                        .collect(),
                    Err(e) => {
                        host.push_error(&e);
                        HashMap::new()
                    }
                };
            let updated = data::apply_tc_map(entities_mut(host), &tc_by_handle);
            if updated > 0 || !tc_by_handle.is_empty() {
                host.set_dirty();
                host.bump_geometry();
                host.push_info(&format!(
                    "HydroComplete: updated inlet Tc on {updated} structure(s)."
                ));
            }
            true
        }
        cmd if cmd == "HC_PARAMS" || cmd.starts_with("HC_PARAMS ") => {
            let rest = cmd.trim_start_matches("HC_PARAMS").trim();
            let state = ensure_plugin_state(host, PLUGIN_ID, HydroTabState::default);
            match params_cmd::apply_params(state, rest) {
                Ok(msg) => host.push_info(&msg),
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_ABOUT" => {
            for line in commands::about_lines() {
                host.push_output(line);
            }
            host.push_info("HydroComplete for Open CAD Studio loaded.");
            true
        }
        "HC_NETWORK" => {
            let lines = commands::network_summary_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_PIPES" => {
            let lines = commands::pipes_capacity_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_CAPACITY" => {
            let params = tab_params(host);
            let lines = commands::capacity_check_lines(entities(host), &params);
            commands::emit_lines(host, lines);
            true
        }
        "HC_PIPES_WRITE" => {
            match write_labels::plan_capacity_labels(entities(host)) {
                Ok(labels) => {
                    host.push_undo("HC_PIPES_WRITE");
                    for e in labels {
                        let _ = host.add_entity(e);
                    }
                    host.set_dirty();
                    host.bump_geometry();
                    host.push_info("Wrote capacity label(s) on layer HC-CAPACITY.");
                }
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_CAPACITY_WRITE" => {
            let params = tab_params(host);
            match write_labels::plan_design_capacity_labels(entities(host), &params, true) {
                Ok(labels) => {
                    let n = labels.len();
                    host.push_undo("HC_CAPACITY_WRITE");
                    for e in labels {
                        let _ = host.add_entity(e);
                    }
                    host.set_dirty();
                    host.bump_geometry();
                    host.push_info(&format!(
                        "Wrote {n} design-capacity label(s) on layer HC-CAPACITY."
                    ));
                }
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_RATIONAL" => {
            let params = tab_params(host);
            let lines = commands::rational_peak_lines(entities(host), &params);
            commands::emit_lines(host, lines);
            true
        }
        "HC_SCS" => {
            let lines = commands::scs_runoff_lines(entities(host), 3.0);
            commands::emit_lines(host, lines);
            true
        }
        cmd if cmd == "HC_ATLAS14" || cmd.starts_with("HC_ATLAS14 ") => {
            let args = command_arg(cmd).unwrap_or("");
            commands::emit_lines(host, commands::atlas14_lines(args));
            true
        }
        "HC_LICENSE" => {
            for line in commands::license_lines() {
                host.push_output(&line);
            }
            true
        }
        cmd if cmd == "HC_ACTIVATE" || cmd.starts_with("HC_ACTIVATE ") => {
            let args = command_arg(cmd).unwrap_or("");
            for line in commands::activate_lines(args) {
                host.push_output(&line);
            }
            true
        }
        "HC_HGL" => {
            let params = tab_params(host);
            match analysis::profile_doc(entities(host), &params) {
                Ok(ents) => {
                    for e in ents {
                        let _ = host.add_entity(e);
                    }
                    host.bump_geometry();
                    host.push_info("HydroComplete: HGL profile drawn on HC-HGL-PROFILE.");
                }
                Err(e) => host.push_error(&e),
            }
            true
        }
        cmd if cmd == "HC_GVF" || cmd.starts_with("HC_GVF ") => {
            let args = cmd.trim_start_matches("HC_GVF").trim();
            commands::emit_lines(host, commands::gvf_lines(args));
            true
        }
        cmd if cmd == "HC_CULVERT" || cmd.starts_with("HC_CULVERT ") => {
            let args = cmd.trim_start_matches("HC_CULVERT").trim();
            for line in commands::culvert_lines(args) {
                host.push_output(&line);
            }
            true
        }
        cmd if cmd == "HC_TC" || cmd.starts_with("HC_TC ") => {
            let args = cmd.trim_start_matches("HC_TC").trim();
            for line in commands::tc_lines(args) {
                host.push_output(&line);
            }
            true
        }
        cmd if cmd == "HC_INLETS" || cmd.starts_with("HC_INLETS ") => {
            let params = tab_params(host);
            let args = cmd.trim_start_matches("HC_INLETS").trim();
            let lines = commands::inlets_lines(entities(host), &params, args);
            commands::emit_lines(host, lines);
            true
        }
        cmd if cmd == "HC_HYDROGRAPH" || cmd.starts_with("HC_HYDROGRAPH ") => {
            let args = cmd.trim_start_matches("HC_HYDROGRAPH").trim();
            let lines = commands::hydrograph_lines(entities(host), args);
            commands::emit_lines(host, lines);
            true
        }
        cmd if cmd == "HC_ROUTE_HYDRO" || cmd.starts_with("HC_ROUTE_HYDRO ") => {
            let params = tab_params(host);
            let args = cmd.trim_start_matches("HC_ROUTE_HYDRO").trim();
            let lines = commands::route_hydro_lines(entities(host), &params, args);
            commands::emit_lines(host, lines);
            true
        }
        cmd if cmd == "HC_PUMP" || cmd.starts_with("HC_PUMP ") => {
            let args = cmd.trim_start_matches("HC_PUMP").trim();
            for line in commands::pump_lines(args) {
                host.push_output(&line);
            }
            true
        }
        "HC_COST" => {
            let lines = commands::cost_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        cmd if cmd == "HC_PROFILE_DXF" || cmd.starts_with("HC_PROFILE_DXF ") => {
            let params = tab_params(host);
            let args = cmd.trim_start_matches("HC_PROFILE_DXF").trim();
            let lines = commands::profile_dxf_export(entities(host), &params, "drawing", args);
            commands::emit_lines(host, lines);
            true
        }
        "HC_NETWORK_DIAGRAM" => {
            let params = tab_params(host);
            let lines = commands::network_diagram_export(entities(host), &params, "drawing");
            commands::emit_lines(host, lines);
            true
        }
        "HC_LANDXML" => {
            let lines = commands::landxml_export(entities(host), "drawing", "");
            commands::emit_lines(host, lines);
            true
        }
        cmd if cmd.starts_with("HC_LANDXML ") => {
            let args = cmd.trim_start_matches("HC_LANDXML").trim();
            if args.ends_with(".xml") || args.contains('\\') || args.contains('/') {
                match std::fs::read_to_string(args) {
                    Ok(xml) => match landxml_import::import_landxml(host, &xml) {
                        Ok(msg) => host.push_info(&msg),
                        Err(e) => host.push_error(&e),
                    },
                    Err(e) => host.push_error(&format!("cannot read {args}: {e}")),
                }
            } else {
                let lines = commands::landxml_export(entities(host), "drawing", args);
                commands::emit_lines(host, lines);
            }
            true
        }
        cmd if cmd == "HC_LANDXML_IMPORT" || cmd == "HC_IMPORTXML" => {
            host.push_info("Use HC_LANDXML_IMPORT <path-to-file> to import LandXML.");
            true
        }
        cmd if cmd.starts_with("HC_LANDXML_IMPORT ") || cmd.starts_with("HC_IMPORTXML ") => {
            let Some(path) = command_arg(cmd) else {
                host.push_error("Expected: HC_LANDXML_IMPORT <path-to-landxml-file>");
                return true;
            };
            match std::fs::read_to_string(path) {
                Ok(xml) => match landxml_import::import_landxml(host, &xml) {
                    Ok(msg) => host.push_info(&msg),
                    Err(e) => host.push_error(&e),
                },
                Err(e) => host.push_error(&format!("cannot read {path}: {e}")),
            }
            true
        }
        "HC_REVIEW" => {
            let params = tab_params(host);
            let state_code = crate::analyze_full::default_state_code();
            let state = hydrocomplete::state_compliance::get(state_code);
            let dev_type = "residential";
            if crate::analyze_full::has_catchments(entities(host)) {
                let dk = drawing_key(host);
                match crate::analyze_full::run_full_analysis(
                    entities(host), &params, state_code, dev_type, Some(&dk),
                ) {
                    Ok(full) => {
                        if let Some(comp) = &full.compliance {
                            match commands::review_summary_lines(
                                entities(host), &params, comp, &state.name, dev_type,
                            ) {
                                Ok(lines) => commands::emit_lines(host, Ok(lines)),
                                Err(e) => host.push_error(&e),
                            }
                        }
                    }
                    Err(e) => host.push_error(&e),
                }
            } else {
                let catchments = data::catchments_from_entities(entities(host));
                let mut compliance_input =
                    hydrocomplete::compliance::ComplianceAnalysisResults::default();
                if !catchments.is_empty() {
                    let engine_catchments: Vec<hydrocomplete::models::Catchment> = catchments
                        .iter()
                        .map(|c| hydrocomplete::models::Catchment {
                            name: c.name.clone(),
                            area_acres: c.area_acres,
                            runoff_c: c.runoff_c,
                            curve_number: c.curve_number,
                            tc_minutes: c.tc_minutes,
                            outfall_structure_id: None,
                            outfall_structure_name: None,
                        })
                        .collect();
                    let wqv = hydrocomplete::water_quality::compute_wqv_from_catchments(
                        &engine_catchments,
                        state.wq_volume_factor_inches,
                    );
                    let sediment_rows: Vec<_> = catchments
                        .iter()
                        .map(|c| hydrocomplete::sediment::rusle(
                            c.area_acres, 5.0, 300.0, c.runoff_c, state.default_r_factor, 0.32, 1.0, &c.name,
                        ))
                        .collect();
                    compliance_input.water_quality =
                        Some(hydrocomplete::compliance::WaterQualityComplianceInput {
                            bmp_count: 0,
                            wqv_required_cf: Some(wqv.wqv_cf),
                            wqv_provided_cf: Some(0.0),
                            ..Default::default()
                        });
                    compliance_input.sediment =
                        Some(hydrocomplete::compliance::SedimentComplianceInput {
                            total_soil_loss_tons_per_ac_yr:
                                hydrocomplete::sediment::weighted_average_soil_loss(&sediment_rows),
                            sediment_control_count: 0,
                            watershed_results: sediment_rows
                                .iter()
                                .map(|r| hydrocomplete::compliance::WatershedSedimentInput {
                                    name: r.name.clone(),
                                    risk_level: r.risk_level.clone(),
                                })
                                .collect(),
                        });
                }
                if let Ok(drawn) = data::drawn_network_from_entities(entities(host)) {
                    if let Ok(analysis) = analysis::run_analysis_on_network(&drawn.network, &params) {
                        let post = analysis.pipes.iter().map(|p| p.design_q).fold(0.0_f64, f64::max);
                        if post > 0.0 {
                            compliance_input.hydrology =
                                Some(hydrocomplete::compliance::HydrologyComplianceInput {
                                    has_detention: false,
                                    pre_peak_cfs: Some(post * 0.8),
                                    post_peak_cfs: Some(post),
                                });
                        }
                    }
                }
                let comp = hydrocomplete::compliance::check_compliance(
                    &compliance_input, state_code, dev_type,
                );
                match commands::review_summary_lines(
                    entities(host), &params, &comp, &state.name, dev_type,
                ) {
                    Ok(lines) => commands::emit_lines(host, Ok(lines)),
                    Err(e) => host.push_error(&e),
                }
            }
            true
        }
        "HC_UNIT_HYDRO" => {
            let lines = commands::unit_hydro_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_SEDIMENT" => {
            let lines = commands::sediment_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_WQV" => {
            let lines = commands::wqv_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_DETENTION" => {
            let lines = commands::detention_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_BMP_SIZE" => {
            let lines = commands::bmp_size_lines(entities(host), hydrocomplete::bmp::bmp_type::BIORETENTION);
            commands::emit_lines(host, lines);
            true
        }
        cmd if cmd.starts_with("HC_BMP_SIZE ") => {
            let bmp = command_arg(cmd).unwrap_or(hydrocomplete::bmp::bmp_type::BIORETENTION);
            let lines = commands::bmp_size_lines(entities(host), bmp);
            commands::emit_lines(host, lines);
            true
        }
        "HC_WQ_TRAIN" => {
            let lines = commands::wq_train_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_SEDIMENT_BASIN" => {
            let lines = commands::sediment_basin_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_PREPOST" => {
            let lines = commands::prepost_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_OPTIMIZE" => {
            let lines = commands::optimize_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_BIORETENTION" => {
            let lines = commands::bioretention_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        "HC_WETLAND" => {
            let lines = commands::wetland_lines(entities(host));
            commands::emit_lines(host, lines);
            true
        }
        cmd if cmd == "HC_NETWORK_EDIT" || cmd.starts_with("HC_NETWORK_EDIT ") => {
            let args = command_arg(cmd).unwrap_or("");
            let drawing = drawing_key(host);
            let ents: Vec<_> = host.document().entities().collect();
            let lines = network_edit::run(&drawing, ents.iter().copied(), args);
            commands::emit_lines(host, lines);
            true
        }
        cmd if cmd == "HC_SOIL" || cmd.starts_with("HC_SOIL ") => {
            let args = command_arg(cmd).unwrap_or("");
            commands::emit_lines(host, commands::soil_lines(args));
            true
        }
        "HC_BACKGROUND" => {
            host.push_info(background::usage());
            true
        }
        cmd if cmd.starts_with("HC_BACKGROUND ") => {
            let rest = command_arg(cmd).unwrap_or("");
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 4 {
                let path = parts[0];
                let x: f64 = parts[1].parse().unwrap_or(0.0);
                let y: f64 = parts[2].parse().unwrap_or(0.0);
                let w: f64 = parts[3].parse().unwrap_or(1000.0);
                match background::attach_direct(host, path, x, y, w) {
                    Ok(msg) => host.push_output(&msg),
                    Err(e) => host.push_error(&e),
                }
            } else if parts.is_empty() {
                host.push_error(background::usage());
            } else {
                let path = rest.trim();
                match AttachBackgroundInteractive::new(path.to_string()) {
                    Ok(interactive) => host.start_interactive(Box::new(interactive)),
                    Err(e) => host.push_error(&e),
                }
            }
            true
        }
        "HC_INLET" => {
            host.start_interactive(Box::new(PlaceStructureInteractive::inlet()));
            true
        }
        cmd if cmd.starts_with("HC_INLET ") => {
            match placement::place_structure(host, NodeKind::Inlet, command_arg(cmd).unwrap_or("")) {
                Ok(msg) => host.push_info(&msg),
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_JUNCTION" => {
            host.start_interactive(Box::new(PlaceStructureInteractive::junction()));
            true
        }
        cmd if cmd.starts_with("HC_JUNCTION ") => {
            match placement::place_structure(host, NodeKind::Junction, command_arg(cmd).unwrap_or("")) {
                Ok(msg) => host.push_info(&msg),
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_OUTFALL" => {
            host.start_interactive(Box::new(PlaceStructureInteractive::outfall()));
            true
        }
        cmd if cmd.starts_with("HC_OUTFALL ") => {
            match placement::place_structure(host, NodeKind::Outfall, command_arg(cmd).unwrap_or("")) {
                Ok(msg) => host.push_info(&msg),
                Err(e) => host.push_error(&e),
            }
            true
        }
        // No bare handler: OCS --serve dispatches the first token (`HC_PIPE_ARGS`) then
        // the full line, so interactive tools would swallow trailing diameter/n tokens.
        cmd if cmd.starts_with("HC_PIPE_ARGS ") => {
            match placement::place_pipe(host, command_arg(cmd).unwrap_or("")) {
                Ok(msg) => host.push_output(&msg),
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_PIPE_ARGS" => {
            host.push_info(placement::usage_pipe_args());
            true
        }
        "HC_PIPE" => {
            host.start_interactive(Box::new(PlacePipeInteractive::new()));
            true
        }
        cmd if cmd.starts_with("HC_PIPE ") => {
            match placement::place_pipe(host, command_arg(cmd).unwrap_or("")) {
                Ok(msg) => host.push_output(&msg),
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_EDIT" => {
            host.push_info(edit::usage());
            true
        }
        cmd if cmd.starts_with("HC_EDIT ") => {
            match edit::edit_entity(host, command_arg(cmd).unwrap_or("")) {
                Ok(msg) => host.push_info(&msg),
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_CATCHMENT" => {
            host.push_info(
                "Tag catchments with HYDROCOMPLETE_CATCHMENT XDATA on closed polylines, or import via LandXML.",
            );
            true
        }
        _ => false,
    }
}