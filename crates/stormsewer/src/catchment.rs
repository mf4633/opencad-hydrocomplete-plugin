// SPDX-License-Identifier: GPL-3.0-only

//! Catchment polygon geometry helpers (CAD-agnostic).

use crate::hydrology::kirpich_minutes;

/// Shoelace formula area for a closed polygon (square feet).
pub fn shoelace_area_sqft(vertices: &[(f64, f64)]) -> f64 {
    let n = vertices.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let (x0, y0) = vertices[i];
        let (x1, y1) = vertices[(i + 1) % n];
        sum += x0 * y1 - x1 * y0;
    }
    (sum / 2.0).abs()
}

/// Plan centroid of a closed polygon.
pub fn polygon_centroid(vertices: &[(f64, f64)]) -> (f64, f64) {
    let n = vertices.len();
    if n < 3 {
        let sx: f64 = vertices.iter().map(|v| v.0).sum();
        let sy: f64 = vertices.iter().map(|v| v.1).sum();
        let d = n.max(1) as f64;
        return (sx / d, sy / d);
    }
    let mut a2 = 0.0;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..n {
        let (x0, y0) = vertices[i];
        let (x1, y1) = vertices[(i + 1) % n];
        let cross = x0 * y1 - x1 * y0;
        a2 += cross;
        cx += (x0 + x1) * cross;
        cy += (y0 + y1) * cross;
    }
    if a2.abs() < 1e-12 {
        let sx: f64 = vertices.iter().map(|v| v.0).sum();
        let sy: f64 = vertices.iter().map(|v| v.1).sum();
        return (sx / n as f64, sy / n as f64);
    }
    (cx / (3.0 * a2), cy / (3.0 * a2))
}

/// Convert square feet to acres.
pub fn sqft_to_acres(area_sqft: f64) -> f64 {
    area_sqft / 43_560.0
}

/// Default flow-path length: plan distance from catchment centroid to a target point (ft).
pub fn default_flow_length_ft(centroid: (f64, f64), target: (f64, f64)) -> f64 {
    let dx = target.0 - centroid.0;
    let dy = target.1 - centroid.1;
    (dx * dx + dy * dy).sqrt()
}

/// Kirpich Tc (minutes) for a catchment polygon draining toward a structure.
pub fn catchment_tc_minutes(flow_length_ft: f64, slope: f64) -> f64 {
    kirpich_minutes(flow_length_ft, slope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_square_area() {
        let verts = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!((shoelace_area_sqft(&verts) - 100.0).abs() < 1e-6);
        assert!((sqft_to_acres(43_560.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn centroid_of_square() {
        let verts = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let (cx, cy) = polygon_centroid(&verts);
        assert!((cx - 5.0).abs() < 1e-6 && (cy - 5.0).abs() < 1e-6);
    }
}