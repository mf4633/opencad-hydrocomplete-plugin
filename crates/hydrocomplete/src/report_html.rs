//! KaTeX-enabled HTML hydraulic reports (Manning capacity, design check, steady HGL).
//!
//! Mirrors `HydroComplete.Civil3D.Writing.HtmlReportWriter` structure for OpenCAD.

use std::collections::HashMap;

use stormsewer::hydraulics::{circular_geometry, full_area, G_US, K_MANNING_US};
use stormsewer::network::{Analysis, Network, NodeKind, Pipe};
use stormsewer::params::StormAnalysisParams;

/// Metadata for the HTML report header.
#[derive(Clone, Debug)]
pub struct HtmlReportMeta {
    pub title: String,
    pub drawing_name: String,
    pub generated_local: String,
}

impl Default for HtmlReportMeta {
    fn default() -> Self {
        Self {
            title: "HydroComplete Report".into(),
            drawing_name: "drawing".into(),
            generated_local: String::new(),
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn f(x: f64, prec: usize) -> String {
    format!("{x:.prec$}", prec = prec)
}

fn trim(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('~');
        t
    }
}

fn escape_latex_identifier(label: &str) -> String {
    label
        .replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('_', "\\_")
}

fn escape_latex_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

fn try_map_formula_to_latex(formula: &str) -> Option<&'static str> {
    match formula.trim() {
        "(1.486/n)*A*R^(2/3)*S^(1/2)" => Some(r"Q = \frac{1.486}{n} A R^{2/3} S^{1/2}"),
        "pi*D^2/4" => Some(r"A = \frac{\pi D^2}{4}"),
        "D/4" => Some(r"R = \frac{D}{4}"),
        "Q_full/A_full" => Some(r"V = \frac{Q_{\text{full}}}{A_{\text{full}}}"),
        "[n*Q/(1.486*A*R^(2/3))]^2" => Some(r"S_f = \left[\frac{n Q}{1.486\, A\, R^{2/3}}\right]^2"),
        "S_f*L" => Some(r"h_f = S_f L"),
        "K*Vh" => Some(r"h_m = K \cdot V_h"),
        "Q = C*i*A" => Some(r"Q = C \cdot i \cdot A"),
        "i = a/(t+b)^c" => Some(r"i = \frac{a}{(t+b)^c}"),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct CalcStep {
    label: String,
    formula: String,
    value: f64,
    units: String,
}

impl CalcStep {
    fn new(label: impl Into<String>, formula: impl Into<String>, value: f64, units: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            formula: formula.into(),
            value,
            units: units.into(),
        }
    }
}

fn format_result_latex(step: &CalcStep) -> String {
    let units = if step.units.is_empty() {
        String::new()
    } else {
        format!("\\ \\text{{{}}}", escape_latex_text(&step.units))
    };
    format!(
        "{} = {:.4}{}",
        escape_latex_identifier(&step.label),
        step.value,
        units
    )
}

fn render_calc_step_html(step: &CalcStep) -> String {
    let latex = try_map_formula_to_latex(&step.formula);
    let result_latex = format_result_latex(step);
    let mut block = String::new();
    block.push_str(&format!(
        r#"<div class="hc-formula-step" data-label="{}">"#,
        esc(&step.label)
    ));
    block.push_str(&format!(
        r#"<div class="hc-formula-title">{}</div>"#,
        esc(&step.label)
    ));
    if let Some(eq) = latex {
        block.push_str(&format!(
            r#"<div class="hc-formula-equation"><code class="hc-tex-fallback">{}</code></div>"#,
            esc(eq)
        ));
        block.push_str(&format!(
            r#"<div class="hc-formula-result"><code class="hc-tex-fallback">{}</code></div>"#,
            esc(&result_latex)
        ));
    } else {
        block.push_str(&format!(
            r#"<div class="hc-formula-desc">{}</div>"#,
            esc(&format!("{} = {:.4} {}", step.label, step.value, step.units))
        ));
        if !step.formula.is_empty() {
            block.push_str(&format!(
                r#"<div class="hc-formula-desc">{}</div>"#,
                esc(&step.formula)
            ));
        }
    }
    block.push_str("</div>");
    block
}

fn append_calc_steps(out: &mut String, steps: &[CalcStep], heading: Option<&str>) {
    if steps.is_empty() {
        return;
    }
    if let Some(h) = heading {
        out.push_str(&format!("<h3>{}</h3>", esc(h)));
    }
    out.push_str(r#"<div class="hc-formula-panel">"#);
    for step in steps {
        out.push_str(&render_calc_step_html(step));
    }
    out.push_str("</div>");
}

fn append_html_head(out: &mut String, page_title: &str) {
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"/>\n");
    out.push_str(&format!("<title>{}</title>\n", esc(page_title)));
    out.push_str(
        r#"<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.8/dist/katex.min.css">"#,
    );
    out.push_str(
        r#"<script src="https://cdn.jsdelivr.net/npm/katex@0.16.8/dist/katex.min.js"></script>"#,
    );
    out.push_str(
        r#"<script src="https://cdn.jsdelivr.net/npm/katex@0.16.8/dist/contrib/auto-render.min.js"></script>"#,
    );
    append_report_css(out);
    out.push_str("</head><body>\n");
}

fn append_report_css(out: &mut String) {
    out.push_str(
        r#"<style>
body{font-family:Segoe UI,Arial,sans-serif;margin:24px;color:#1a1a1a;}
h1{font-size:1.4rem;} h2{font-size:1.15rem;margin-top:28px;} h3{font-size:1rem;margin-top:16px;}
h4{font-size:0.95rem;margin-top:12px;} h5{font-size:0.9rem;margin-top:8px;}
table{border-collapse:collapse;width:100%;margin:16px 0;}
th,td{border:1px solid #ccc;padding:6px 8px;text-align:left;font-size:0.9rem;}
th{background:#f0f4f8;} tr.surcharged{background:#ffe6e6;}
.disclaimer{margin-top:24px;padding:12px;background:#fff8e6;border:1px solid #e6c200;}
.hc-formula-panel{margin:12px 0;}
.hc-formula-step{border:1px solid #e0e6ed;border-radius:6px;padding:10px 12px;margin:8px 0;background:#fafbfc;}
.hc-formula-title{font-weight:600;font-size:0.95rem;margin-bottom:6px;}
.hc-formula-equation,.hc-formula-result{margin:4px 0;}
.hc-formula-desc{font-family:Consolas,monospace;font-size:0.85rem;color:#444;}
.hc-tex-fallback{font-family:Consolas,monospace;font-size:0.9rem;}
.pass{color:#0a7a2f;font-weight:600;} .failtxt{color:#b00020;font-weight:600;}
</style>"#,
    );
}

const KATEX_REHYDRATION: &str = r#"<script>
(function rehydrateKaTeX() {
  if (typeof katex === 'undefined') return setTimeout(rehydrateKaTeX, 50);
  document.querySelectorAll('code.hc-tex-fallback').forEach(function(el) {
    var latex = el.textContent;
    try {
      var span = document.createElement('span');
      katex.render(latex, span, {
        displayMode: el.closest('.hc-formula-equation, .hc-formula-result') !== null,
        throwOnError: false,
        strict: false
      });
      el.replaceWith(span);
    } catch (e) {}
  });
  if (typeof renderMathInElement !== 'undefined') {
    renderMathInElement(document.body, {
      delimiters: [
        { left: '$$', right: '$$', display: true },
        { left: '\\(', right: '\\)', display: false }
      ],
      throwOnError: false
    });
  }
})();
</script>"#;

fn append_html_foot(out: &mut String) {
    out.push_str(KATEX_REHYDRATION);
    out.push_str("</body></html>\n");
}

fn pipe_by_id<'a>(net: &'a Network) -> HashMap<&'a str, &'a Pipe> {
    net.pipes.iter().map(|p| (p.id.as_str(), p)).collect()
}

fn manning_steps(pipe: &Pipe, pr: &stormsewer::network::PipeResult) -> Vec<CalcStep> {
    let d = pipe.diameter;
    let n = pipe.n;
    let s = pr.slope;
    let area = full_area(d);
    let radius = d / 4.0;
    let q_full = pr.capacity;
    let v_full = pr.velocity_full;
    vec![
        CalcStep::new("A", "pi*D^2/4", area, "ft^2"),
        CalcStep::new("R", "D/4", radius, "ft"),
        CalcStep::new("Q_full", "(1.486/n)*A*R^(2/3)*S^(1/2)", q_full, "cfs"),
        CalcStep::new("V_full", "Q_full/A_full", v_full, "ft/s"),
        CalcStep::new("n", "Manning roughness", n, ""),
        CalcStep::new("S", "Pipe slope", s, "ft/ft"),
    ]
}

fn hgl_steps(
    pipe: &Pipe,
    pr: &stormsewer::network::PipeResult,
    junction_k: f64,
) -> Vec<CalcStep> {
    let d = pipe.diameter;
    let q = pr.design_q;
    let n = pipe.n;
    let length = pipe.length;
    let hm = junction_k * pr.velocity.powi(2) / (2.0 * G_US);
    let (area, _, radius, _) = if pr.surcharged {
        (full_area(d), 0.0, d / 4.0, 0.0)
    } else {
        let y = pr.normal_depth.unwrap_or(d);
        circular_geometry(y, d)
    };
    let conv = if n > 0.0 {
        K_MANNING_US / n * area * radius.powf(2.0 / 3.0)
    } else {
        0.0
    };
    let sf = if conv > 0.0 {
        (q / conv).powi(2)
    } else {
        0.0
    };
    let hf = if let (Some(up), Some(dn)) = (pr.hgl_up, pr.hgl_dn) {
        (up - dn - hm).max(0.0)
    } else {
        sf * length
    };
    vec![
        CalcStep::new("Q", "Design flow", q, "cfs"),
        CalcStep::new("S_f", "[n*Q/(1.486*A*R^(2/3))]^2", sf, "ft/ft"),
        CalcStep::new("h_f", "S_f*L", hf, "ft"),
        CalcStep::new("h_m", "K*Vh", hm, "ft"),
        CalcStep::new("K", "Junction loss coefficient", junction_k, ""),
    ]
}

fn append_manning_section(out: &mut String, net: &Network, a: &Analysis) {
    let pipes = pipe_by_id(net);
    out.push_str("<h2>Manning Pipe Capacity</h2>\n");
    out.push_str(
        "<p>Method: Manning full-barrel capacity for circular pipes (US customary, n=0.013 default).</p>\n",
    );
    out.push_str("<table><thead><tr>\n");
    out.push_str("<th>Network / Pipe</th><th>Dia (ft)</th><th>Slope</th><th>Q<sub>full</sub> (cfs)</th><th>V<sub>full</sub> (fps)</th>\n");
    out.push_str("</tr></thead><tbody>\n");
    for pr in &a.pipes {
        let Some(pipe) = pipes.get(pr.id.as_str()) else {
            continue;
        };
        out.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            esc(&trim(&format!("Network/{}", pr.id), 48)),
            f(pipe.diameter, 2),
            f(pr.slope, 4),
            f(pr.capacity, 2),
            f(pr.velocity_full, 2),
        ));
    }
    out.push_str("</tbody></table>\n");
    out.push_str("<h3>Manning calculation steps</h3>\n");
    for pr in &a.pipes {
        let Some(pipe) = pipes.get(pr.id.as_str()) else {
            continue;
        };
        out.push_str(&format!(
            "<h4>{}</h4><div class=\"steps\">\n",
            esc(&trim(&format!("Network/{}", pr.id), 64))
        ));
        append_calc_steps(out, &manning_steps(pipe, pr), None);
        out.push_str("</div>\n");
    }
}

fn append_capacity_section(out: &mut String, net: &Network, a: &Analysis, design_flow_cfs: f64) {
    let pipes = pipe_by_id(net);
    out.push_str("<h2>Design Capacity Check</h2>\n");
    out.push_str(&format!(
        "<p>Method: Manning normal depth at per-pipe routed catchment Q (system total = <strong>{} cfs</strong>). \
         Surcharge when Q exceeds peak open-channel capacity (d/D &rarr; 1.0).</p>\n",
        f(design_flow_cfs, 2)
    ));
    out.push_str("<table><thead><tr>\n");
    out.push_str("<th>Network / Pipe</th><th>Q<sub>full</sub> (cfs)</th><th>Q<sub>des</sub> (cfs)</th>");
    out.push_str("<th>Q<sub>des</sub>/Q<sub>full</sub></th><th>d/D</th><th>SURCH</th>\n");
    out.push_str("</tr></thead><tbody>\n");
    for pr in &a.pipes {
        let row_class = if pr.surcharged {
            r#" class="surcharged""#
        } else {
            ""
        };
        let flow_ratio = if pr.capacity > 0.0 {
            pr.design_q / pr.capacity
        } else {
            0.0
        };
        let d_over_d = if pr.surcharged {
            "SURCH".to_string()
        } else {
            pipes
                .get(pr.id.as_str())
                .and_then(|pipe| {
                    pr.normal_depth
                        .map(|y| format!("{:.2}", y / pipe.diameter))
                })
                .unwrap_or_else(|| "1.00".into())
        };
        out.push_str(&format!(
            "<tr{row_class}><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            esc(&trim(&format!("Network/{}", pr.id), 48)),
            f(pr.capacity, 1),
            f(pr.design_q, 1),
            f(flow_ratio, 2),
            esc(&d_over_d),
            if pr.surcharged { "*" } else { "" },
        ));
    }
    out.push_str("</tbody></table>\n");
}

fn system_design_flow(a: &Analysis) -> f64 {
    a.pipes.iter().map(|p| p.design_q).fold(0.0f64, f64::max)
}

fn outfall_tailwater(net: &Network, params: &StormAnalysisParams) -> f64 {
    if let Some(tw) = params.hydraulics.tailwater {
        return tw;
    }
    net.nodes
        .iter()
        .find(|n| n.kind == NodeKind::Outfall)
        .map(|n| n.invert)
        .unwrap_or(0.0)
}

fn append_hgl_section(out: &mut String, net: &Network, a: &Analysis, params: &StormAnalysisParams) {
    let design_q = system_design_flow(a);
    let tailwater = outfall_tailwater(net, params);
    let pipes = pipe_by_id(net);
    let loss_note = " with HEC-22 junction/exit losses";
    out.push_str("<h2>Steady HGL Profile</h2>\n");
    out.push_str(&format!(
        "<p>Method: steady uniform-flow stepping downstream from headwater HGL \
         using Manning normal depth per reach (partial-flow A and R){loss_note}. \
         (S<sub>f</sub> = [n&middot;Q/(1.486&middot;A&middot;R<sup>2/3</sup>)]<sup>2</sup>, \
         h<sub>f</sub> = S<sub>f</sub>&middot;L, h<sub>m</sub> = K&middot;Vh). \
         design Q = <strong>{} cfs</strong>.</p>\n",
        f(design_q, 2)
    ));
    out.push_str(&format!(
        "<h3>Network</h3><p>Outfall tailwater HGL = {} ft (profile stepped upstream from the outfall, adding friction + HEC-22 minor losses).</p>\n",
        f(tailwater, 2)
    ));
    out.push_str("<table><thead><tr>\n");
    out.push_str("<th>Pipe</th><th>d/D</th><th>h<sub>f</sub> (ft)</th><th>h<sub>m</sub> (ft)</th>");
    out.push_str("<th>HGL<sub>US</sub> (ft)</th><th>HGL<sub>DS</sub> (ft)</th><th>SURCH</th>\n");
    out.push_str("</tr></thead><tbody>\n");
    for pr in &a.pipes {
        let Some(pipe) = pipes.get(pr.id.as_str()) else {
            continue;
        };
        let hm = params.hydraulics.junction_k * pr.velocity.powi(2) / (2.0 * G_US);
        let hf = match (pr.hgl_up, pr.hgl_dn) {
            (Some(up), Some(dn)) => (up - dn - hm).max(0.0),
            _ => 0.0,
        };
        let row_class = if pr.surcharged {
            r#" class="surcharged""#
        } else {
            ""
        };
        let d_over_d = if pr.surcharged {
            "SURCH".to_string()
        } else {
            pr.normal_depth
                .map(|y| format!("{:.2}", y / pipe.diameter))
                .unwrap_or_else(|| "--".into())
        };
        let hup = pr.hgl_up.map(|h| f(h, 2)).unwrap_or_else(|| "--".into());
        let hdn = pr.hgl_dn.map(|h| f(h, 2)).unwrap_or_else(|| "--".into());
        out.push_str(&format!(
            "<tr{row_class}><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            esc(&trim(&pr.id, 48)),
            esc(&d_over_d),
            f(hf, 2),
            f(hm, 2),
            esc(&hup),
            esc(&hdn),
            if pr.surcharged { "*" } else { "" },
        ));
    }
    out.push_str("</tbody></table>\n");
    out.push_str("<h4>HGL calculation steps</h4>\n");
    for pr in &a.pipes {
        let Some(pipe) = pipes.get(pr.id.as_str()) else {
            continue;
        };
        out.push_str(&format!(
            "<h5>{}</h5><div class=\"steps\">\n",
            esc(&trim(&pr.id, 64))
        ));
        append_calc_steps(
            out,
            &hgl_steps(pipe, pr, params.hydraulics.junction_k),
            None,
        );
        out.push_str("</div>\n");
    }
}

/// Build a self-contained HTML document (KaTeX CDN) for Manning + capacity + HGL.
pub fn format_hydraulic_report_html(
    net: &Network,
    a: &Analysis,
    params: &StormAnalysisParams,
    meta: &HtmlReportMeta,
) -> String {
    let mut out = String::new();
    append_html_head(&mut out, &meta.title);
    out.push_str("<h1>HydroComplete — Hydraulic Report</h1>\n");
    out.push_str(&format!(
        "<p>Drawing: <strong>{}</strong><br/>Generated: {}<br/>{}</p>\n",
        esc(&meta.drawing_name),
        esc(&meta.generated_local),
        esc(&params.summary()),
    ));
    append_manning_section(&mut out, net, a);
    append_capacity_section(&mut out, net, a, system_design_flow(a));
    append_hgl_section(&mut out, net, a, params);
    out.push_str("<div class=\"disclaimer\">\n");
    out.push_str("<strong>Disclaimer:</strong> This report is generated by HydroComplete for preliminary ");
    out.push_str("storm-sewer review. Verify all inputs (diameter, slope, roughness, design flow) ");
    out.push_str("against the engineer's design basis. Not a substitute for licensed professional ");
    out.push_str("judgment or jurisdiction-specific design standards.\n");
    out.push_str("</div>\n");
    append_html_foot(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use stormsewer::idf::IdfCurve;
    use stormsewer::network::{Node, Pipe};

    fn sample() -> (Network, Analysis) {
        let net = Network {
            nodes: vec![
                Node::inlet("N1", 104.0, 110.0, 1.0, 0.70).with_tc_inlet(12.0),
                Node::outfall("OUT", 100.0, 106.0),
            ],
            pipes: vec![Pipe::new("P1", "N1", "OUT", 200.0, 1.25, 0.013)],
        };
        let idf = IdfCurve::new(60.0, 10.0, 0.8);
        let a = net.analyze(&idf, &Default::default()).unwrap();
        (net, a)
    }

    #[test]
    fn html_has_manning_hgl_capacity_sections() {
        let (net, a) = sample();
        let html = format_hydraulic_report_html(
            &net,
            &a,
            &StormAnalysisParams::default(),
            &HtmlReportMeta {
                title: "HydroComplete Report".into(),
                drawing_name: "test.dwg".into(),
                generated_local: "2026-06-22 12:00:00".into(),
            },
        );
        assert!(html.contains("katex@0.16.8"));
        assert!(html.contains("Manning Pipe Capacity"));
        assert!(html.contains("Design Capacity Check"));
        assert!(html.contains("Steady HGL Profile"));
        assert!(html.contains("hc-formula-panel"));
        assert!(html.contains("P1"));
    }
}