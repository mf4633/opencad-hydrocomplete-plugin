//! Gradually varied flow — Standard Step Method for trapezoidal channels.

use crate::channel_hydraulics::{self, G};

pub const DEFAULT_EDDY_LOSS_COEFFICIENT: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GvfBoundaryType {
    Normal,
    Critical,
    Known,
}

#[derive(Debug, Clone)]
pub struct ChannelParameters {
    pub bottom_width_ft: f64,
    pub side_slope_z: f64,
    pub manning_n: f64,
    pub bed_slope_ft_per_ft: f64,
}

#[derive(Debug, Clone)]
pub struct Station {
    pub distance_ft: f64,
    pub invert_elev_ft: f64,
}

#[derive(Debug, Clone)]
pub struct ProfilePoint {
    pub station_ft: f64,
    pub invert_elev_ft: f64,
    pub depth_ft: f64,
    pub water_surface_elev_ft: f64,
    pub velocity_fps: f64,
    pub froude_number: f64,
    pub flow_regime: String,
    pub friction_slope: f64,
}

#[derive(Debug, Clone)]
pub struct ProfileResult {
    pub profile: Vec<ProfilePoint>,
    pub profile_type: String,
    pub normal_depth_ft: f64,
    pub critical_depth_ft: f64,
    pub is_subcritical: bool,
    pub boundary_depth_ft: f64,
    pub boundary_type: GvfBoundaryType,
}

pub fn specific_energy(depth_ft: f64, velocity_fps: f64) -> f64 {
    depth_ft + velocity_fps * velocity_fps / (2.0 * G)
}

pub fn froude_number(velocity_fps: f64, hydraulic_depth_ft: f64) -> f64 {
    if hydraulic_depth_ft <= 0.0 {
        return f64::INFINITY;
    }
    velocity_fps / (G * hydraulic_depth_ft).sqrt()
}

pub fn manning_friction_slope(
    flow_cfs: f64,
    bottom_width_ft: f64,
    side_slope_z: f64,
    manning_n: f64,
    depth_ft: f64,
) -> f64 {
    let geom = channel_hydraulics::trapezoidal_geometry(bottom_width_ft, side_slope_z, depth_ft);
    if geom.area_ft2 <= 0.0 || geom.hyd_radius_ft <= 0.0 {
        return 0.0;
    }
    let v = flow_cfs / geom.area_ft2;
    let ratio = v * manning_n / (channel_hydraulics::KN * geom.hyd_radius_ft.powf(2.0 / 3.0));
    ratio * ratio
}

pub fn compute_water_surface_profile(
    flow_cfs: f64,
    channel: &ChannelParameters,
    boundary_type: GvfBoundaryType,
    known_boundary_depth_ft: f64,
    stations: &[Station],
    eddy_loss_coefficient: f64,
) -> Result<ProfileResult, String> {
    if flow_cfs < 0.0 {
        return Err("flow must be >= 0".into());
    }
    if stations.is_empty() {
        return Err("at least one station is required".into());
    }
    let b = channel.bottom_width_ft;
    let z = channel.side_slope_z;
    let n = channel.manning_n;
    let s0 = channel.bed_slope_ft_per_ft;
    if b < 0.0 || z < 0.0 || n <= 0.0 || s0 < 0.0 {
        return Err("invalid channel parameters".into());
    }

    let start_depth = resolve_boundary_depth(flow_cfs, channel, boundary_type, known_boundary_depth_ft)?;
    let yn = channel_hydraulics::normal_depth(b, z, n, s0, flow_cfs).depth_ft;
    let yc = channel_hydraulics::critical_depth(b, z, flow_cfs).depth_ft;
    let is_subcritical = start_depth > yc;

    let mut sorted: Vec<Station> = stations.to_vec();
    if is_subcritical {
        sorted.sort_by(|a, b| b.distance_ft.partial_cmp(&a.distance_ft).unwrap());
    } else {
        sorted.sort_by(|a, b| a.distance_ft.partial_cmp(&b.distance_ft).unwrap());
    }

    let mut profile = Vec::new();
    let mut prev_depth = start_depth;

    for (i, stn) in sorted.iter().enumerate() {
        if i == 0 {
            let geom = channel_hydraulics::trapezoidal_geometry(b, z, start_depth);
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
            profile.push(ProfilePoint {
                station_ft: stn.distance_ft,
                invert_elev_ft: stn.invert_elev_ft,
                depth_ft: start_depth,
                water_surface_elev_ft: stn.invert_elev_ft + start_depth,
                velocity_fps: v,
                froude_number: froude_number(v, hydraulic_depth),
                flow_regime: flow_regime_label(froude_number(v, hydraulic_depth)),
                friction_slope: manning_friction_slope(flow_cfs, b, z, n, start_depth),
            });
            continue;
        }

        let prev_stn = &sorted[i - 1];
        let d_l = (stn.distance_ft - prev_stn.distance_ft).abs();
        let mut y2 = prev_depth;

        for _ in 0..50 {
            let geom1 = channel_hydraulics::trapezoidal_geometry(b, z, prev_depth);
            let v1 = if geom1.area_ft2 > 0.0 {
                flow_cfs / geom1.area_ft2
            } else {
                0.0
            };
            let geom2 = channel_hydraulics::trapezoidal_geometry(b, z, y2);
            let v2 = if geom2.area_ft2 > 0.0 {
                flow_cfs / geom2.area_ft2
            } else {
                0.0
            };
            let sf1 = manning_friction_slope(flow_cfs, b, z, n, prev_depth);
            let sf2 = manning_friction_slope(flow_cfs, b, z, n, y2);
            let sf_avg = 0.5 * (sf1 + sf2);
            let e1 = prev_stn.invert_elev_ft + prev_depth + v1 * v1 / (2.0 * G);
            let e2 = stn.invert_elev_ft + y2 + v2 * v2 / (2.0 * G);
            let hf = sf_avg * d_l;
            let he = eddy_loss_coefficient * (v1 * v1 / (2.0 * G) - v2 * v2 / (2.0 * G)).abs();
            let residual = e1 + hf + he - e2;
            if residual.abs() < 1e-6 {
                break;
            }
            let dy = 0.0001;
            let y2p = y2 + dy;
            let geom2p = channel_hydraulics::trapezoidal_geometry(b, z, y2p);
            let v2p = if geom2p.area_ft2 > 0.0 {
                flow_cfs / geom2p.area_ft2
            } else {
                0.0
            };
            let sf2p = manning_friction_slope(flow_cfs, b, z, n, y2p);
            let sf_avg_p = 0.5 * (sf1 + sf2p);
            let e2p = stn.invert_elev_ft + y2p + v2p * v2p / (2.0 * G);
            let hf_p = sf_avg_p * d_l;
            let he_p = eddy_loss_coefficient * (v1 * v1 / (2.0 * G) - v2p * v2p / (2.0 * G)).abs();
            let residual_p = e1 + hf_p + he_p - e2p;
            let d_rdy = (residual_p - residual) / dy;
            if d_rdy.abs() < 1e-12 {
                break;
            }
            y2 -= residual / d_rdy;
            y2 = y2.max(0.01);
        }

        let geom = channel_hydraulics::trapezoidal_geometry(b, z, y2);
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
        let fr = froude_number(v, hydraulic_depth);
        profile.push(ProfilePoint {
            station_ft: stn.distance_ft,
            invert_elev_ft: stn.invert_elev_ft,
            depth_ft: y2,
            water_surface_elev_ft: stn.invert_elev_ft + y2,
            velocity_fps: v,
            froude_number: fr,
            flow_regime: flow_regime_label(fr),
            friction_slope: manning_friction_slope(flow_cfs, b, z, n, y2),
        });
        prev_depth = y2;
    }

    profile.sort_by(|a, b| a.station_ft.partial_cmp(&b.station_ft).unwrap());
    let profile_type = classify_profile(s0, yn, yc, start_depth);
    Ok(ProfileResult {
        profile,
        profile_type,
        normal_depth_ft: yn,
        critical_depth_ft: yc,
        is_subcritical,
        boundary_depth_ft: start_depth,
        boundary_type,
    })
}

fn resolve_boundary_depth(
    flow_cfs: f64,
    channel: &ChannelParameters,
    boundary_type: GvfBoundaryType,
    known_boundary_depth_ft: f64,
) -> Result<f64, String> {
    match boundary_type {
        GvfBoundaryType::Normal => Ok(
            channel_hydraulics::normal_depth(
                channel.bottom_width_ft,
                channel.side_slope_z,
                channel.manning_n,
                channel.bed_slope_ft_per_ft,
                flow_cfs,
            )
            .depth_ft,
        ),
        GvfBoundaryType::Critical => Ok(channel_hydraulics::critical_depth(
            channel.bottom_width_ft,
            channel.side_slope_z,
            flow_cfs,
        )
        .depth_ft),
        GvfBoundaryType::Known => {
            if known_boundary_depth_ft <= 0.0 {
                Err("known boundary depth must be > 0".into())
            } else {
                Ok(known_boundary_depth_ft)
            }
        }
    }
}

fn classify_profile(s0: f64, yn: f64, yc: f64, start_depth: f64) -> String {
    if s0 > 0.0 && yn > yc {
        if start_depth > yn {
            return "M1 (backwater)".into();
        }
        if start_depth < yn && start_depth > yc {
            return "M2 (drawdown)".into();
        }
        return "M3 (supercritical on mild slope)".into();
    }
    if s0 > 0.0 && yn < yc {
        if start_depth > yc {
            return "S1 (subcritical on steep slope)".into();
        }
        if start_depth < yc && start_depth > yn {
            return "S2 (drawdown)".into();
        }
        return "S3 (supercritical below normal)".into();
    }
    "Computed profile".into()
}

fn flow_regime_label(fr: f64) -> String {
    if fr < 1.0 {
        "subcritical".into()
    } else if fr > 1.0 {
        "supercritical".into()
    } else {
        "critical".into()
    }
}