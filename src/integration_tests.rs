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
    fn two_pipe_diameters_reconstruct_correctly() {
        let mut s1 = EntityType::Circle(Circle {
            center: Vector3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s1.common_mut().handle = Handle::new(43);
        s1.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Inlet, 100.0, 106.0, 2.0, 0.75));

        let mut s2 = EntityType::Circle(Circle {
            center: Vector3::new(50.0, 0.0, 0.0),
            radius: 4.0,
            ..Default::default()
        });
        s2.common_mut().handle = Handle::new(44);
        s2.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Junction, 99.5, 105.5, 0.0, 0.0));

        let mut s3 = EntityType::Circle(Circle {
            center: Vector3::new(100.0, 0.0, 0.0),
            radius: 6.0,
            ..Default::default()
        });
        s3.common_mut().handle = Handle::new(45);
        s3.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Outfall, 99.0, 105.0, 0.0, 0.0));

        let mut p1 = EntityType::Line(Line::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(50.0, 0.0, 0.0),
        ));
        p1.common_mut().handle = Handle::new(46);
        p1.common_mut()
            .extended_data
            .add_record(pipe_xdata(1.25, 0.013, Handle::new(43), Handle::new(44)));

        let mut p2 = EntityType::Line(Line::from_points(
            Vector3::new(50.0, 0.0, 0.0),
            Vector3::new(100.0, 0.0, 0.0),
        ));
        p2.common_mut().handle = Handle::new(47);
        p2.common_mut()
            .extended_data
            .add_record(pipe_xdata(1.50, 0.013, Handle::new(44), Handle::new(45)));

        let ents = vec![s1, s2, s3, p1, p2];
        let net = crate::data::network_from_entities(ents.iter()).expect("network");
        assert_eq!(net.pipes.len(), 2);
        assert!((net.pipes[0].diameter - 1.25).abs() < 1e-6, "P1 dia {}", net.pipes[0].diameter);
        assert!((net.pipes[1].diameter - 1.50).abs() < 1e-6);
        let params = StormAnalysisParams::municipal();
        let a = analysis::run_analysis_on_network(&net, &params).unwrap();
        assert!(a.pipes[0].capacity > 0.0);
        assert!((a.pipes[0].slope - 0.01).abs() < 1e-4, "P1 slope {}", a.pipes[0].slope);
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

    #[test]
    fn drawing_params_marker_roundtrip() {
        use acadrust::MText;
        use acadrust::xdata::{ExtendedDataRecord, XDataValue};

        use crate::drawing_params::{self, APP_PARAMS};

        let mut params = StormAnalysisParams::municipal();
        let preset = hydrocomplete::atlas14_presets::find("charlotte-nc").unwrap();
        let curve = preset.to_curve(10).unwrap();
        params.idf.set_curve(10, curve);
        params.idf.set_design_rp(10);

        let blob = drawing_params::ParamsBlob::from_params(&params, Some("charlotte-nc"));
        let json = serde_json::to_string(&blob).unwrap();
        let mut marker = EntityType::MText(MText {
            value: "HC".into(),
            ..Default::default()
        });
        let mut rec = ExtendedDataRecord::new(APP_PARAMS);
        rec.add_value(XDataValue::String(json));
        marker.common_mut().extended_data.add_record(rec);

        let restored = drawing_params::read_params_from_entities([&marker].into_iter())
            .expect("read params marker");
        assert!((restored.idf.design_curve().a - 81.2).abs() < 0.1);
    }
}