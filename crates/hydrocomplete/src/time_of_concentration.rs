//! Time-of-concentration methods (Kirpich, TR-55, FAA).

use crate::channel_hydraulics;

#[derive(Debug, Clone)]
pub struct TcResult {
    pub tc_minutes: f64,
}

#[derive(Debug, Clone)]
pub struct TravelReach {
    pub name: String,
    pub length_ft: f64,
    pub velocity_fps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShallowSurfaceType {
    Unpaved,
    Paved,
}

#[derive(Debug, Clone)]
pub struct TcSegment {
    pub name: String,
    pub segment_type: String,
    pub length_ft: f64,
    pub slope: f64,
    pub manning_n: f64,
    pub rainfall_2year_in: f64,
    pub surface_type: ShallowSurfaceType,
    pub bottom_width_ft: f64,
    pub side_slope_z: f64,
    pub depth_ft: f64,
}

impl Default for TcSegment {
    fn default() -> Self {
        Self {
            name: String::new(),
            segment_type: "sheet".into(),
            length_ft: 0.0,
            slope: 0.05,
            manning_n: 0.40,
            rainfall_2year_in: 3.0,
            surface_type: ShallowSurfaceType::Unpaved,
            bottom_width_ft: 4.0,
            side_slope_z: 3.0,
            depth_ft: 1.0,
        }
    }
}

pub fn kirpich(length_ft: f64, slope: f64) -> TcResult {
    assert!(length_ft > 0.0 && slope > 0.0);
    let tc = 0.0078 * length_ft.powf(0.77) * slope.powf(-0.385);
    TcResult { tc_minutes: tc }
}

pub fn from_reaches(reaches: &[TravelReach]) -> TcResult {
    let mut total_min = 0.0;
    for reach in reaches {
        assert!(reach.length_ft >= 0.0 && reach.velocity_fps > 0.0);
        total_min += reach.length_ft / reach.velocity_fps / 60.0;
    }
    TcResult {
        tc_minutes: total_min,
    }
}

pub fn sheet_flow(manning_n: f64, length_ft: f64, rainfall_2year_in: f64, slope: f64) -> TcResult {
    assert!(manning_n > 0.0 && length_ft > 0.0 && rainfall_2year_in > 0.0 && slope > 0.0);
    let l = length_ft.min(100.0);
    let tt_hr = 0.007 * (manning_n * l).powf(0.8) / (rainfall_2year_in.sqrt() * slope.powf(0.4));
    TcResult {
        tc_minutes: tt_hr * 60.0,
    }
}

pub fn shallow_concentrated(length_ft: f64, slope: f64, surface_type: ShallowSurfaceType) -> TcResult {
    assert!(length_ft > 0.0 && slope > 0.0);
    let k = match surface_type {
        ShallowSurfaceType::Paved => 20.3282,
        ShallowSurfaceType::Unpaved => 16.1345,
    };
    let velocity = k * slope.sqrt();
    let tt_hr = length_ft / velocity / 3600.0;
    TcResult {
        tc_minutes: tt_hr * 60.0,
    }
}

pub fn from_tr55_segments(segments: &[TcSegment]) -> TcResult {
    let mut total_min = 0.0;
    for seg in segments {
        let seg_type = seg.segment_type.trim().to_ascii_lowercase();
        let minutes = if seg_type == "sheet" {
            sheet_flow(seg.manning_n, seg.length_ft, seg.rainfall_2year_in, seg.slope).tc_minutes
        } else if seg_type == "shallow" {
            shallow_concentrated(seg.length_ft, seg.slope, seg.surface_type).tc_minutes
        } else if seg_type == "channel" {
            let flow = channel_hydraulics::flow_at_depth(
                seg.bottom_width_ft,
                seg.side_slope_z,
                seg.depth_ft,
                seg.manning_n,
                seg.slope,
            );
            assert!(flow.velocity_fps > 0.0, "zero velocity in channel segment");
            seg.length_ft / flow.velocity_fps / 60.0
        } else {
            panic!("unknown segment type '{}'", seg.segment_type);
        };
        total_min += minutes;
    }
    TcResult {
        tc_minutes: total_min,
    }
}

pub fn faa(length_ft: f64, slope: f64, hydraulic_radius_ft: f64) -> TcResult {
    assert!(length_ft > 0.0 && slope > 0.0 && hydraulic_radius_ft > 0.0);
    let slope_term = (100.0 * slope).powf(1.0 / 3.0);
    let hr_term = hydraulic_radius_ft.powf(0.3);
    let tc = 1.8 * length_ft.powf(0.5) / slope_term / hr_term;
    TcResult { tc_minutes: tc }
}

/// Default TR-55 worksheet (sheet 100 ft + shallow 500 ft).
pub fn default_tr55_worksheet() -> Vec<TcSegment> {
    vec![
        TcSegment {
            name: "sheet".into(),
            segment_type: "sheet".into(),
            length_ft: 100.0,
            slope: 0.05,
            manning_n: 0.40,
            rainfall_2year_in: 3.0,
            ..Default::default()
        },
        TcSegment {
            name: "shallow".into(),
            segment_type: "shallow".into(),
            length_ft: 500.0,
            slope: 0.05,
            surface_type: ShallowSurfaceType::Unpaved,
            ..Default::default()
        },
    ]
}