// SPDX-License-Identifier: GPL-3.0-only

//! Open-channel & partial-flow hydraulics for circular closed conduits.
//!
//! US customary units throughout (feet, seconds, cfs). Pass `K_MANNING_SI`
//! and `G_SI` for metric. All geometry is exact (no table lookups).

use std::f64::consts::PI;

/// Manning conversion factor, US customary (1.486 rounded to 1.49 by convention).
pub const K_MANNING_US: f64 = 1.49;
/// Manning conversion factor, SI.
pub const K_MANNING_SI: f64 = 1.0;
/// Gravitational acceleration, US customary (ft/s^2).
pub const G_US: f64 = 32.2;
/// Gravitational acceleration, SI (m/s^2).
pub const G_SI: f64 = 9.81;

/// Central angle (radians) subtended at the pipe centre by the water surface,
/// for flow depth `y` in a circular pipe of diameter `d`.
///
/// `0` when empty, `2*PI` when full.
pub fn circular_angle(y: f64, d: f64) -> f64 {
    let y = y.clamp(0.0, d);
    2.0 * (1.0 - 2.0 * y / d).acos()
}

/// Flow geometry at depth `y` in a circular pipe of diameter `d`.
///
/// Returns `(area, wetted_perimeter, hydraulic_radius, top_width)`.
pub fn circular_geometry(y: f64, d: f64) -> (f64, f64, f64, f64) {
    let theta = circular_angle(y, d);
    let area = d * d / 8.0 * (theta - theta.sin());
    let perim = d * theta / 2.0;
    let radius = if perim > 0.0 { area / perim } else { 0.0 };
    let top = d * (theta / 2.0).sin();
    (area, perim, radius, top)
}

/// Full cross-sectional area of a circular pipe.
pub fn full_area(d: f64) -> f64 {
    PI * d * d / 4.0
}

/// Manning discharge from flow area and hydraulic radius.
///
/// `Q = (k/n) * A * R^(2/3) * S^(1/2)`
pub fn manning_q(n: f64, s: f64, area: f64, radius: f64, k: f64) -> f64 {
    if n <= 0.0 {
        return 0.0;
    }
    k / n * area * radius.powf(2.0 / 3.0) * s.max(0.0).sqrt()
}

/// Just-full (geometrically full, free-surface) capacity of a circular pipe.
///
/// Note: the *maximum* open-channel discharge actually occurs near `0.94 d`
/// and slightly exceeds this value — see [`max_capacity`].
pub fn full_flow_capacity(n: f64, s: f64, d: f64, k: f64) -> f64 {
    manning_q(n, s, full_area(d), d / 4.0, k)
}

/// Discharge at partial depth `y` in a circular pipe.
pub fn circular_q(n: f64, s: f64, d: f64, y: f64, k: f64) -> f64 {
    let (area, _p, radius, _t) = circular_geometry(y, d);
    manning_q(n, s, area, radius, k)
}

/// Maximum open-channel discharge of a circular pipe and the depth at which it
/// occurs (~`0.938 d`). Returns `(q_max, y_at_max)`.
pub fn max_capacity(n: f64, s: f64, d: f64, k: f64) -> (f64, f64) {
    // Q(y) is smooth with a single interior maximum; a fine scan is exact
    // enough and cheap for the depths involved.
    let steps = 2000;
    let mut best_q = 0.0;
    let mut best_y = 0.0;
    for i in 1..=steps {
        let y = d * i as f64 / steps as f64;
        let q = circular_q(n, s, d, y, k);
        if q > best_q {
            best_q = q;
            best_y = y;
        }
    }
    (best_q, best_y)
}

/// Normal (uniform-flow) depth for a target discharge in a circular pipe.
///
/// Returns `None` when `q_target` exceeds the pipe's maximum open-channel
/// capacity — i.e. the pipe must surcharge / flow under pressure.
pub fn normal_depth(q_target: f64, n: f64, s: f64, d: f64, k: f64) -> Option<f64> {
    if q_target <= 0.0 {
        return Some(0.0);
    }
    let (q_max, y_max) = max_capacity(n, s, d, k);
    if q_target > q_max {
        return None;
    }
    // Q is monotonic increasing on (0, y_max]; bisect there.
    let (mut lo, mut hi) = (0.0_f64, y_max);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if circular_q(n, s, d, mid, k) > q_target {
            hi = mid;
        } else {
            lo = mid;
        }
        if hi - lo < 1e-7 {
            break;
        }
    }
    Some(0.5 * (lo + hi))
}

/// Critical depth for a target discharge in a circular pipe.
///
/// Solves `Q^2 * T = g * A^3` (Froude = 1). Valid for `0 < y < d`.
pub fn critical_depth(q: f64, d: f64, g: f64) -> f64 {
    if q <= 0.0 {
        return 0.0;
    }
    let resid = |y: f64| {
        let (area, _p, _r, top) = circular_geometry(y, d);
        if area <= 0.0 || top <= 0.0 {
            return -q * q;
        }
        q * q * top - g * area * area * area
    };
    // resid > 0 for small y, < 0 as y grows: bracket the sign change.
    let (mut lo, mut hi) = (1e-6 * d, 0.999 * d);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if resid(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-7 {
            break;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: f64 = K_MANNING_US;

    #[test]
    fn full_area_matches_geometry() {
        // Geometry at y = d must equal the closed-form full area.
        let d = 2.0;
        let (area, perim, r, _t) = circular_geometry(d, d);
        assert!((area - full_area(d)).abs() < 1e-9);
        assert!((perim - PI * d).abs() < 1e-9); // full wetted perimeter = pi*d
        assert!((r - d / 4.0).abs() < 1e-9); // hydraulic radius full = d/4
    }

    #[test]
    fn half_depth_is_half_area() {
        // A circle is exactly half-full at half-diameter depth.
        let d = 3.0;
        let (area, _p, _r, _t) = circular_geometry(d / 2.0, d);
        assert!((area - full_area(d) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn manning_capacity_known_value() {
        // 2-ft pipe, n=0.013, s=0.005  ->  ~16.0 cfs full-flow (hand calc).
        let q = full_flow_capacity(0.013, 0.005, 2.0, K);
        assert!((q - 16.0).abs() < 0.2, "got {q}");
    }

    #[test]
    fn normal_depth_round_trip() {
        // Build a discharge from a depth, then recover the depth.
        let (n, s, d) = (0.013, 0.01, 2.0);
        let y0 = 1.2;
        let q = circular_q(n, s, d, y0, K);
        let y = normal_depth(q, n, s, d, K).expect("below capacity");
        assert!((y - y0).abs() < 1e-3, "got {y}");
    }

    #[test]
    fn surcharge_returns_none() {
        // A tiny pipe cannot pass a huge flow as open channel.
        let y = normal_depth(500.0, 0.013, 0.005, 1.0, K);
        assert!(y.is_none());
    }

    #[test]
    fn max_capacity_exceeds_full() {
        // Peak open-channel flow occurs near 0.94 d and beats just-full.
        let (qmax, ymax) = max_capacity(0.013, 0.005, 2.0, K);
        let qfull = full_flow_capacity(0.013, 0.005, 2.0, K);
        assert!(qmax > qfull);
        assert!(ymax > 1.8 && ymax < 2.0, "y_at_max = {ymax}");
    }

    #[test]
    fn critical_depth_in_range() {
        let yc = critical_depth(10.0, 2.0, G_US);
        assert!(yc > 0.0 && yc < 2.0, "yc = {yc}");
    }
}
