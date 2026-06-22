//! Manning hydraulics for rectangular box conduits (mirrors `BoxConduit.cs`).

use crate::models::PipeSegment;

pub const KN: f64 = 1.486;

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

pub fn partial_flow_geometry(width_ft: f64, height_ft: f64, depth_ft: f64) -> (f64, f64) {
    if width_ft <= 0.0 || height_ft <= 0.0 || depth_ft <= 0.0 {
        return (0.0, 0.0);
    }
    let depth_ft = depth_ft.min(height_ft);
    let area = width_ft * depth_ft;
    let perimeter = width_ft + 2.0 * depth_ft;
    if perimeter <= 0.0 {
        return (0.0, 0.0);
    }
    (area, area / perimeter)
}

pub fn flow_at_depth(width_ft: f64, height_ft: f64, depth_ft: f64, n: f64, slope: f64) -> f64 {
    if n <= 0.0 || slope <= 0.0 || depth_ft <= 0.0 {
        return 0.0;
    }
    let (area, r) = partial_flow_geometry(width_ft, height_ft, depth_ft);
    if area <= 0.0 || r <= 0.0 {
        return 0.0;
    }
    (KN / n) * area * r.powf(2.0 / 3.0) * slope.sqrt()
}

pub fn capacity(pipe: &PipeSegment) -> CapacityResult {
    let w = pipe.width_ft;
    let h = pipe.height_ft;
    let n = pipe.manning_n;
    let s = pipe.slope;
    let q_full = flow_at_depth(w, h, h, n, s);
    let area = w * h;
    let v_full = if area > 0.0 { q_full / area } else { 0.0 };
    CapacityResult {
        full_flow_cfs: q_full,
        full_velocity_fps: v_full,
        peak_flow_cfs: q_full,
    }
}

pub fn normal_depth(pipe: &PipeSegment, design_q_cfs: f64) -> NormalDepthResult {
    let w = pipe.width_ft;
    let h = pipe.height_ft;
    let cap = capacity(pipe);
    if design_q_cfs > cap.peak_flow_cfs {
        let area = w * h;
        return NormalDepthResult {
            depth_ft: h,
            relative_depth: 1.0,
            velocity_fps: if area > 0.0 { design_q_cfs / area } else { 0.0 },
            surcharged: true,
        };
    }
    let mut lo = 0.0;
    let mut hi = h;
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        let q = flow_at_depth(w, h, mid, pipe.manning_n, pipe.slope);
        if q < design_q_cfs {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let depth = (lo + hi) / 2.0;
    let (area, _) = partial_flow_geometry(w, h, depth);
    NormalDepthResult {
        depth_ft: depth,
        relative_depth: if h > 0.0 { depth / h } else { 0.0 },
        velocity_fps: if area > 0.0 { design_q_cfs / area } else { 0.0 },
        surcharged: false,
    }
}