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
use super::interactive::{PlacePipeInteractive, PlaceStructureInteractive};
use super::placement;
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
            if !run_validation(host, false) {
                // warnings only — still attempt analyze unless hard errors
            }
            let report = validation::validate_entities(entities(host));
            if !report.ok() {
                return true;
            }
            let params = tab_params(host);
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
                    host.push_info(&format!("HydroComplete analyzed ({}).", params.summary()));
                    for line in report.lines() {
                        host.push_output(line);
                    }
                }
                Err(e) => host.push_error(&e),
            }
            true
        }
        "HC_REPORT" => {
            let params = tab_params(host);
            match analysis::report_doc(entities(host), &params) {
                Ok(report) => {
                    for line in report.lines() {
                        host.push_output(line);
                    }
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
        "HC_ATLAS14" => {
            for line in commands::atlas14_lines() {
                host.push_output(&line);
            }
            true
        }
        "HC_LICENSE" => {
            for line in commands::license_lines() {
                host.push_output(&line);
            }
            true
        }
        "HC_ACTIVATE" => {
            host.push_info(
                "HC_ACTIVATE: online Pro activation planned for v0.3 (mirrors hydrocomplete.com/civil3d).",
            );
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
        cmd if matches!(
            cmd,
            "HC_REVIEW"
                | "HC_UNIT_HYDRO"
                | "HC_SEDIMENT"
                | "HC_WQV"
                | "HC_DETENTION"
                | "HC_BMP_SIZE"
                | "HC_WQ_TRAIN"
                | "HC_SEDIMENT_BASIN"
                | "HC_PREPOST"
                | "HC_OPTIMIZE"
                | "HC_CULVERT"
                | "HC_GVF"
                | "HC_PROFILE_DXF"
                | "HC_REPORT_PDF"
                | "HC_TC"
                | "HC_INLETS"
                | "HC_NETWORK_EDIT"
                | "HC_NETWORK_DIAGRAM"
                | "HC_PUMP"
                | "HC_COST"
                | "HC_BACKGROUND"
                | "HC_HYDROGRAPH"
                | "HC_ROUTE_HYDRO"
                | "HC_BIORETENTION"
                | "HC_WETLAND"
                | "HC_SOIL"
                | "HC_LANDXML"
        ) || cmd.starts_with("HC_REVIEW ")
            || cmd.starts_with("HC_NETWORK_EDIT ")
        => {
            host.push_info(&commands::stub_message(cmd.split_whitespace().next().unwrap_or(cmd)));
            true
        }
        cmd if cmd == "HC_LANDXML_IMPORT"
            || cmd == "HC_IMPORTXML"
            || cmd == "HC_LANDXML"
        => {
            host.push_info(
                "Use Import LandXML on the ribbon, or HC_LANDXML_IMPORT <path-to-file>.",
            );
            true
        }
        cmd if cmd.starts_with("HC_LANDXML_IMPORT ")
            || cmd.starts_with("HC_IMPORTXML ")
            || cmd.starts_with("HC_LANDXML ")
        => {
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
        "HC_PIPE" => {
            host.start_interactive(Box::new(PlacePipeInteractive::new()));
            true
        }
        cmd if cmd.starts_with("HC_PIPE ") => {
            match placement::place_pipe(host, command_arg(cmd).unwrap_or("")) {
                Ok(msg) => host.push_info(&msg),
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