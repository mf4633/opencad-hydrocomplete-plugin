// SPDX-License-Identifier: GPL-3.0-only

//! Convert an analyzed network into drawable primitives (CAD-agnostic).
//!
//! Produces a **plan** view (pipes as lines, structures as markers, flow/HGL
//! labels) and a **profile** (HGL long-section of the main stem: ground,
//! invert, and hydraulic-grade-line polylines). Coordinates are plain `f64`
//! drawing units; the host CAD turns these into its own entities.

use crate::network::{Analysis, Network, NodeKind};
use std::collections::HashMap;

/// A pipe drawn in plan.
#[derive(Clone, Debug)]
pub struct PlanPipe {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub surcharged: bool,
}

/// A structure marker drawn in plan.
#[derive(Clone, Debug)]
pub struct PlanNode {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub kind: NodeKind,
}

/// A text label placed at a point.
#[derive(Clone, Debug)]
pub struct Label {
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub height: f64,
}

/// Which line of the profile a polyline represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileRole {
    Ground,
    Invert,
    Hgl,
}

/// A polyline (sequence of points) with a role.
#[derive(Clone, Debug)]
pub struct Polyline {
    pub pts: Vec<(f64, f64)>,
    pub role: ProfileRole,
}

/// The full set of primitives for a network drawing.
#[derive(Clone, Debug, Default)]
pub struct NetworkDrawing {
    pub plan_pipes: Vec<PlanPipe>,
    pub plan_nodes: Vec<PlanNode>,
    pub plan_labels: Vec<Label>,
    pub profile_lines: Vec<Polyline>,
    pub profile_labels: Vec<Label>,
}

/// Layout / styling knobs for [`draw_network`].
#[derive(Clone, Debug)]
pub struct DrawConfig {
    pub text_height: f64,
    pub node_radius: f64,
    /// Plan X mapped to profile X=0 (station origin in drawing units).
    pub profile_origin_x: f64,
    /// Drawing Y at which the profile datum elevation is plotted.
    pub profile_origin_y: f64,
    /// Horizontal scale: station feet → drawing units.
    pub h_scale: f64,
    /// Vertical exaggeration applied to elevations in the profile.
    pub v_exag: f64,
}

impl Default for DrawConfig {
    fn default() -> Self {
        Self {
            text_height: 5.0,
            node_radius: 3.0,
            profile_origin_x: 0.0,
            profile_origin_y: -200.0,
            h_scale: 1.0,
            v_exag: 10.0,
        }
    }
}

/// Build plan + profile primitives for an analyzed network.
pub fn draw_network(net: &Network, a: &Analysis, cfg: &DrawConfig) -> NetworkDrawing {
    let mut d = NetworkDrawing::default();
    let pos: HashMap<&str, (f64, f64)> =
        net.nodes.iter().map(|n| (n.id.as_str(), (n.x, n.y))).collect();
    let hgl: HashMap<&str, f64> = a.nodes.iter().map(|n| (n.id.as_str(), n.hgl)).collect();

    // ── Plan: pipes + labels ────────────────────────────────────────────────
    for pr in &a.pipes {
        let (x1, y1) = pos[pr.from.as_str()];
        let (x2, y2) = pos[pr.to.as_str()];
        d.plan_pipes.push(PlanPipe { x1, y1, x2, y2, surcharged: pr.surcharged });
        d.plan_labels.push(Label {
            x: (x1 + x2) / 2.0,
            y: (y1 + y2) / 2.0 + cfg.text_height,
            text: format!("{}: {:.1} cfs {:.0}%", pr.id, pr.design_q, pr.pct_full * 100.0),
            height: cfg.text_height,
        });
    }

    // ── Plan: structure markers + labels ────────────────────────────────────
    for n in &net.nodes {
        d.plan_nodes.push(PlanNode { x: n.x, y: n.y, radius: cfg.node_radius, kind: n.kind });
        let h = hgl.get(n.id.as_str()).copied().unwrap_or(f64::NAN);
        let label = if h.is_finite() {
            format!("{} HGL {:.1}", n.id, h)
        } else {
            n.id.clone()
        };
        d.plan_labels.push(Label {
            x: n.x + cfg.node_radius,
            y: n.y + cfg.node_radius,
            text: label,
            height: cfg.text_height,
        });
    }

    // ── Profile of the main stem ────────────────────────────────────────────
    let stem = main_stem(net);
    if stem.len() >= 2 {
        let nidx: HashMap<&str, usize> =
            net.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();

        // Stations (ft) from the upstream end.
        let mut stations = vec![0.0f64; stem.len()];
        for k in 1..stem.len() {
            let len = pipe_between(net, stem[k - 1], stem[k]).map(|p| p.length).unwrap_or(0.0);
            stations[k] = stations[k - 1] + len;
        }
        let datum = stem.iter().map(|&i| net.nodes[i].invert).fold(f64::INFINITY, f64::min);
        let px = |st: f64| cfg.profile_origin_x + st * cfg.h_scale;
        let py = |elev: f64| cfg.profile_origin_y + (elev - datum) * cfg.v_exag;

        let mut ground = Vec::new();
        let mut invert = Vec::new();
        let mut hgl_line = Vec::new();
        for (k, &i) in stem.iter().enumerate() {
            let n = &net.nodes[i];
            let st = stations[k];
            ground.push((px(st), py(n.rim)));
            invert.push((px(st), py(n.invert)));
            if let Some(&h) = hgl.get(n.id.as_str()) {
                if h.is_finite() {
                    hgl_line.push((px(st), py(h)));
                }
            }
            d.profile_labels.push(Label {
                x: px(st),
                y: py(n.rim) + cfg.text_height,
                text: n.id.clone(),
                height: cfg.text_height,
            });
            let _ = nidx; // (reserved for future cross-refs)
        }
        d.profile_lines.push(Polyline { pts: ground, role: ProfileRole::Ground });
        d.profile_lines.push(Polyline { pts: invert, role: ProfileRole::Invert });
        if hgl_line.len() >= 2 {
            d.profile_lines.push(Polyline { pts: hgl_line, role: ProfileRole::Hgl });
        }
    }

    d
}

/// The main trunk, upstream-first: walk from the outfall up the incoming pipe
/// whose upstream node carries the most accumulated drainage area.
fn main_stem(net: &Network) -> Vec<usize> {
    let n = net.nodes.len();
    if n == 0 {
        return Vec::new();
    }
    let nidx: HashMap<&str, usize> =
        net.nodes.iter().enumerate().map(|(i, nd)| (nd.id.as_str(), i)).collect();
    let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); n]; // upstream node indices
    let mut has_out = vec![false; n];
    for p in &net.pipes {
        if let (Some(&u), Some(&v)) = (nidx.get(p.from.as_str()), nidx.get(p.to.as_str())) {
            incoming[v].push(u);
            has_out[u] = true;
        }
    }
    let ca = net.accumulate_ca().unwrap_or_default();
    let size = |i: usize| ca.get(net.nodes[i].id.as_str()).copied().unwrap_or(0.0);

    // Start at the outfall (kind, else the first node with no outgoing pipe).
    let start = net
        .nodes
        .iter()
        .position(|nd| nd.kind == NodeKind::Outfall)
        .or_else(|| (0..n).find(|&i| !has_out[i]))
        .unwrap_or(0);

    let mut stem = vec![start];
    let mut cur = start;
    let mut guard = 0;
    while guard < n {
        guard += 1;
        match incoming[cur].iter().copied().max_by(|&a, &b| {
            size(a).partial_cmp(&size(b)).unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Some(up) => {
                stem.push(up);
                cur = up;
            }
            None => break,
        }
    }
    stem.reverse(); // upstream-first
    stem
}

fn pipe_between<'a>(net: &'a Network, up: usize, dn: usize) -> Option<&'a crate::network::Pipe> {
    let up_id = net.nodes[up].id.as_str();
    let dn_id = net.nodes[dn].id.as_str();
    net.pipes.iter().find(|p| p.from == up_id && p.to == dn_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idf::IdfCurve;
    use crate::network::{AnalysisOptions, Node, Pipe};

    fn sample() -> Network {
        Network {
            nodes: vec![
                Node::inlet("N1", 104.0, 110.0, 1.0, 0.70).at(0.0, 0.0),
                Node::inlet("N2", 102.5, 108.5, 1.0, 0.70).at(300.0, 0.0),
                Node::junction("N3", 101.2, 107.0, 0.5, 0.80).at(550.0, 0.0),
                Node::outfall("OUT", 100.0, 106.0).at(730.0, 0.0),
            ],
            pipes: vec![
                Pipe::new("P1", "N1", "N2", 300.0, 1.25, 0.013),
                Pipe::new("P2", "N2", "N3", 250.0, 1.50, 0.013),
                Pipe::new("P3", "N3", "OUT", 180.0, 1.75, 0.013),
            ],
        }
    }

    fn analyzed() -> (Network, Analysis) {
        let net = sample();
        let a = net.analyze(&IdfCurve::new(60.0, 10.0, 0.8), &AnalysisOptions { tailwater: Some(100.5), ..Default::default() }).unwrap();
        (net, a)
    }

    #[test]
    fn plan_has_one_line_per_pipe_and_marker_per_node() {
        let (net, a) = analyzed();
        let d = draw_network(&net, &a, &DrawConfig::default());
        assert_eq!(d.plan_pipes.len(), net.pipes.len());
        assert_eq!(d.plan_nodes.len(), net.nodes.len());
        assert!(d.plan_labels.len() >= net.pipes.len() + net.nodes.len());
    }

    #[test]
    fn main_stem_is_full_trunk_upstream_first() {
        let net = sample();
        let stem = main_stem(&net);
        let ids: Vec<&str> = stem.iter().map(|&i| net.nodes[i].id.as_str()).collect();
        assert_eq!(ids, vec!["N1", "N2", "N3", "OUT"]);
    }

    #[test]
    fn profile_has_ground_invert_and_hgl() {
        let (net, a) = analyzed();
        let d = draw_network(&net, &a, &DrawConfig::default());
        let roles: Vec<ProfileRole> = d.profile_lines.iter().map(|p| p.role).collect();
        assert!(roles.contains(&ProfileRole::Ground));
        assert!(roles.contains(&ProfileRole::Invert));
        assert!(roles.contains(&ProfileRole::Hgl));
        for pl in &d.profile_lines {
            assert!(pl.pts.len() >= 2, "{:?} too short", pl.role);
        }
    }
}
