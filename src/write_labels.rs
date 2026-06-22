//! Write MText labels to the drawing (HC_PIPES_WRITE, HC_CAPACITY_WRITE).

use acadrust::types::Vector3;
use acadrust::{EntityType, MText};
use hydrocomplete::manning;
use stormsewer::params::StormAnalysisParams;

use crate::analysis;
use crate::data;

const LAYER: &str = "HC-CAPACITY";

pub fn plan_capacity_labels<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
) -> Result<Vec<EntityType>, String> {
    let pipes = data::pipe_segments(entities)?;
    let mut out = Vec::new();
    for p in &pipes {
        let cap = manning::capacity(&p.segment);
        let text = format!(
            "Qfull={:.1} cfs\nVfull={:.1} fps",
            cap.full_flow_cfs, cap.full_velocity_fps
        );
        out.push(label_at(p.mid_x, p.mid_y, &text));
    }
    Ok(out)
}

pub fn plan_design_capacity_labels<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
    overload_only: bool,
) -> Result<Vec<EntityType>, String> {
    let drawn = data::drawn_network_from_entities(entities)?;
    let analysis = analysis::run_analysis_on_network(&drawn.network, params)?;
    let pipes = data::pipe_segments_from_drawn(&drawn, Some(&analysis))?;
    let mut out = Vec::new();
    for p in &pipes {
        let nd = manning::normal_depth(&p.segment, p.design_q_cfs);
        if overload_only && !nd.surcharged {
            continue;
        }
        let cap = manning::capacity(&p.segment);
        let flag = if nd.surcharged { " SURCH" } else { "" };
        let text = format!(
            "Q={:.1} Qfull={:.1}\nd/D={:.2}{}",
            p.design_q_cfs, cap.peak_flow_cfs, nd.relative_depth, flag
        );
        out.push(label_at(p.mid_x, p.mid_y, &text));
    }
    Ok(out)
}

fn label_at(x: f64, y: f64, text: &str) -> EntityType {
    let mut mt = MText::default();
    mt.insertion_point = Vector3::new(x, y, 0.0);
    mt.value = text.to_string();
    mt.height = 2.5;
    mt.common.layer = LAYER.to_string();
    EntityType::MText(mt)
}