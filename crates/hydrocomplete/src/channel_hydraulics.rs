//! Trapezoidal open-channel hydraulics (US customary).

pub const KN: f64 = 1.486;
pub const G: f64 = 32.174;

#[derive(Debug, Clone)]
pub struct GeometryResult {
    pub depth_ft: f64,
    pub area_ft2: f64,
    pub wetted_perimeter_ft: f64,
    pub hyd_radius_ft: f64,
    pub top_width_ft: f64,
}

#[derive(Debug, Clone)]
pub struct FlowResult {
    pub flow_cfs: f64,
    pub velocity_fps: f64,
    pub geometry: GeometryResult,
}

#[derive(Debug, Clone)]
pub struct NormalDepthResult {
    pub depth_ft: f64,
    pub flow_cfs: f64,
    pub velocity_fps: f64,
    pub froude_number: f64,
    pub flow_regime: String,
    pub geometry: GeometryResult,
}

#[derive(Debug, Clone)]
pub struct CriticalDepthResult {
    pub depth_ft: f64,
    pub velocity_fps: f64,
    pub froude_number: f64,
    pub geometry: GeometryResult,
}

pub fn trapezoidal_geometry(bottom_width_ft: f64, side_slope_z: f64, depth_ft: f64) -> GeometryResult {
    let y = depth_ft;
    let area = (bottom_width_ft + side_slope_z * y) * y;
    let perimeter = bottom_width_ft + 2.0 * y * (1.0 + side_slope_z * side_slope_z).sqrt();
    let top_width = bottom_width_ft + 2.0 * side_slope_z * y;
    let hyd_radius = if perimeter > 0.0 { area / perimeter } else { 0.0 };
    GeometryResult {
        depth_ft: y,
        area_ft2: area,
        wetted_perimeter_ft: perimeter,
        hyd_radius_ft: hyd_radius,
        top_width_ft: top_width,
    }
}

pub fn flow_at_depth(
    bottom_width_ft: f64,
    side_slope_z: f64,
    depth_ft: f64,
    manning_n: f64,
    slope_ft_per_ft: f64,
) -> FlowResult {
    validate_manning(bottom_width_ft, side_slope_z, depth_ft, manning_n, slope_ft_per_ft);
    let geom = trapezoidal_geometry(bottom_width_ft, side_slope_z, depth_ft);
    let (q, v) = if geom.area_ft2 > 0.0 && geom.hyd_radius_ft > 0.0 {
        let q = (KN / manning_n)
            * geom.area_ft2
            * geom.hyd_radius_ft.powf(2.0 / 3.0)
            * slope_ft_per_ft.sqrt();
        (q, q / geom.area_ft2)
    } else {
        (0.0, 0.0)
    };
    FlowResult {
        flow_cfs: q,
        velocity_fps: v,
        geometry: geom,
    }
}

pub fn normal_depth(
    bottom_width_ft: f64,
    side_slope_z: f64,
    manning_n: f64,
    slope_ft_per_ft: f64,
    target_flow_cfs: f64,
) -> NormalDepthResult {
    assert!(target_flow_cfs >= 0.0);
    validate_manning(bottom_width_ft, side_slope_z, 0.001, manning_n, slope_ft_per_ft);

    let mut y_lo = 0.0001;
    let mut y_hi = 100.0;
    while flow_at_depth(bottom_width_ft, side_slope_z, y_hi, manning_n, slope_ft_per_ft).flow_cfs
        < target_flow_cfs
        && y_hi < 500.0
    {
        y_hi *= 2.0;
    }
    for _ in 0..60 {
        let y_mid = 0.5 * (y_lo + y_hi);
        let q_mid = flow_at_depth(bottom_width_ft, side_slope_z, y_mid, manning_n, slope_ft_per_ft).flow_cfs;
        if q_mid > target_flow_cfs {
            y_hi = y_mid;
        } else {
            y_lo = y_mid;
        }
    }
    let yn = 0.5 * (y_lo + y_hi);
    let flow = flow_at_depth(bottom_width_ft, side_slope_z, yn, manning_n, slope_ft_per_ft);
    let hydraulic_depth = if flow.geometry.top_width_ft > 0.0 {
        flow.geometry.area_ft2 / flow.geometry.top_width_ft
    } else {
        0.0
    };
    let fr = if hydraulic_depth > 0.0 {
        flow.velocity_fps / (G * hydraulic_depth).sqrt()
    } else {
        0.0
    };
    let regime = if fr < 1.0 {
        "subcritical"
    } else if fr > 1.0 {
        "supercritical"
    } else {
        "critical"
    }
    .to_string();
    NormalDepthResult {
        depth_ft: yn,
        flow_cfs: flow.flow_cfs,
        velocity_fps: flow.velocity_fps,
        froude_number: fr,
        flow_regime: regime,
        geometry: flow.geometry,
    }
}

pub fn critical_depth(bottom_width_ft: f64, side_slope_z: f64, flow_cfs: f64) -> CriticalDepthResult {
    assert!(bottom_width_ft >= 0.0 && side_slope_z >= 0.0 && flow_cfs >= 0.0);
    let target = flow_cfs * flow_cfs / G;
    let mut y_lo = 0.0001;
    let mut y_hi = 50.0;
    while critical_function(bottom_width_ft, side_slope_z, y_hi) < target && y_hi < 500.0 {
        y_hi *= 2.0;
    }
    for _ in 0..100 {
        let y_mid = 0.5 * (y_lo + y_hi);
        if critical_function(bottom_width_ft, side_slope_z, y_mid) > target {
            y_hi = y_mid;
        } else {
            y_lo = y_mid;
        }
    }
    let yc = 0.5 * (y_lo + y_hi);
    let geom = trapezoidal_geometry(bottom_width_ft, side_slope_z, yc);
    let v = if geom.area_ft2 > 0.0 {
        flow_cfs / geom.area_ft2
    } else {
        0.0
    };
    let hydraulic_depth = if geom.top_width_ft > 0.0 {
        geom.area_ft2 / geom.top_width_ft
    } else {
        0.0
    };
    let fr = if hydraulic_depth > 0.0 {
        v / (G * hydraulic_depth).sqrt()
    } else {
        0.0
    };
    CriticalDepthResult {
        depth_ft: yc,
        velocity_fps: v,
        froude_number: fr,
        geometry: geom,
    }
}

fn critical_function(b: f64, z: f64, y: f64) -> f64 {
    let geom = trapezoidal_geometry(b, z, y);
    if geom.top_width_ft <= 0.0 {
        return 0.0;
    }
    geom.area_ft2.powi(3) / geom.top_width_ft
}

fn validate_manning(b: f64, z: f64, depth: f64, n: f64, s: f64) {
    assert!(b >= 0.0 && z >= 0.0 && depth > 0.0 && n > 0.0 && s >= 0.0);
}