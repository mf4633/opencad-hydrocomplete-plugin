// Visual feedback for analysis results (surcharged pipes, flooded structures).

use acadrust::types::Color;
use acadrust::{EntityType, Handle};

use stormsewer::network::Analysis;

use super::data::DrawnNetwork;

fn color_surcharged_pipe() -> Color {
    Color::from_index(1)
}

fn color_flooded_struct() -> Color {
    Color::from_index(6)
}

/// Build handle → color assignments from an analysis result.
pub fn style_assignments(drawn: &DrawnNetwork, analysis: &Analysis) -> Vec<(Handle, Color)> {
    let mut out = Vec::new();
    for (handle, pr) in drawn.pipe_handles.iter().zip(analysis.pipes.iter()) {
        if pr.surcharged {
            out.push((*handle, color_surcharged_pipe()));
        }
    }
    for (handle, nr) in drawn.node_handles.iter().zip(analysis.nodes.iter()) {
        if nr.surcharge_to_surface {
            out.push((*handle, color_flooded_struct()));
        }
    }
    out
}

/// Apply handle → color assignments to drawing entities.
pub fn apply_colors<'a>(
    entities: impl Iterator<Item = &'a mut EntityType>,
    assignments: &[(Handle, Color)],
) -> usize {
    let mut applied = 0usize;
    for e in entities {
        let h = e.common().handle;
        if let Some((_, color)) = assignments.iter().find(|(handle, _)| *handle == h) {
            e.common_mut().color = *color;
            applied += 1;
        }
    }
    applied
}

/// Color-code pipes and structures from an analysis result.
pub fn apply_analysis_style<'a>(
    entities: impl Iterator<Item = &'a mut EntityType>,
    drawn: &DrawnNetwork,
    analysis: &Analysis,
) -> (usize, usize) {
    let assignments = style_assignments(drawn, analysis);
    let surcharged = analysis.pipes.iter().filter(|p| p.surcharged).count();
    let flooded = analysis.nodes.iter().filter(|n| n.surcharge_to_surface).count();
    let _ = apply_colors(entities, &assignments);
    (surcharged, flooded)
}