// SPDX-License-Identifier: GPL-3.0-only

//! Pipe sizing against design criteria — smallest standard pipe that carries
//! the Rational design flow within velocity and capacity limits.

use crate::hydraulics::{
    circular_geometry, full_area, full_flow_capacity, max_capacity, normal_depth, K_MANNING_US,
};
use crate::network::{Analysis, Network, Pipe};

use super::criteria::DesignCriteria;

/// Outcome of sizing a single pipe for a known discharge and slope.
#[derive(Clone, Debug, PartialEq)]
pub enum SizeOutcome {
    /// Smallest catalog pipe that meets all criteria.
    Sized,
    /// Current diameter already meets criteria (may equal recommended).
    Adequate,
    /// No catalog pipe satisfies the criteria at this slope.
    NoSolution,
}

/// Result of sizing one pipe cross-section.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeSizeResult {
    pub diameter_ft: f64,
    pub velocity: f64,
    pub pct_full: f64,
    pub normal_depth: Option<f64>,
    pub surcharged: bool,
    pub outcome: SizeOutcome,
}

/// Per-pipe recommendation for a sized network.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeSizeRecommendation {
    pub pipe_id: String,
    pub design_q: f64,
    pub slope: f64,
    pub current_diameter_ft: f64,
    pub recommended_diameter_ft: f64,
    pub meets_criteria: bool,
    pub velocity: f64,
    pub pct_full: f64,
    pub surcharged: bool,
    pub outcome: SizeOutcome,
    pub note: String,
}

/// Evaluate whether diameter `d` carries `q` on slope `s` within criteria.
fn evaluate_diameter(q: f64, slope: f64, n: f64, d: f64, criteria: &DesignCriteria) -> Option<PipeSizeResult> {
    if d <= 0.0 || q < 0.0 {
        return None;
    }
    let k = K_MANNING_US;
    let (q_max, _) = max_capacity(n, slope, d, k);
    if criteria.require_open_channel && q > q_max + 1e-9 {
        return None;
    }
    let yn = normal_depth(q, n, slope, d, k);
    let surcharged = yn.is_none();
    if criteria.require_open_channel && surcharged {
        return None;
    }
    let area = if surcharged {
        full_area(d)
    } else {
        circular_geometry(yn.unwrap_or(0.0), d).0
    };
    let velocity = if area > 0.0 { q / area } else { 0.0 };
    let capacity = full_flow_capacity(n, slope, d, k);
    let pct_full = if capacity > 0.0 { q / capacity } else { 0.0 };

    if velocity < criteria.min_velocity - 1e-9 {
        return None;
    }
    if velocity > criteria.max_velocity + 1e-9 {
        return None;
    }
    if pct_full > criteria.max_pct_full + 1e-9 {
        return None;
    }

    Some(PipeSizeResult {
        diameter_ft: d,
        velocity,
        pct_full,
        normal_depth: yn,
        surcharged,
        outcome: SizeOutcome::Sized,
    })
}

/// Pick the smallest catalog diameter that carries `q` on slope `s`.
pub fn size_pipe_for_flow(q: f64, slope: f64, n: f64, criteria: &DesignCriteria) -> PipeSizeResult {
    for &d in &criteria.standard_diameters_ft {
        if let Some(r) = evaluate_diameter(q, slope, n, d, criteria) {
            return r;
        }
    }
    // No solution — report the largest catalog pipe's hydraulics for diagnostics.
    let d = *criteria.standard_diameters_ft.last().unwrap_or(&0.0);
    let k = K_MANNING_US;
    let yn = normal_depth(q, n, slope, d, k);
    let surcharged = yn.is_none();
    let area = if surcharged {
        full_area(d)
    } else {
        circular_geometry(yn.unwrap_or(0.0), d).0
    };
    let velocity = if area > 0.0 { q / area } else { 0.0 };
    let capacity = full_flow_capacity(n, slope, d, k);
    PipeSizeResult {
        diameter_ft: d,
        velocity,
        pct_full: if capacity > 0.0 { q / capacity } else { 0.0 },
        normal_depth: yn,
        surcharged,
        outcome: SizeOutcome::NoSolution,
    }
}

/// Check whether an existing diameter meets criteria (no upsizing).
pub fn check_pipe(q: f64, slope: f64, n: f64, diameter_ft: f64, criteria: &DesignCriteria) -> PipeSizeResult {
    if let Some(mut r) = evaluate_diameter(q, slope, n, diameter_ft, criteria) {
        r.outcome = SizeOutcome::Adequate;
        r
    } else {
        size_pipe_for_flow(q, slope, n, criteria)
    }
}

fn format_diameter_in(d_ft: f64) -> String {
    let inches = (d_ft * 12.0).round() as i32;
    format!("{inches}\"")
}

fn recommend_for_pipe(p: &Pipe, design_q: f64, slope: f64, criteria: &DesignCriteria) -> PipeSizeRecommendation {
    let current = p.diameter;
    let check = check_pipe(design_q, slope, p.n, current, criteria);
    let sized = size_pipe_for_flow(design_q, slope, p.n, criteria);

    let meets = check.outcome == SizeOutcome::Adequate
        && (check.diameter_ft - current).abs() < 1e-6
        && !check.surcharged;

    let (recommended, outcome, note) = if meets {
        (
            current,
            SizeOutcome::Adequate,
            format!("{} meets criteria ({:.1}% full, {:.2} ft/s)", p.id, check.pct_full * 100.0, check.velocity),
        )
    } else if sized.outcome == SizeOutcome::NoSolution {
        (
            sized.diameter_ft,
            SizeOutcome::NoSolution,
            format!(
                "{}: no catalog pipe meets criteria (Q={:.2} cfs, S={:.4}); largest tried {} still surcharged={}",
                p.id,
                design_q,
                slope,
                format_diameter_in(sized.diameter_ft),
                sized.surcharged
            ),
        )
    } else {
        (
            sized.diameter_ft,
            SizeOutcome::Sized,
            format!(
                "{}: upsize {} → {} ({:.1}% full, {:.2} ft/s)",
                p.id,
                format_diameter_in(current),
                format_diameter_in(sized.diameter_ft),
                sized.pct_full * 100.0,
                sized.velocity
            ),
        )
    };

    PipeSizeRecommendation {
        pipe_id: p.id.clone(),
        design_q,
        slope,
        current_diameter_ft: current,
        recommended_diameter_ft: recommended,
        meets_criteria: meets,
        velocity: if meets { check.velocity } else { sized.velocity },
        pct_full: if meets { check.pct_full } else { sized.pct_full },
        surcharged: if meets { check.surcharged } else { sized.surcharged },
        outcome,
        note,
    }
}

/// Size every pipe in `net` using flows from a completed [`Analysis`].
pub fn size_network(net: &Network, analysis: &Analysis, criteria: &DesignCriteria) -> Vec<PipeSizeRecommendation> {
    net.pipes
        .iter()
        .map(|p| {
            let pr = analysis.pipes.iter().find(|r| r.id == p.id);
            let (q, slope) = pr.map(|r| (r.design_q, r.slope)).unwrap_or((0.0, 0.0));
            recommend_for_pipe(p, q, slope, criteria)
        })
        .collect()
}

/// Apply recommended diameters to a network (returns a cloned, sized network).
pub fn apply_sizing(net: &Network, recs: &[PipeSizeRecommendation]) -> Network {
    let mut sized = net.clone();
    for (p, r) in sized.pipes.iter_mut().zip(recs.iter()) {
        if r.outcome != SizeOutcome::NoSolution {
            p.diameter = r.recommended_diameter_ft;
        }
    }
    sized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{Network, Node};

    fn heavy_net() -> Network {
        Network {
            nodes: vec![
                Node::inlet("N1", 100.0, 105.0, 2.0, 0.7),
                Node::inlet("N2", 99.0, 104.0, 3.0, 0.8),
                Node::outfall("OUT", 98.0, 103.0),
            ],
            pipes: vec![
                Pipe::new("P1", "N1", "N2", 100.0, 1.5, 0.013),
                Pipe::new("P2", "N2", "OUT", 100.0, 1.5, 0.013),
            ],
        }
    }

    #[test]
    fn adequately_sized_pipe_is_adequate() {
        let net = heavy_net();
        let pipes = net.analyze_rational(2.0).unwrap();
        let p2_q = pipes.iter().find(|x| x.id == "P2").unwrap().design_q;
        let criteria = DesignCriteria::default();
        let recs = size_network(&net, &Analysis { pipes, nodes: vec![] }, &criteria);
        let p2r = recs.iter().find(|r| r.pipe_id == "P2").unwrap();
        assert!(p2r.meets_criteria || p2r.recommended_diameter_ft >= 1.5);
        assert!(p2_q > 0.0);
    }

    #[test]
    fn undersized_pipe_gets_larger_recommendation() {
        let net = heavy_net();
        let pipes = net.analyze_rational(4.0).unwrap();
        assert!(pipes.iter().find(|x| x.id == "P2").unwrap().surcharged);
        let criteria = DesignCriteria::default();
        let recs = size_network(&net, &Analysis { pipes, nodes: vec![] }, &criteria);
        let p2r = recs.iter().find(|r| r.pipe_id == "P2").unwrap();
        assert!(p2r.recommended_diameter_ft > 1.5, "got {}", p2r.recommended_diameter_ft);
        assert_eq!(p2r.outcome, SizeOutcome::Sized);
    }

    #[test]
    fn apply_sizing_updates_diameters() {
        let net = heavy_net();
        let pipes = net.analyze_rational(4.0).unwrap();
        let criteria = DesignCriteria::default();
        let recs = size_network(&net, &Analysis { pipes, nodes: vec![] }, &criteria);
        let sized = apply_sizing(&net, &recs);
        let p2 = sized.pipes.iter().find(|p| p.id == "P2").unwrap();
        assert!(p2.diameter > 1.5);
    }

    #[test]
    fn sample_network_has_catalog_solutions() {
        let text = include_str!("../../examples/sample.ssn");
        let parsed = crate::parse::parse_ssn(text).unwrap();
        let a = parsed.network.analyze(&parsed.idf, &parsed.options).unwrap();
        assert!(a.pipes.iter().all(|p| !p.surcharged), "sample should not surcharge");
        let recs = size_network(&parsed.network, &a, &DesignCriteria::default());
        assert!(
            recs.iter().all(|r| r.outcome != SizeOutcome::NoSolution),
            "unexpected: {:?}",
            recs.iter().map(|r| (&r.pipe_id, &r.note)).collect::<Vec<_>>()
        );
    }
}