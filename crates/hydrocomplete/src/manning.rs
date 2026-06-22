//! Shape-dispatching Manning capacity (mirrors `Manning.cs`).

use crate::arch_conduit;
use crate::box_conduit;
use crate::models::{PipeSegment, PipeShape};
use stormsewer::hydraulics::{circular_q, full_flow_capacity, max_capacity, K_MANNING_US};

#[derive(Debug, Clone)]
pub struct CapacityResult {
    pub full_flow_cfs: f64,
    pub full_velocity_fps: f64,
    pub peak_flow_cfs: f64,
}

#[derive(Debug, Clone)]
pub struct NormalDepthResult {
    pub depth_ft: f64,
    pub relative_depth: f64,
    pub velocity_fps: f64,
    pub surcharged: bool,
}

pub fn capacity(pipe: &PipeSegment) -> CapacityResult {
    match pipe.shape {
        PipeShape::Box => {
            let c = box_conduit::capacity(pipe);
            CapacityResult {
                full_flow_cfs: c.full_flow_cfs,
                full_velocity_fps: c.full_velocity_fps,
                peak_flow_cfs: c.peak_flow_cfs,
            }
        }
        PipeShape::Arch => {
            let c = arch_conduit::capacity(pipe);
            CapacityResult {
                full_flow_cfs: c.full_flow_cfs,
                full_velocity_fps: c.full_velocity_fps,
                peak_flow_cfs: c.peak_flow_cfs,
            }
        }
        PipeShape::Circular => capacity_circular(pipe),
    }
}

fn capacity_circular(pipe: &PipeSegment) -> CapacityResult {
    let d = pipe.diameter_ft;
    let n = pipe.manning_n;
    let s = pipe.slope;
    let q_full = full_flow_capacity(n, s, d, K_MANNING_US);
    let (q_peak, y_peak) = max_capacity(n, s, d, K_MANNING_US);
    let area = std::f64::consts::PI * d * d / 4.0;
    let v_full = if area > 0.0 { q_full / area } else { 0.0 };
    let _ = y_peak;
    CapacityResult {
        full_flow_cfs: q_full,
        full_velocity_fps: v_full,
        peak_flow_cfs: q_peak,
    }
}

pub fn normal_depth(pipe: &PipeSegment, design_q_cfs: f64) -> NormalDepthResult {
    match pipe.shape {
        PipeShape::Box => {
            let r = box_conduit::normal_depth(pipe, design_q_cfs);
            NormalDepthResult {
                depth_ft: r.depth_ft,
                relative_depth: r.relative_depth,
                velocity_fps: r.velocity_fps,
                surcharged: r.surcharged,
            }
        }
        PipeShape::Arch => {
            let r = arch_conduit::normal_depth(pipe, design_q_cfs);
            NormalDepthResult {
                depth_ft: r.depth_ft,
                relative_depth: r.relative_depth,
                velocity_fps: r.velocity_fps,
                surcharged: r.surcharged,
            }
        }
        PipeShape::Circular => normal_depth_circular(pipe, design_q_cfs),
    }
}

fn normal_depth_circular(pipe: &PipeSegment, design_q_cfs: f64) -> NormalDepthResult {
    let d = pipe.diameter_ft;
    let n = pipe.manning_n;
    let s = pipe.slope;
    let (q_peak, _) = max_capacity(n, s, d, K_MANNING_US);
    if design_q_cfs > q_peak {
        let area = std::f64::consts::PI * d * d / 4.0;
        return NormalDepthResult {
            depth_ft: d,
            relative_depth: 1.0,
            velocity_fps: if area > 0.0 { design_q_cfs / area } else { 0.0 },
            surcharged: true,
        };
    }
    let mut lo = 0.0;
    let mut hi = d;
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        let q = circular_q(n, s, d, mid, K_MANNING_US);
        if q < design_q_cfs {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let depth = (lo + hi) / 2.0;
    let area = stormsewer::hydraulics::circular_geometry(depth, d).0;
    NormalDepthResult {
        depth_ft: depth,
        relative_depth: if d > 0.0 { depth / d } else { 0.0 },
        velocity_fps: if area > 0.0 { design_q_cfs / area } else { 0.0 },
        surcharged: false,
    }
}