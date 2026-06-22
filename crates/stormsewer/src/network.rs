// SPDX-License-Identifier: GPL-3.0-only

//! Storm-sewer network model: nodes (inlets / junctions / outfalls), pipes,
//! topology, Rational-method peak-flow accumulation, time-of-concentration /
//! IDF intensity, and a hydraulic-grade-line (HGL) backwater pass.
//!
//! The network is assumed **dendritic** (tree-like, draining to outfalls) as is
//! standard for gravity HydroCompletes. Looped networks are rejected by the
//! topological sort.

use crate::design::{size_network, DesignCriteria, PipeSizeRecommendation};
use crate::hydraulics::*;
use crate::hydrology::IdfSet;
use crate::idf::IdfCurve;
use crate::params::StormAnalysisParams;
use std::collections::HashMap;

/// Kind of network node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Inlet,
    Junction,
    Outfall,
}

/// A network node (structure).
#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    /// Flowline / invert elevation (ft).
    pub invert: f64,
    /// Rim / ground elevation (ft) — used for surcharge-to-surface checks.
    pub rim: f64,
    /// Local contributing drainage area (acres).
    pub area_ac: f64,
    /// Rational runoff coefficient C for the local area (0–1).
    pub c: f64,
    /// Inlet time of concentration for the local catchment (minutes).
    pub tc_inlet: f64,
    /// Plan (map) X coordinate of the structure (drawing units).
    pub x: f64,
    /// Plan (map) Y coordinate of the structure (drawing units).
    pub y: f64,
}

impl Node {
    pub fn inlet(id: &str, invert: f64, rim: f64, area_ac: f64, c: f64) -> Self {
        Self { id: id.into(), kind: NodeKind::Inlet, invert, rim, area_ac, c, tc_inlet: 10.0, x: 0.0, y: 0.0 }
    }
    pub fn junction(id: &str, invert: f64, rim: f64, area_ac: f64, c: f64) -> Self {
        Self { id: id.into(), kind: NodeKind::Junction, invert, rim, area_ac, c, tc_inlet: 10.0, x: 0.0, y: 0.0 }
    }
    pub fn outfall(id: &str, invert: f64, rim: f64) -> Self {
        Self { id: id.into(), kind: NodeKind::Outfall, invert, rim, area_ac: 0.0, c: 0.0, tc_inlet: 0.0, x: 0.0, y: 0.0 }
    }
    /// Builder: set the local inlet time (minutes).
    pub fn with_tc_inlet(mut self, t_min: f64) -> Self {
        self.tc_inlet = t_min;
        self
    }
    /// Builder: set the plan (map) coordinates.
    pub fn at(mut self, x: f64, y: f64) -> Self {
        self.x = x;
        self.y = y;
        self
    }
    /// Local C*A product (acres).
    pub fn ca(&self) -> f64 {
        self.c * self.area_ac
    }
}

/// A pipe (network link) carrying flow from `from` (upstream) to `to`.
#[derive(Clone, Debug)]
pub struct Pipe {
    pub id: String,
    pub from: String,
    pub to: String,
    pub length: f64,   // ft
    pub diameter: f64, // ft
    pub n: f64,        // Manning roughness
}

impl Pipe {
    pub fn new(id: &str, from: &str, to: &str, length: f64, diameter: f64, n: f64) -> Self {
        Self { id: id.into(), from: from.into(), to: to.into(), length, diameter, n }
    }
}

/// A storm-sewer network.
#[derive(Clone, Debug, Default)]
pub struct Network {
    pub nodes: Vec<Node>,
    pub pipes: Vec<Pipe>,
}

/// Per-pipe analysis result.
#[derive(Clone, Debug)]
pub struct PipeResult {
    pub id: String,
    pub from: String,
    pub to: String,
    /// Pipe slope from upstream/downstream inverts (ft/ft).
    pub slope: f64,
    /// Slope used for Manning capacity (equals [`slope`](Self::slope) when positive;
    /// otherwise [`AnalysisOptions::min_slope`](AnalysisOptions::min_slope) for flat inverts).
    pub manning_slope: f64,
    /// Accumulated C*A draining into this pipe (acres).
    pub total_ca: f64,
    /// Design rainfall intensity used (in/hr).
    pub intensity: f64,
    /// Time of concentration at the upstream end (minutes).
    pub tc: f64,
    /// Flow travel time through this pipe at design flow (minutes).
    pub travel_time: f64,
    /// Rational peak design discharge (cfs).
    pub design_q: f64,
    /// Just-full capacity (cfs).
    pub capacity: f64,
    /// Maximum open-channel capacity (~0.94 d) (cfs).
    pub max_capacity: f64,
    /// True if the design flow exceeds open-channel capacity (pipe surcharges).
    pub surcharged: bool,
    /// Normal depth (ft), or `None` if surcharged.
    pub normal_depth: Option<f64>,
    /// Critical depth (ft).
    pub critical_depth: f64,
    /// Actual velocity at design flow (ft/s).
    pub velocity: f64,
    /// Full-flow velocity (ft/s).
    pub velocity_full: f64,
    /// Design flow as a fraction of just-full capacity.
    pub pct_full: f64,
    /// HGL elevation at the upstream structure (ft), if an HGL pass was run.
    pub hgl_up: Option<f64>,
    /// HGL elevation at the downstream structure (ft), if an HGL pass was run.
    pub hgl_dn: Option<f64>,
}

impl PipeResult {
    /// Manning capacity cannot be compared to design Q (adverse slope, or flat/zero with no capacity).
    pub fn capacity_unavailable(&self) -> bool {
        self.manning_slope < 0.0 || (self.slope <= 0.0 && self.capacity <= 0.0)
    }

    /// True surcharge for reporting — excludes zero/adverse-slope false positives.
    pub fn report_surcharged(&self) -> bool {
        self.surcharged && !self.capacity_unavailable()
    }

    /// HTML/text label when [`Self::capacity_unavailable`] is true.
    pub fn capacity_na_label(&self) -> &'static str {
        if self.manning_slope < 0.0 {
            "ADVERSE SLOPE — capacity N/A"
        } else {
            "ZERO SLOPE — capacity N/A"
        }
    }
}

/// Back-compat alias — prefer [`PipeResult::capacity_na_label`].
pub const CAPACITY_NA_ZERO_SLOPE: &str = "ZERO SLOPE — capacity N/A";

/// Per-node analysis result (populated by the HGL pass).
#[derive(Clone, Debug)]
pub struct NodeResult {
    pub id: String,
    pub tc: f64,
    pub hgl: f64,
    pub rim: f64,
    /// True if the HGL rises above the rim (flooding).
    pub surcharge_to_surface: bool,
}

/// Full network analysis (pipes + nodes).
#[derive(Clone, Debug)]
pub struct Analysis {
    pub pipes: Vec<PipeResult>,
    pub nodes: Vec<NodeResult>,
}

/// Options for [`Network::analyze`].
#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisOptions {
    /// Minimum time of concentration (minutes) — floors the IDF duration.
    pub min_tc: f64,
    /// Tailwater elevation at outfalls (ft). `None` → free outfall at invert.
    pub tailwater: Option<f64>,
    /// Junction/structure loss coefficient K in `H = K * V^2 / 2g`.
    pub junction_k: f64,
    /// If set, use this constant intensity (in/hr) instead of the IDF curve.
    pub intensity_override: Option<f64>,
    /// Minimum slope (ft/ft) assumed for Manning capacity when pipe inverts are flat.
    pub min_slope: f64,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            min_tc: 10.0,
            tailwater: None,
            junction_k: 0.5,
            intensity_override: None,
            min_slope: 0.001,
        }
    }
}

/// Bed slope from inverts; when flat (`bed == 0`), use `min_slope` for Manning capacity.
fn manning_slope(bed_slope: f64, min_slope: f64) -> f64 {
    if bed_slope > 0.0 {
        bed_slope
    } else if bed_slope < 0.0 {
        bed_slope
    } else {
        min_slope
    }
}

/// Errors raised during analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkError {
    UnknownNode(String),
    CyclicNetwork,
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::UnknownNode(id) => write!(f, "pipe references unknown node '{id}'"),
            NetworkError::CyclicNetwork => write!(f, "network contains a cycle"),
        }
    }
}
impl std::error::Error for NetworkError {}

/// Conveyance K = (k/n) * A * R^(2/3), so that Q = K * sqrt(S).
fn conveyance(n: f64, area: f64, radius: f64, k: f64) -> f64 {
    if n <= 0.0 {
        return 0.0;
    }
    k / n * area * radius.powf(2.0 / 3.0)
}

impl Network {
    fn index(&self) -> HashMap<&str, usize> {
        self.nodes.iter().enumerate().map(|(i, nd)| (nd.id.as_str(), i)).collect()
    }

    /// Topological node order, most-upstream first (Kahn's algorithm).
    fn topo_order(&self, idx: &HashMap<&str, usize>) -> Result<Vec<usize>, NetworkError> {
        let n = self.nodes.len();
        let mut indeg = vec![0usize; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for p in &self.pipes {
            let u = *idx.get(p.from.as_str()).ok_or_else(|| NetworkError::UnknownNode(p.from.clone()))?;
            let v = *idx.get(p.to.as_str()).ok_or_else(|| NetworkError::UnknownNode(p.to.clone()))?;
            adj[u].push(v);
            indeg[v] += 1;
        }
        let mut queue: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(u) = queue.pop() {
            order.push(u);
            for &v in &adj[u] {
                indeg[v] -= 1;
                if indeg[v] == 0 {
                    queue.push(v);
                }
            }
        }
        if order.len() != n {
            return Err(NetworkError::CyclicNetwork);
        }
        Ok(order)
    }

    /// Accumulated C*A (acres) reaching each node, keyed by node id.
    pub fn accumulate_ca(&self) -> Result<HashMap<String, f64>, NetworkError> {
        let idx = self.index();
        let order = self.topo_order(&idx)?;
        let mut feeders: Vec<Vec<usize>> = vec![Vec::new(); self.nodes.len()];
        for p in &self.pipes {
            feeders[idx[p.to.as_str()]].push(idx[p.from.as_str()]);
        }
        let mut total = vec![0.0f64; self.nodes.len()];
        for &i in &order {
            let mut acc = self.nodes[i].ca();
            for &u in &feeders[i] {
                acc += total[u];
            }
            total[i] = acc;
        }
        Ok(self.nodes.iter().enumerate().map(|(i, nd)| (nd.id.clone(), total[i])).collect())
    }

    /// Simple Rational analysis with one constant intensity (no Tc/HGL).
    /// Retained for quick checks; [`analyze`](Self::analyze) is the full method.
    pub fn analyze_rational(&self, intensity: f64) -> Result<Vec<PipeResult>, NetworkError> {
        let opts = AnalysisOptions { intensity_override: Some(intensity), ..Default::default() };
        // A flat IDF is unused when intensity_override is set.
        Ok(self.analyze(&IdfCurve::new(0.0, 1.0, 1.0), &opts)?.pipes)
    }

    /// Analyze then recommend standard pipe diameters for each link.
    pub fn analyze_and_size(
        &self,
        idf: &IdfCurve,
        opts: &AnalysisOptions,
        criteria: &DesignCriteria,
    ) -> Result<(Analysis, Vec<PipeSizeRecommendation>), NetworkError> {
        let a = self.analyze(idf, opts)?;
        let recs = size_network(self, &a, criteria);
        Ok((a, recs))
    }

    /// Full analyze + size using [`StormAnalysisParams`].
    pub fn analyze_and_size_params(
        &self,
        params: &StormAnalysisParams,
    ) -> Result<(Analysis, Vec<PipeSizeRecommendation>), NetworkError> {
        self.analyze_and_size(params.idf.design_curve(), &params.hydraulics, &params.sizing)
    }

    /// Run analysis at every configured return period.
    pub fn analyze_all_rps(
        &self,
        idf_set: &IdfSet,
        opts: &AnalysisOptions,
    ) -> Result<Vec<(u32, Analysis)>, NetworkError> {
        let mut out = Vec::new();
        for rp in idf_set.return_periods() {
            let curve = idf_set.curve(rp).expect("return_periods keys exist");
            out.push((rp, self.analyze(curve, opts)?));
        }
        Ok(out)
    }

    /// Full analysis: Tc accumulation, per-pipe IDF intensity, Rational design
    /// flows, pipe hydraulics, and an HGL backwater pass with junction losses.
    pub fn analyze(&self, idf: &IdfCurve, opts: &AnalysisOptions) -> Result<Analysis, NetworkError> {
        let k = K_MANNING_US;
        let idxm = self.index();
        let order = self.topo_order(&idxm)?;
        let n_nodes = self.nodes.len();
        let n_pipes = self.pipes.len();

        // Resolve pipe endpoints to node indices once.
        let pe: Vec<(usize, usize)> = self
            .pipes
            .iter()
            .map(|p| (idxm[p.from.as_str()], idxm[p.to.as_str()]))
            .collect();

        // incoming[node] = (pipe_idx, upstream_node_idx)
        let mut incoming: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n_nodes];
        // outgoing[node] = (pipe_idx, downstream_node_idx)
        let mut outgoing: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n_nodes];
        for (pi, &(u, v)) in pe.iter().enumerate() {
            outgoing[u].push((pi, v));
            incoming[v].push((pi, u));
        }

        let ca = self.accumulate_ca()?;
        let total_ca: Vec<f64> = self.nodes.iter().map(|nd| ca[&nd.id]).collect();

        // Per-pipe scratch (bed slope from inverts).
        let mut p_slope = vec![0.0f64; n_pipes];
        let mut p_manning_slope = vec![0.0f64; n_pipes];
        let mut p_intensity = vec![0.0f64; n_pipes];
        let mut p_q = vec![0.0f64; n_pipes];
        let mut p_vel = vec![0.0f64; n_pipes];
        let mut p_surch = vec![false; n_pipes];
        let mut p_yn: Vec<Option<f64>> = vec![None; n_pipes];
        let mut p_travel = vec![0.0f64; n_pipes];
        let mut tc_node = vec![0.0f64; n_nodes];

        // ── Forward pass (upstream → downstream): Tc, intensity, design Q, vel.
        for &i in &order {
            let nd = &self.nodes[i];
            let mut tc = if nd.kind == NodeKind::Outfall { 0.0 } else { nd.tc_inlet };
            for &(pi, _u) in &incoming[i] {
                tc = tc.max(tc_node[/*upstream*/ pe[pi].0] + p_travel[pi]);
            }
            tc = tc.max(opts.min_tc);
            tc_node[i] = tc;

            for &(pi, v) in &outgoing[i] {
                let p = &self.pipes[pi];
                let bed_slope = if p.length > 0.0 {
                    (self.nodes[i].invert - self.nodes[v].invert) / p.length
                } else {
                    0.0
                };
                let m_slope = manning_slope(bed_slope, opts.min_slope);
                let intensity = opts.intensity_override.unwrap_or_else(|| idf.intensity(tc));
                let q = intensity * total_ca[i];
                let (q_max, _) = max_capacity(p.n, m_slope, p.diameter, k);
                let surcharged = q > q_max;
                let yn = normal_depth(q, p.n, m_slope, p.diameter, k);
                let area = if surcharged {
                    full_area(p.diameter)
                } else {
                    circular_geometry(yn.unwrap_or(p.diameter), p.diameter).0
                };
                let vel = if area > 0.0 { q / area } else { 0.0 };
                let travel = if vel > 0.0 { p.length / vel / 60.0 } else { 0.0 };

                p_slope[pi] = bed_slope;
                p_manning_slope[pi] = m_slope;
                p_intensity[pi] = intensity;
                p_q[pi] = q;
                p_vel[pi] = vel;
                p_surch[pi] = surcharged;
                p_yn[pi] = yn;
                p_travel[pi] = travel;
            }
        }

        // ── Backward pass (downstream → upstream): HGL with junction losses.
        let mut hgl = vec![f64::NAN; n_nodes];
        let mut pipe_hgl_up = vec![None; n_pipes];
        let mut pipe_hgl_dn = vec![None; n_pipes];

        for &d in order.iter().rev() {
            // Seed outfall HGL from tailwater (or free outfall at invert).
            if self.nodes[d].kind == NodeKind::Outfall && hgl[d].is_nan() {
                hgl[d] = opts.tailwater.unwrap_or(self.nodes[d].invert);
            }
            if hgl[d].is_nan() {
                hgl[d] = self.nodes[d].invert; // disconnected fallback
            }

            for &(pi, u) in &incoming[d] {
                let p = &self.pipes[pi];
                let dia = p.diameter;
                let inv_d = self.nodes[d].invert;
                let inv_u = self.nodes[u].invert;
                let q = p_q[pi];

                let (ws_d, hf) = if p_surch[pi] {
                    let conv_full = conveyance(p.n, full_area(dia), dia / 4.0, k);
                    let sf = if conv_full > 0.0 { (q / conv_full).powi(2) } else { 0.0 };
                    let crown_d = inv_d + dia;
                    (hgl[d].max(crown_d), sf * p.length)
                } else {
                    let yn = p_yn[pi].unwrap_or(0.0);
                    let (a, _pp, r, _t) = circular_geometry(yn, dia);
                    let conv = conveyance(p.n, a, r, k);
                    let sf = if conv > 0.0 { (q / conv).powi(2) } else { p_slope[pi].max(0.0) };
                    (hgl[d].max(inv_d + yn), sf * p.length)
                };

                let yn_u = p_yn[pi].unwrap_or(0.0);
                let hgl_us_pipe = (ws_d + hf).max(inv_u + yn_u);
                let hj = opts.junction_k * p_vel[pi].powi(2) / (2.0 * G_US);
                let hgl_u = hgl_us_pipe + hj;

                hgl[u] = if hgl[u].is_nan() { hgl_u } else { hgl[u].max(hgl_u) };
                pipe_hgl_dn[pi] = Some(hgl[d]);
                pipe_hgl_up[pi] = Some(hgl_u);
            }
        }

        // ── Assemble results.
        let pipes = self
            .pipes
            .iter()
            .enumerate()
            .map(|(pi, p)| {
                let capacity = full_flow_capacity(p.n, p_manning_slope[pi], p.diameter, k);
                let velocity_full = if full_area(p.diameter) > 0.0 {
                    capacity / full_area(p.diameter)
                } else {
                    0.0
                };
                PipeResult {
                    id: p.id.clone(),
                    from: p.from.clone(),
                    to: p.to.clone(),
                    slope: p_slope[pi],
                    manning_slope: p_manning_slope[pi],
                    total_ca: total_ca[pe[pi].0],
                    intensity: p_intensity[pi],
                    tc: tc_node[pe[pi].0],
                    travel_time: p_travel[pi],
                    design_q: p_q[pi],
                    capacity,
                    max_capacity: max_capacity(p.n, p_manning_slope[pi], p.diameter, k).0,
                    surcharged: p_surch[pi],
                    normal_depth: p_yn[pi],
                    critical_depth: critical_depth(p_q[pi], p.diameter, G_US),
                    velocity: p_vel[pi],
                    velocity_full,
                    pct_full: if capacity > 0.0 { p_q[pi] / capacity } else { 0.0 },
                    hgl_up: pipe_hgl_up[pi],
                    hgl_dn: pipe_hgl_dn[pi],
                }
            })
            .collect();

        let nodes = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, nd)| NodeResult {
                id: nd.id.clone(),
                tc: tc_node[i],
                hgl: hgl[i],
                rim: nd.rim,
                surcharge_to_surface: nd.kind != NodeKind::Outfall && hgl[i] > nd.rim,
            })
            .collect();

        Ok(Analysis { pipes, nodes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// N1 (inlet) -> N2 (inlet) -> OUT, two pipes.
    fn sample() -> Network {
        Network {
            nodes: vec![
                Node::inlet("N1", 100.0, 105.0, 2.0, 0.7), // CA = 1.4
                Node::inlet("N2", 99.0, 104.0, 3.0, 0.8),  // CA = 2.4
                Node::outfall("OUT", 98.0, 103.0),
            ],
            pipes: vec![
                Pipe::new("P1", "N1", "N2", 100.0, 1.5, 0.013),
                Pipe::new("P2", "N2", "OUT", 100.0, 1.5, 0.013),
            ],
        }
    }

    #[test]
    fn ca_accumulates_downstream() {
        let ca = sample().accumulate_ca().unwrap();
        assert!((ca["N1"] - 1.4).abs() < 1e-9);
        assert!((ca["N2"] - 3.8).abs() < 1e-9);
        assert!((ca["OUT"] - 3.8).abs() < 1e-9);
    }

    #[test]
    fn rational_design_flows() {
        let r = sample().analyze_rational(4.0).unwrap();
        let p1 = r.iter().find(|x| x.id == "P1").unwrap();
        let p2 = r.iter().find(|x| x.id == "P2").unwrap();
        assert!((p1.design_q - 5.6).abs() < 1e-6, "P1 {}", p1.design_q);
        assert!((p2.design_q - 15.2).abs() < 1e-6, "P2 {}", p2.design_q);
    }

    #[test]
    fn slope_from_inverts() {
        let r = sample().analyze_rational(4.0).unwrap();
        assert!((r[0].slope - 0.01).abs() < 1e-9);
        assert!((r[0].manning_slope - 0.01).abs() < 1e-9);
    }

    #[test]
    fn adverse_slope_is_capacity_unavailable_not_surcharged() {
        let net = Network {
            nodes: vec![
                Node::inlet("N1", 100.0, 106.0, 1.0, 0.7),
                Node::outfall("OUT", 102.0, 106.0),
            ],
            pipes: vec![Pipe::new("P1", "N1", "OUT", 100.0, 1.5, 0.013)],
        };
        let r = net
            .analyze(&IdfCurve::new(60.0, 10.0, 0.8), &Default::default())
            .unwrap();
        let p = &r.pipes[0];
        assert!(p.slope < 0.0);
        assert!(p.capacity_unavailable());
        assert_eq!(p.capacity_na_label(), "ADVERSE SLOPE — capacity N/A");
        assert!(!p.report_surcharged());
    }

    #[test]
    fn flat_inverts_assume_min_slope_for_manning() {
        let net = Network {
            nodes: vec![
                Node::inlet("N1", 100.0, 105.0, 1.0, 0.7),
                Node::outfall("OUT", 100.0, 105.0),
            ],
            pipes: vec![Pipe::new("P1", "N1", "OUT", 100.0, 1.5, 0.013)],
        };
        let opts = AnalysisOptions {
            intensity_override: Some(4.0),
            ..Default::default()
        };
        let r = net.analyze(&IdfCurve::new(60.0, 10.0, 0.8), &opts).unwrap();
        let p = &r.pipes[0];
        assert!((p.slope).abs() < 1e-12);
        assert!((p.manning_slope - 0.001).abs() < 1e-12);
        assert!(p.capacity > 0.0);
        assert!(!p.capacity_unavailable());
    }

    #[test]
    fn small_pipe_surcharges_under_heavy_flow() {
        let r = sample().analyze_rational(4.0).unwrap();
        let p1 = r.iter().find(|x| x.id == "P1").unwrap();
        let p2 = r.iter().find(|x| x.id == "P2").unwrap();
        assert!(!p1.surcharged, "P1 should fit");
        assert!(p1.normal_depth.is_some());
        assert!(p2.surcharged, "P2 should surcharge");
        assert!(p2.normal_depth.is_none());
    }

    #[test]
    fn cycle_is_rejected() {
        let net = Network {
            nodes: vec![Node::junction("A", 10.0, 20.0, 0.0, 0.0), Node::junction("B", 9.0, 19.0, 0.0, 0.0)],
            pipes: vec![Pipe::new("AB", "A", "B", 50.0, 1.0, 0.013), Pipe::new("BA", "B", "A", 50.0, 1.0, 0.013)],
        };
        assert_eq!(net.accumulate_ca().unwrap_err(), NetworkError::CyclicNetwork);
    }

    #[test]
    fn unknown_node_is_rejected() {
        let net = Network {
            nodes: vec![Node::outfall("OUT", 98.0, 103.0)],
            pipes: vec![Pipe::new("P", "GHOST", "OUT", 100.0, 1.5, 0.013)],
        };
        assert!(matches!(net.accumulate_ca().unwrap_err(), NetworkError::UnknownNode(_)));
    }

    #[test]
    fn tc_accumulates_downstream() {
        // Downstream pipe's Tc must include upstream inlet time + travel time.
        let idf = IdfCurve::new(120.0, 10.0, 0.8);
        let a = sample().analyze(&idf, &AnalysisOptions::default()).unwrap();
        let p1 = a.pipes.iter().find(|x| x.id == "P1").unwrap();
        let p2 = a.pipes.iter().find(|x| x.id == "P2").unwrap();
        assert!(p2.tc >= p1.tc, "p2.tc {} p1.tc {}", p2.tc, p1.tc);
        // Intensity falls as Tc grows downstream.
        assert!(p2.intensity <= p1.intensity, "i2 {} i1 {}", p2.intensity, p1.intensity);
    }

    #[test]
    fn analyze_all_return_periods() {
        use crate::hydrology::IdfSet;
        let mut idf_set = IdfSet::default();
        idf_set.set_curve(25, IdfCurve::new(90.0, 12.0, 0.8));
        let results = sample().analyze_all_rps(&idf_set, &AnalysisOptions::default()).unwrap();
        assert_eq!(results.len(), 2);
        let q10 = results.iter().find(|(rp, _)| *rp == 10).unwrap().1.pipes[1].design_q;
        let q25 = results.iter().find(|(rp, _)| *rp == 25).unwrap().1.pipes[1].design_q;
        assert!(q25 > q10);
    }

    #[test]
    fn hgl_rises_upstream() {
        // With a tailwater, HGL must be monotonically higher going upstream.
        let idf = IdfCurve::new(120.0, 10.0, 0.8);
        let opts = AnalysisOptions { tailwater: Some(100.0), ..Default::default() };
        let a = sample().analyze(&idf, &opts).unwrap();
        let h = |id: &str| a.nodes.iter().find(|n| n.id == id).unwrap().hgl;
        assert!(h("OUT") <= h("N2"), "OUT {} N2 {}", h("OUT"), h("N2"));
        assert!(h("N2") <= h("N1"), "N2 {} N1 {}", h("N2"), h("N1"));
        assert!((h("OUT") - 100.0).abs() < 1e-9, "tailwater seeded");
    }
}
