//! Bridge XDATA entities → `NetworkAnalysisPipeline` → text summary.

use acadrust::EntityType;
use hydrocomplete::models::{Catchment, NetworkAnalysisPipe, NetworkPipeLink};
use hydrocomplete::network_analysis::{self, NetworkAnalysisInput, NetworkAnalysisResult};
use hydrocomplete::state_compliance;
use stormsewer::network::Analysis;
use stormsewer::params::StormAnalysisParams;

use crate::analysis;
use crate::data::{self, DrawnNetwork};
use crate::network_override;

const DEFAULT_STATE: &str = "NC";

pub fn has_catchments<'a>(entities: impl Iterator<Item = &'a EntityType>) -> bool {
    !data::catchments_from_entities(entities).is_empty()
}

pub fn run_full_analysis<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
    state_code: &str,
    development_type: &str,
    drawing_key: Option<&str>,
) -> Result<NetworkAnalysisResult, String> {
    let ents: Vec<_> = entities.collect();
    let catchment_infos = data::catchments_from_entities(ents.iter().copied());
    if catchment_infos.is_empty() {
        return Err(
            "No catchments found — hydrology and routing require at least one catchment.".into(),
        );
    }

    let state = state_compliance::get(state_code);
    let idf = *params.idf.design_curve();

    let catchments: Vec<Catchment> = catchment_infos
        .iter()
        .map(|c| Catchment {
            name: c.name.clone(),
            area_acres: c.area_acres,
            runoff_c: c.runoff_c,
            curve_number: c.curve_number,
            tc_minutes: c.tc_minutes,
            outfall_structure_id: c.outfall_structure_id.clone(),
            outfall_structure_name: None,
        })
        .collect();

    let (mut pipes, structure_names, storm_network, storm_analysis) =
        build_pipes_and_storm_analysis(ents.iter().copied(), params)?;
    if let Some(key) = drawing_key {
        let overrides = network_override::load(key);
        network_override::apply_to_pipes(&mut pipes, &overrides);
    }

    let input = NetworkAnalysisInput {
        catchments,
        pipes,
        state_code: state.code.to_string(),
        development_type: development_type.to_string(),
        idf,
        scs_design_rainfall_inches: state.design_storm_inches,
        structure_id_to_name: Some(structure_names),
        storm_network,
        storm_analysis,
        review_criteria: None,
        placeholder_bmp_chain: None,
        land_use: "commercial".into(),
        rusle_slope_percent: 5.0,
        rusle_slope_length_ft: 300.0,
    };

    network_analysis::run(&input)
}

fn build_pipes_and_storm_analysis<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<(Vec<NetworkAnalysisPipe>, std::collections::HashMap<String, String>, Option<stormsewer::network::Network>, Option<Analysis>), String>
{
    let drawn = data::drawn_network_from_entities(entities).ok();
    let mut pipes = Vec::new();
    let mut structure_names = std::collections::HashMap::new();
    let mut storm_network = None;
    let mut storm_analysis = None;

    if let Some(ref d) = drawn {
        for (node, &handle) in d.network.nodes.iter().zip(d.node_handles.iter()) {
            let sid = structure_id(handle);
            structure_names.insert(sid, node.id.clone());
        }

        let analysis = analysis::run_analysis_on_network(&d.network, params).ok();
        let drawn_pipes = data::pipe_segments_from_drawn(d, analysis.as_ref()).ok();

        if let Some(dp) = drawn_pipes {
            for (idx, p) in dp.iter().enumerate() {
                let pipe_handle = d.pipe_handles.get(idx).copied();
                let (us_id, ds_id) = pipe_endpoint_ids(d, idx);
                let pipe_key = pipe_handle
                    .map(structure_id)
                    .unwrap_or_else(|| format!("P{}", idx + 1));
                let link = NetworkPipeLink {
                    pipe_key: pipe_key.clone(),
                    network_name: "default".into(),
                    pipe_name: p.name.clone(),
                    upstream_structure_id: us_id.clone(),
                    downstream_structure_id: ds_id.clone(),
                };
                pipes.push(NetworkAnalysisPipe {
                    pipe_key: pipe_key.clone(),
                    network_name: "default".into(),
                    pipe_name: p.name.clone(),
                    link,
                    segment: p.segment.clone(),
                    upstream_node_id: us_id,
                    downstream_node_id: ds_id,
                    length_ft: p.segment.length_ft,
                    upstream_invert_ft: p.segment.start_invert_ft,
                    downstream_invert_ft: p.segment.end_invert_ft,
                });
            }
        }
        storm_network = Some(d.network.clone());
        storm_analysis = analysis;
    }

    Ok((pipes, structure_names, storm_network, storm_analysis))
}

fn structure_id(handle: acadrust::Handle) -> String {
    format!("H{}", handle.value())
}

fn pipe_endpoint_ids(drawn: &DrawnNetwork, pipe_idx: usize) -> (String, String) {
    let pipe = &drawn.network.pipes[pipe_idx];
    let us = drawn
        .network
        .nodes
        .iter()
        .position(|n| n.id == pipe.from)
        .and_then(|i| drawn.node_handles.get(i).copied())
        .map(structure_id)
        .unwrap_or_else(|| pipe.from.clone());
    let ds = drawn
        .network
        .nodes
        .iter()
        .position(|n| n.id == pipe.to)
        .and_then(|i| drawn.node_handles.get(i).copied())
        .map(structure_id)
        .unwrap_or_else(|| pipe.to.clone());
    (us, ds)
}

pub fn default_state_code() -> &'static str {
    DEFAULT_STATE
}

#[cfg(test)]
mod tests {
    use super::*;
    use acadrust::types::Vector3;
    use acadrust::{Circle, EntityType, Handle, Line, LwPolyline};
    use acadrust::entities::LwVertex;
    use acadrust::types::Vector2;
    use stormsewer::network::NodeKind;

    use crate::data::{catchment_xdata, pipe_xdata, structure_xdata};

    fn minimal_drawing() -> Vec<EntityType> {
        let mut s1 = EntityType::Circle(Circle {
            center: Vector3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s1.common_mut().handle = Handle::new(1);
        s1.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Inlet, 100.0, 106.0, 1.0, 0.7));

        let mut s2 = EntityType::Circle(Circle {
            center: Vector3::new(100.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s2.common_mut().handle = Handle::new(2);
        s2.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Outfall, 99.0, 104.0, 0.0, 0.0));

        let mut p = EntityType::Line(Line::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(100.0, 0.0, 0.0),
        ));
        p.common_mut().handle = Handle::new(3);
        p.common_mut()
            .extended_data
            .add_record(pipe_xdata(1.5, 0.013, Handle::new(1), Handle::new(2)));

        let mut pl = LwPolyline::default();
        pl.is_closed = true;
        pl.vertices = vec![
            LwVertex::new(Vector2::new(-20.0, -20.0)),
            LwVertex::new(Vector2::new(20.0, -20.0)),
            LwVertex::new(Vector2::new(20.0, 20.0)),
            LwVertex::new(Vector2::new(-20.0, 20.0)),
        ];
        let mut cat = EntityType::LwPolyline(pl);
        cat.common_mut().handle = Handle::new(10);
        cat.common_mut().extended_data.add_record(catchment_xdata(
            0.7,
            500.0,
            0.02,
            Handle::new(1),
        ));

        vec![s1, s2, p, cat]
    }

    #[test]
    fn full_analysis_runs_with_catchment_and_network() {
        let ents = minimal_drawing();
        let params = StormAnalysisParams::municipal();
        let result = run_full_analysis(ents.iter(), &params, "NC", "residential", None).expect("runs");
        assert!(!result.hydrology.is_empty());
        assert!(result.compliance.is_some());
    }
}