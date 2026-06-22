//! # hydrocomplete
//!
//! Portable stormwater engine — Rust counterpart to `HydroComplete.Engine`.
//! Builds on [`stormsewer`] for network topology, Rational method, Manning
//! (circular), HGL, and IDF; adds box/arch conduits, SCS runoff, and shared
//! models used by the OpenCAD plugin and future WASM/desktop targets.

pub mod about;
pub mod atlas14_fetcher;
pub mod atlas14_presets;
pub mod arch_conduit;
pub mod box_conduit;
pub mod bmp;
pub mod bmp_optimizer;
pub mod catchment_flow_router;
pub mod channel_hydraulics;
pub mod compliance;
pub mod detention;
pub mod clark_unit_hydrograph;
pub mod culvert_hydraulics;
pub mod gradually_varied_flow;
pub mod hydrograph_convolution;
pub mod hydrograph_router;
pub mod landxml_writer;
pub mod license;
pub mod manning;
pub mod pdf_report;
pub mod models;
pub mod network_analysis;
pub mod network_diagram;
pub mod rational;
pub mod report_html;
pub mod sediment;
pub mod soil_database;
pub mod ssurgo;
pub mod state_compliance;
pub mod trace;
pub mod water_quality;
pub mod wqv;
pub mod output_paths;
pub mod pipe_cost_catalog;
pub mod pipe_plan_geometry;
pub mod prepost;
pub mod profile_dxf_writer;
pub mod pump_station;
pub mod scs_runoff;
pub mod scs_unit_hydrograph;
pub mod snyder_unit_hydrograph;
pub mod time_of_concentration;

pub use stormsewer;

#[cfg(test)]
mod engine_tests {
    use super::*;

    #[test]
    fn culvert_headwater_positive() {
        let culvert = culvert_hydraulics::CulvertParameters::default();
        let hw = culvert_hydraulics::headwater(50.0, &culvert, 0.0);
        assert!(hw.headwater_ft > 0.0);
    }

    #[test]
    fn gvf_profile_three_stations() {
        let channel = gradually_varied_flow::ChannelParameters {
            bottom_width_ft: 10.0,
            side_slope_z: 2.0,
            manning_n: 0.03,
            bed_slope_ft_per_ft: 0.001,
        };
        let stations = vec![
            gradually_varied_flow::Station { distance_ft: 0.0, invert_elev_ft: 100.0 },
            gradually_varied_flow::Station { distance_ft: 100.0, invert_elev_ft: 99.9 },
            gradually_varied_flow::Station { distance_ft: 200.0, invert_elev_ft: 99.8 },
        ];
        let result = gradually_varied_flow::compute_water_surface_profile(
            100.0, &channel, gradually_varied_flow::GvfBoundaryType::Normal, 0.0, &stations,
            gradually_varied_flow::DEFAULT_EDDY_LOSS_COEFFICIENT,
        ).unwrap();
        assert_eq!(result.profile.len(), 3);
    }

    #[test]
    fn tr55_sheet_flow_capped() {
        let a = time_of_concentration::sheet_flow(0.40, 100.0, 3.0, 0.05).tc_minutes;
        let b = time_of_concentration::sheet_flow(0.40, 200.0, 3.0, 0.05).tc_minutes;
        assert!((a - b).abs() < 1e-9);
    }

    #[test]
    fn scs_uh_peak() {
        let uh = scs_unit_hydrograph::generate(10.0, 15.0, None, None);
        assert!(uh.peak_flow_cfs > 0.0);
    }

    #[test]
    fn landxml_has_circ_pipe() {
        let pipes = vec![landxml_writer::LandXmlPipeRecord {
            name: "P1".into(), network_name: "Storm".into(), length_ft: 120.0,
            diameter_ft: 2.0, slope: 0.01, start_invert_ft: 100.0, end_invert_ft: 98.8,
            manning_n: 0.013, design_flow_cfs: None, start_structure_name: "MH-1".into(),
            end_structure_name: "OF-1".into(), shape: landxml_writer::LandXmlPipeShape::Circular,
            width_ft: 0.0, height_ft: 0.0,
        }];
        let xml = landxml_writer::write_to_string(&pipes, None, Some("Test"));
        assert!(xml.contains("CircPipe"));
    }
}