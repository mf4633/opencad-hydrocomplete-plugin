// SPDX-License-Identifier: GPL-3.0-only

//! Design-standard review of an *analyzed* storm-sewer network.
//!
//! Unlike the host plugin's `HC_VALIDATE` integrity checks (XDATA well-formed,
//! handles resolve), this flags engineering-criteria violations that only show
//! up once the network is analyzed: velocity outside the self-cleansing/scour
//! band, capacity/surcharge, suspect slopes, insufficient cover, pipe-size
//! reductions in the downstream direction, and HGL surcharging to the surface.

use std::collections::HashMap;

use crate::network::{Analysis, Network, Node, Pipe};

/// Severity of a design-review finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

/// A single design-criteria finding, tied to the offending pipe or node id.
#[derive(Clone, Debug, PartialEq)]
pub struct DesignFinding {
    pub severity: Severity,
    pub id: String,
    pub message: String,
}

impl DesignFinding {
    fn warn(id: &str, message: String) -> Self {
        Self { severity: Severity::Warning, id: id.to_string(), message }
    }
    fn error(id: &str, message: String) -> Self {
        Self { severity: Severity::Error, id: id.to_string(), message }
    }
}

/// Agency-style review thresholds. Defaults follow common municipal/DOT storm
/// criteria (and mirror [`super::DesignCriteria`] for the velocity band).
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewCriteria {
    /// Minimum design velocity (ft/s) for self-cleansing.
    pub min_velocity: f64,
    /// Maximum design velocity (ft/s) before scour/erosion concern.
    pub max_velocity: f64,
    /// Maximum design flow as a fraction of just-full capacity before warning.
    pub max_pct_full: f64,
    /// Minimum cover (ft) from ground (rim) to pipe crown.
    pub min_cover_ft: f64,
    /// Slopes below this (ft/ft) are flagged as suspiciously flat.
    pub min_slope: f64,
    /// Warn when a pipe is smaller than an upstream pipe feeding the same node.
    pub check_size_progression: bool,
}

impl Default for ReviewCriteria {
    fn default() -> Self {
        Self {
            min_velocity: 2.0,
            max_velocity: 10.0,
            max_pct_full: 0.85,
            min_cover_ft: 1.0,
            min_slope: 0.0005,
            check_size_progression: true,
        }
    }
}

/// Review an analyzed network against design criteria. Pure: takes the network
/// and its [`Analysis`], returns findings (empty = passes).
pub fn design_review(net: &Network, analysis: &Analysis, c: &ReviewCriteria) -> Vec<DesignFinding> {
    let mut out = Vec::new();

    let nodes: HashMap<&str, &Node> = net.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let pipes: HashMap<&str, &Pipe> = net.pipes.iter().map(|p| (p.id.as_str(), p)).collect();

    for pr in &analysis.pipes {
        let id = pr.id.as_str();

        // Slope: adverse (uphill) is an error; near-flat is a warning.
        if pr.slope < 0.0 {
            out.push(DesignFinding::error(
                id,
                format!("Pipe {id}: adverse slope {:.4} ft/ft (runs uphill)", pr.slope),
            ));
        } else if pr.slope < c.min_slope {
            out.push(DesignFinding::warn(
                id,
                format!("Pipe {id}: very flat slope {:.4} ft/ft (< {:.4})", pr.slope, c.min_slope),
            ));
        }

        // Capacity: surcharge is an error; high % full is a warning.
        if pr.surcharged {
            out.push(DesignFinding::error(
                id,
                format!(
                    "Pipe {id}: surcharged — design Q {:.2} cfs exceeds capacity {:.2} cfs",
                    pr.design_q, pr.capacity
                ),
            ));
        } else if pr.capacity > 0.0 && pr.design_q / pr.capacity > c.max_pct_full {
            out.push(DesignFinding::warn(
                id,
                format!(
                    "Pipe {id}: {:.0}% of full capacity (> {:.0}%)",
                    100.0 * pr.design_q / pr.capacity,
                    100.0 * c.max_pct_full
                ),
            ));
        }

        // Velocity band (only meaningful when the pipe actually carries flow).
        if pr.design_q > 0.0 {
            if pr.velocity < c.min_velocity {
                out.push(DesignFinding::warn(
                    id,
                    format!(
                        "Pipe {id}: velocity {:.1} ft/s below self-cleansing min {:.1} ft/s",
                        pr.velocity, c.min_velocity
                    ),
                ));
            } else if pr.velocity > c.max_velocity {
                out.push(DesignFinding::warn(
                    id,
                    format!(
                        "Pipe {id}: velocity {:.1} ft/s exceeds max {:.1} ft/s (scour)",
                        pr.velocity, c.max_velocity
                    ),
                ));
            }
        }

        // Cover at each end: rim - (invert + diameter) = ground to crown.
        if let Some(p) = pipes.get(id) {
            for (end, nid) in [("upstream", p.from.as_str()), ("downstream", p.to.as_str())] {
                if let Some(nd) = nodes.get(nid) {
                    let cover = nd.rim - (nd.invert + p.diameter);
                    if cover < c.min_cover_ft {
                        out.push(DesignFinding::warn(
                            id,
                            format!(
                                "Pipe {id}: {cover:.2} ft cover at {end} node {nid} (< {:.2} ft min)",
                                c.min_cover_ft
                            ),
                        ));
                    }
                }
            }
        }
    }

    // Pipe-size progression: a pipe leaving a node must not be smaller than any
    // pipe entering it (storm trunks don't reduce diameter downstream).
    if c.check_size_progression {
        let mut out_by_node: HashMap<&str, Vec<&Pipe>> = HashMap::new();
        for p in &net.pipes {
            out_by_node.entry(p.from.as_str()).or_default().push(p);
        }
        for up in &net.pipes {
            if let Some(downs) = out_by_node.get(up.to.as_str()) {
                for down in downs {
                    if down.diameter + 1e-9 < up.diameter {
                        out.push(DesignFinding::warn(
                            &down.id,
                            format!(
                                "Pipe {}: diameter {:.2} ft is smaller than upstream pipe {} ({:.2} ft) at node {}",
                                down.id, down.diameter, up.id, up.diameter, up.to
                            ),
                        ));
                    }
                }
            }
        }
    }

    // HGL surcharging above the rim = surface flooding.
    for nr in &analysis.nodes {
        if nr.surcharge_to_surface {
            out.push(DesignFinding::error(
                &nr.id,
                format!(
                    "Node {}: HGL {:.2} ft surcharges above rim {:.2} ft (flooding)",
                    nr.id, nr.hgl, nr.rim
                ),
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idf::IdfCurve;
    use crate::network::{AnalysisOptions, Network, Node, Pipe};

    fn analyze(net: &Network) -> Analysis {
        let idf = IdfCurve::new(60.0, 10.0, 0.8);
        let opts = AnalysisOptions::default();
        net.analyze(&idf, &opts).expect("analyze")
    }

    fn has(findings: &[DesignFinding], sev: Severity, needle: &str) -> bool {
        findings
            .iter()
            .any(|f| f.severity == sev && f.message.contains(needle))
    }

    #[test]
    fn flags_pipe_size_decrease_downstream() {
        // 24" feeds an 18" at N2 — diameter drops downstream.
        let net = Network {
            nodes: vec![
                Node::inlet("N1", 100.0, 110.0, 1.0, 0.7).at(0.0, 0.0),
                Node::junction("N2", 99.0, 109.0, 0.5, 0.7).at(200.0, 0.0),
                Node::outfall("OUT", 98.0, 108.0).at(400.0, 0.0),
            ],
            pipes: vec![
                Pipe::new("P1", "N1", "N2", 200.0, 2.0, 0.013),
                Pipe::new("P2", "N2", "OUT", 200.0, 1.5, 0.013),
            ],
        };
        let a = analyze(&net);
        let f = design_review(&net, &a, &ReviewCriteria::default());
        assert!(has(&f, Severity::Warning, "smaller than upstream pipe"));
    }

    #[test]
    fn flags_adverse_slope() {
        // Outfall invert higher than the inlet — pipe runs uphill.
        let net = Network {
            nodes: vec![
                Node::inlet("N1", 98.0, 110.0, 1.0, 0.7).at(0.0, 0.0),
                Node::outfall("OUT", 100.0, 112.0).at(200.0, 0.0),
            ],
            pipes: vec![Pipe::new("P1", "N1", "OUT", 200.0, 2.0, 0.013)],
        };
        let a = analyze(&net);
        let f = design_review(&net, &a, &ReviewCriteria::default());
        assert!(has(&f, Severity::Error, "adverse slope"));
    }

    #[test]
    fn flags_insufficient_cover() {
        // Rim only 1.0 ft above invert with a 1.5 ft pipe -> negative cover.
        let net = Network {
            nodes: vec![
                Node::inlet("N1", 100.0, 101.0, 1.0, 0.7).at(0.0, 0.0),
                Node::outfall("OUT", 99.0, 108.0).at(200.0, 0.0),
            ],
            pipes: vec![Pipe::new("P1", "N1", "OUT", 200.0, 1.5, 0.013)],
        };
        let a = analyze(&net);
        let f = design_review(&net, &a, &ReviewCriteria::default());
        assert!(has(&f, Severity::Warning, "cover at upstream node N1"));
    }

    #[test]
    fn well_formed_network_has_no_errors() {
        // Properly graded, non-decreasing diameters, deep cover.
        let net = Network {
            nodes: vec![
                Node::inlet("N1", 104.0, 110.0, 0.5, 0.7).at(0.0, 0.0),
                Node::junction("N2", 102.5, 109.0, 0.3, 0.7).at(300.0, 0.0),
                Node::outfall("OUT", 101.0, 108.0).at(550.0, 0.0),
            ],
            pipes: vec![
                Pipe::new("P1", "N1", "N2", 300.0, 1.5, 0.013),
                Pipe::new("P2", "N2", "OUT", 250.0, 1.75, 0.013),
            ],
        };
        let a = analyze(&net);
        let f = design_review(&net, &a, &ReviewCriteria::default());
        assert!(
            !f.iter().any(|x| x.severity == Severity::Error),
            "unexpected errors: {f:?}"
        );
    }
}
