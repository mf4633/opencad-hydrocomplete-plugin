// SPDX-License-Identifier: GPL-3.0-only

//! Plain-text report tables for an [`Analysis`] and pipe-sizing output, in the
//! spirit of Hydraflow HydroCompletes' pipe and HGL summaries.

use crate::design::PipeSizeRecommendation;
use crate::hydrology::IdfSet;
use crate::network::{Analysis, AnalysisOptions, Network};

fn f(x: f64, w: usize, p: usize) -> String {
    format!("{:>w$.p$}", x, w = w, p = p)
}

/// Format the pipe hydraulics table.
pub fn pipe_table(a: &Analysis) -> String {
    let mut s = String::new();
    s.push_str(
        "Pipe   From   To     Slope    Tc    i      Q      Cap    %Full  V      yn     HGLup    HGLdn    Status\n",
    );
    s.push_str(
        "                     ft/ft    min   in/hr  cfs    cfs    %      ft/s   ft     ft       ft\n",
    );
    s.push_str(&"-".repeat(110));
    s.push('\n');
    for p in &a.pipes {
        let yn = p.normal_depth.map(|y| f(y, 6, 2)).unwrap_or_else(|| "  full".into());
        let hup = p.hgl_up.map(|h| f(h, 8, 2)).unwrap_or_else(|| "      --".into());
        let hdn = p.hgl_dn.map(|h| f(h, 8, 2)).unwrap_or_else(|| "      --".into());
        let status = if p.surcharged { "SURCHARGED" } else { "ok" };
        s.push_str(&format!(
            "{:<6} {:<6} {:<6} {} {} {} {} {} {} {} {} {} {}  {}\n",
            p.id,
            p.from,
            p.to,
            f(p.slope, 7, 4),
            f(p.tc, 5, 1),
            f(p.intensity, 6, 2),
            f(p.design_q, 6, 2),
            f(p.capacity, 6, 2),
            f(p.pct_full * 100.0, 6, 1),
            f(p.velocity, 6, 2),
            yn,
            hup,
            hdn,
            status,
        ));
    }
    s
}

/// Format the node / HGL table.
pub fn node_table(a: &Analysis) -> String {
    let mut s = String::new();
    s.push_str("Node   Tc(min)  Rim(ft)   HGL(ft)   Freeboard  Status\n");
    s.push_str(&"-".repeat(60));
    s.push('\n');
    for n in &a.nodes {
        let fb = n.rim - n.hgl;
        let status = if n.surcharge_to_surface { "FLOODING" } else { "ok" };
        s.push_str(&format!(
            "{:<6} {} {} {} {}  {}\n",
            n.id,
            f(n.tc, 7, 1),
            f(n.rim, 8, 2),
            f(n.hgl, 8, 2),
            f(fb, 9, 2),
            status,
        ));
    }
    s
}

fn dia_in(d_ft: f64) -> String {
    format!("{}\"", (d_ft * 12.0).round() as i32)
}

/// Pipe-sizing recommendations table.
pub fn sizing_table(recs: &[PipeSizeRecommendation]) -> String {
    let mut s = String::new();
    s.push_str("Pipe   Q(cfs)  Slope    Current  Rec'd    %Full  V(ft/s)  Status\n");
    s.push_str(&"-".repeat(72));
    s.push('\n');
    for r in recs {
        let status = match r.outcome {
            crate::design::SizeOutcome::Adequate => "ok",
            crate::design::SizeOutcome::Sized => "UPSIZE",
            crate::design::SizeOutcome::NoSolution => "NO SIZE",
        };
        s.push_str(&format!(
            "{:<6} {} {} {} {} {} {}  {}\n",
            r.pipe_id,
            f(r.design_q, 6, 2),
            f(r.slope, 7, 4),
            format!("{:>6}", dia_in(r.current_diameter_ft)),
            format!("{:>6}", dia_in(r.recommended_diameter_ft)),
            f(r.pct_full * 100.0, 6, 1),
            f(r.velocity, 6, 2),
            status,
        ));
    }
    s
}

/// Full sizing report with per-pipe notes.
pub fn format_sizing(recs: &[PipeSizeRecommendation]) -> String {
    let mut s = String::new();
    s.push_str("=== HydroComplete PIPE SIZING ===\n\n");
    s.push_str(&sizing_table(recs));
    s.push('\n');
    for r in recs {
        s.push_str(&r.note);
        s.push('\n');
    }
    let upsized: Vec<&str> = recs
        .iter()
        .filter(|r| r.outcome == crate::design::SizeOutcome::Sized)
        .map(|r| r.pipe_id.as_str())
        .collect();
    let failed: Vec<&str> = recs
        .iter()
        .filter(|r| r.outcome == crate::design::SizeOutcome::NoSolution)
        .map(|r| r.pipe_id.as_str())
        .collect();
    if upsized.is_empty() && failed.is_empty() {
        s.push_str("\nAll pipes meet design criteria.\n");
    } else {
        if !upsized.is_empty() {
            s.push_str(&format!("\nPipes to upsize: {}\n", upsized.join(", ")));
        }
        if !failed.is_empty() {
            s.push_str(&format!("Pipes with no catalog solution: {}\n", failed.join(", ")));
        }
    }
    s
}

/// Summary of peak design flows at each configured return period.
pub fn format_multi_rp(net: &Network, idf_set: &IdfSet, opts: &AnalysisOptions) -> String {
    let mut s = String::new();
    s.push_str("=== MULTI RETURN-PERIOD PEAK FLOWS ===\n\n");
    s.push_str("RP(yr)  Pipe   Q(cfs)   Surcharged\n");
    s.push_str(&"-".repeat(40));
    s.push('\n');
    match net.analyze_all_rps(idf_set, opts) {
        Ok(runs) => {
            for (rp, a) in runs {
                for p in &a.pipes {
                    let flag = if p.surcharged { "yes" } else { "no" };
                    s.push_str(&format!("{rp:<7} {:<6} {}  {flag}\n", p.id, f(p.design_q, 6, 2)));
                }
                s.push('\n');
            }
        }
        Err(e) => s.push_str(&format!("error: {e}\n")),
    }
    s
}

/// Full report: pipe table followed by node/HGL table.
pub fn format_analysis(a: &Analysis) -> String {
    let mut s = String::new();
    s.push_str("=== HydroComplete ANALYSIS ===\n\n");
    s.push_str(&pipe_table(a));
    s.push('\n');
    s.push_str(&node_table(a));
    // Summary flags.
    let surcharged: Vec<&str> = a
        .pipes
        .iter()
        .filter(|p| p.report_surcharged())
        .map(|p| p.id.as_str())
        .collect();
    let flooding: Vec<&str> = a.nodes.iter().filter(|n| n.surcharge_to_surface).map(|n| n.id.as_str()).collect();
    s.push('\n');
    if surcharged.is_empty() && flooding.is_empty() {
        s.push_str("All pipes flow open-channel; no surface flooding.\n");
    } else {
        if !surcharged.is_empty() {
            s.push_str(&format!("Surcharged pipes: {}\n", surcharged.join(", ")));
        }
        if !flooding.is_empty() {
            s.push_str(&format!("Structures flooding (HGL > rim): {}\n", flooding.join(", ")));
        }
    }
    s
}
