//! Headless integration tests — XDATA round-trip and analysis consistency.

#[cfg(test)]
mod tests {
    use acadrust::types::Vector3;
    use acadrust::xdata::XDataValue;
    use acadrust::{Circle, EntityType, Handle, Line};
    use stormsewer::network::NodeKind;
    use stormsewer::params::StormAnalysisParams;

    use crate::analysis;
    use crate::data::{apply_tc_map, pipe_xdata, structure_xdata};

    fn drawn_inlet_outfall_pipe() -> Vec<EntityType> {
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

        vec![s1, s2, p]
    }

    #[test]
    fn xdata_roundtrip_and_analyze_consistency() {
        let ents = drawn_inlet_outfall_pipe();
        let params = StormAnalysisParams::municipal();
        let (_annots, report, _a) =
            analysis::analyze_doc(ents.iter(), &params).expect("analyze from XDATA ents");
        assert!(!report.is_empty());
        assert!(
            report.contains("HydroComplete") || report.contains("Q") || report.contains("flow"),
            "report:\n{report}"
        );
        let net = crate::data::network_from_entities(ents.iter()).expect("re-parse net");
        assert_eq!(net.nodes.len(), 2);
        assert_eq!(net.pipes.len(), 1);
    }

    #[test]
    fn design_review_flags_low_cover_end_to_end() {
        // Inlet rim only 1 ft above invert with a 1.5 ft pipe -> negative cover.
        let mut s1 = EntityType::Circle(Circle {
            center: Vector3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s1.common_mut().handle = Handle::new(1);
        s1.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Inlet, 100.0, 101.0, 1.0, 0.7));

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

        let ents = vec![s1, s2, p];
        let params = StormAnalysisParams::municipal();
        let findings =
            analysis::design_review_doc(ents.iter(), &params).expect("design review runs");
        assert!(
            findings.iter().any(|f| f.message.contains("cover")),
            "expected a cover finding, got: {findings:?}"
        );
    }

    #[test]
    fn apply_tc_map_updates_structure_xdata() {
        let mut ents = drawn_inlet_outfall_pipe();
        let mut tc = std::collections::HashMap::new();
        tc.insert(Handle::new(1), 15.5);
        let updated = apply_tc_map(ents.iter_mut(), &tc);
        assert_eq!(updated, 1);
        let rec = ents[0]
            .common()
            .extended_data
            .get_record(crate::data::APP_STRUCT)
            .expect("struct xdata");
        let tc_val = match rec.values.last() {
            Some(XDataValue::Real(v)) => *v,
            _ => 0.0,
        };
        assert!((tc_val - 15.5).abs() < 1e-6, "tc={tc_val}");
    }
}