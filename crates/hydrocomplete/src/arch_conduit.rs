//! Manning hydraulics for pipe-arch conduits (mirrors `ArchConduit.cs`).

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

pub fn arc_radius_ft(span_ft: f64, rise_ft: f64) -> f64 {
    span_ft * span_ft / (16.0 * rise_ft) + rise_ft / 2.0
}

fn arch_geometry(span_ft: f64, rise_ft: f64, depth_ft: f64) -> (f64, f64) {
    if span_ft <= 0.0 || rise_ft <= 0.0 || depth_ft <= 0.0 {
        return (0.0, 0.0);
    }
    let depth_ft = depth_ft.min(rise_ft);
    let r = arc_radius_ft(span_ft, rise_ft);
    let y_s = r - (r * r - (span_ft / 2.0).powi(2)).max(0.0).sqrt();

    let (area, perim) = if depth_ft <= y_s {
        let theta = 2.0 * ((r - depth_ft) / r).acos();
        let a = r * r / 2.0 * (theta - theta.sin());
        let p = r * theta;
        (a, p)
    } else {
        let theta = 2.0 * ((r - y_s) / r).acos();
        let arc_area = r * r / 2.0 * (theta - theta.sin());
        let arc_perim = r * theta;
        let wall_h = depth_ft - y_s;
        let wall_area = span_ft * wall_h;
        let wall_perim = 2.0 * wall_h;
        (arc_area + wall_area, arc_perim + wall_perim)
    };

    if perim <= 0.0 {
        (0.0, 0.0)
    } else {
        (area, area / perim)
    }
}

pub fn flow_at_depth(span_ft: f64, rise_ft: f64, depth_ft: f64, n: f64, slope: f64) -> f64 {
    if n <= 0.0 || slope <= 0.0 {
        return 0.0;
    }
    let (area, hyd_r) = arch_geometry(span_ft, rise_ft, depth_ft);
    if area <= 0.0 || hyd_r <= 0.0 {
        return 0.0;
    }
    (KN / n) * area * hyd_r.powf(2.0 / 3.0) * slope.sqrt()
}

pub fn capacity(pipe: &PipeSegment) -> CapacityResult {
    let b = pipe.effective_span_ft();
    let h = pipe.effective_rise_ft();
    let n = pipe.manning_n;
    let s = pipe.slope;
    let mut peak_q = 0.0;
    let steps = 200;
    for i in 1..=steps {
        let y = h * i as f64 / steps as f64;
        let q = flow_at_depth(b, h, y, n, s);
        if q > peak_q {
            peak_q = q;
        }
    }
    let q_full = flow_at_depth(b, h, h, n, s);
    let (area, _) = arch_geometry(b, h, h);
    let v_full = if area > 0.0 { q_full / area } else { 0.0 };
    CapacityResult {
        full_flow_cfs: q_full,
        full_velocity_fps: v_full,
        peak_flow_cfs: peak_q.max(q_full),
    }
}

pub fn normal_depth(pipe: &PipeSegment, design_q_cfs: f64) -> NormalDepthResult {
    let b = pipe.effective_span_ft();
    let h = pipe.effective_rise_ft();
    let cap = capacity(pipe);
    if design_q_cfs > cap.peak_flow_cfs {
        let (area, _) = arch_geometry(b, h, h);
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
        let q = flow_at_depth(b, h, mid, pipe.manning_n, pipe.slope);
        if q < design_q_cfs {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let depth = (lo + hi) / 2.0;
    let (area, _) = arch_geometry(b, h, depth);
    NormalDepthResult {
        depth_ft: depth,
        relative_depth: if h > 0.0 { depth / h } else { 0.0 },
        velocity_fps: if area > 0.0 { design_q_cfs / area } else { 0.0 },
        surcharged: false,
    }
}