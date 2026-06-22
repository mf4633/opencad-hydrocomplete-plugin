// Apply storm-sewer pipe sizing to the drawn network in the active document.

use acadrust::{EntityType, Handle};

use stormsewer::design::SizeOutcome;
use stormsewer::params::StormAnalysisParams;
use stormsewer::report::format_sizing;

use super::data;

/// A pipe diameter change to apply in the document.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeDiameterUpdate {
    pub handle: Handle,
    pub new_diameter_ft: f64,
}

/// Size pipes from drawn entities without modifying the document.
pub fn size_doc_report<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<String, String> {
    let (_, report, _) = plan_size_updates(entities, params)?;
    Ok(report)
}

/// Compute sizing recommendations and the list of pipe updates to apply.
pub fn plan_size_updates<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
) -> Result<(Vec<PipeDiameterUpdate>, String, usize), String> {
    let drawn = data::drawn_network_from_entities(entities)?;
    let (_, recs) = drawn
        .network
        .analyze_and_size_params(params)
        .map_err(|e| e.to_string())?;

    let mut updates = Vec::new();
    for (handle, rec) in drawn.pipe_handles.iter().zip(recs.iter()) {
        if rec.outcome == SizeOutcome::NoSolution {
            continue;
        }
        if (rec.recommended_diameter_ft - rec.current_diameter_ft).abs() < 1e-6 {
            continue;
        }
        updates.push(PipeDiameterUpdate {
            handle: *handle,
            new_diameter_ft: rec.recommended_diameter_ft,
        });
    }

    let report = format_sizing(&recs);
    let pending = updates.len();
    Ok((updates, report, pending))
}

/// Write planned diameter updates onto matching pipe entities.
pub fn apply_updates<'a>(entities: impl Iterator<Item = &'a mut EntityType>, updates: &[PipeDiameterUpdate]) -> usize {
    let mut applied = 0usize;
    for e in entities {
        let h = e.common().handle;
        if let Some(u) = updates.iter().find(|u| u.handle == h) {
            if data::set_pipe_diameter(e, u.new_diameter_ft) {
                applied += 1;
            }
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use acadrust::types::Vector3;
    use acadrust::{Circle, Line};
    use stormsewer::network::NodeKind;

    fn mk_struct(h: u64, kind: NodeKind, x: f64, invert: f64) -> EntityType {
        let mut e = EntityType::Circle(Circle {
            center: Vector3::new(x, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        e.common_mut().handle = Handle::new(h);
        e.common_mut()
            .extended_data
            .add_record(data::structure_xdata(kind, invert, invert + 6.0, 2.0, 0.7));
        e
    }

    fn mk_pipe(from: u64, to: u64, x1: f64, x2: f64, dia: f64) -> EntityType {
        let mut e = EntityType::Line(Line::from_points(
            Vector3::new(x1, 0.0, 0.0),
            Vector3::new(x2, 0.0, 0.0),
        ));
        e.common_mut().handle = Handle::new(from + 100);
        e.common_mut()
            .extended_data
            .add_record(data::pipe_xdata(dia, 0.013, Handle::new(from), Handle::new(to)));
        e
    }

    #[test]
    fn plan_size_finds_undersized_trunk() {
        let ents = vec![
            mk_struct(1, NodeKind::Inlet, 0.0, 100.0),
            mk_struct(2, NodeKind::Inlet, 100.0, 99.0),
            mk_struct(3, NodeKind::Outfall, 200.0, 98.0),
            mk_pipe(1, 2, 0.0, 100.0, 1.5),
            mk_pipe(2, 3, 100.0, 200.0, 1.5),
        ];
        let p = stormsewer::params::StormAnalysisParams::municipal();
        let (updates, report, pending) = plan_size_updates(ents.iter(), &p).expect("size");
        assert!(pending >= 1, "report:\n{report}");
        assert!(!updates.is_empty());
    }

    #[test]
    fn apply_updates_writes_xdata() {
        let mut ents = vec![
            mk_struct(1, NodeKind::Inlet, 0.0, 100.0),
            mk_struct(2, NodeKind::Outfall, 200.0, 98.0),
            mk_pipe(1, 2, 0.0, 200.0, 0.5),
        ];
        let p = stormsewer::params::StormAnalysisParams::municipal();
        let (updates, _, _) = plan_size_updates(ents.iter(), &p).expect("plan");
        let applied = apply_updates(ents.iter_mut(), &updates);
        assert!(applied >= 1);
        let drawn = data::drawn_network_from_entities(ents.iter()).unwrap();
        assert!(drawn.network.pipes[0].diameter > 0.5);
    }
}