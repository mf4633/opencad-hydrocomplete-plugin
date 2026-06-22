// SPDX-License-Identifier: GPL-3.0-only

//! KaTeX-enabled HTML analysis reports (Hydraflow-style transparency).

use crate::design::{check_inlet_geom, InletGeometry};
use crate::network::{Analysis, Network, NodeKind};
use crate::params::StormAnalysisParams;

/// Metadata for the HTML report header.
#[derive(Clone, Debug)]
pub struct HtmlReportMeta {
    pub title: String,
    pub drawing_name: String,
    pub generated_utc: String,
}

impl Default for HtmlReportMeta {
    fn default() -> Self {
        Self {
            title: "HydroComplete Analysis Report".into(),
            drawing_name: "drawing".into(),
            generated_utc: String::new(),
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn f(x: f64, p: usize) -> String {
    format!("{x:.prec$}", prec = p)
}

fn formula_step(title: &str, equation_latex: &str, result_latex: &str) -> String {
    format!(
        r#"<div class="hc-formula-step">
<div class="hc-formula-title">{title}</div>
<div class="hc-formula-equation"><code class="hc-tex-fallback">{eq}</code></div>
<div class="hc-formula-result"><code class="hc-tex-fallback">{res}</code></div>
</div>"#,
        title = esc(title),
        eq = esc(equation_latex),
        res = esc(result_latex),
    )
}

fn append_css(out: &mut String) {
    out.push_str(
        r#"<style>
body{font-family:Segoe UI,Arial,sans-serif;margin:24px;color:#1a1a1a;}
h1{font-size:1.4rem;} h2{font-size:1.15rem;margin-top:28px;} h3{font-size:1rem;margin-top:16px;}
table{border-collapse:collapse;width:100%;margin:16px 0;}
th,td{border:1px solid #ccc;padding:6px 8px;text-align:left;font-size:0.9rem;}
th{background:#f0f4f8;} tr.surcharged{background:#ffe6e6;} tr.flooding{background:#fff0f0;}
.disclaimer{margin-top:24px;padding:12px;background:#fff8e6;border:1px solid #e6c200;font-size:0.85rem;}
.hc-formula-panel{margin:12px 0;}
.hc-formula-step{border:1px solid #e0e6ed;border-radius:6px;padding:10px 12px;margin:8px 0;background:#fafbfc;}
.hc-formula-title{font-weight:600;font-size:0.95rem;margin-bottom:6px;}
.hc-formula-equation,.hc-formula-result{margin:4px 0;}
.hc-tex-fallback{font-family:Consolas,monospace;font-size:0.9rem;}
.meta{color:#555;font-size:0.9rem;}
.pass{color:#0a7a2f;font-weight:600;} .failtxt{color:#b00020;font-weight:600;}
</style>"#,
    );
}

const KATEX_REHYDRATE: &str = r#"<script>
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
})();
</script>"#;

fn pipe_table_html(a: &Analysis) -> String {
    let mut s = String::from(
        r#"<table><thead><tr>
<th>Pipe</th><th>From</th><th>To</th><th>Slope</th><th>Tc</th><th>i</th>
<th>Q</th><th>Cap</th><th>%Full</th><th>V</th><th>yn</th><th>HGL up</th><th>HGL dn</th><th>Status</th>
</tr></thead><tbody>"#,
    );
    for p in &a.pipes {
        let row_class = if p.surcharged { r#" class="surcharged""# } else { "" };
        let yn = p
            .normal_depth
            .map(|y| f(y, 2))
            .unwrap_or_else(|| "full".into());
        let hup = p.hgl_up.map(|h| f(h, 2)).unwrap_or_else(|| "--".into());
        let hdn = p.hgl_dn.map(|h| f(h, 2)).unwrap_or_else(|| "--".into());
        let status = if p.surcharged {
            r#"<span class="failtxt">SURCHARGED</span>"#
        } else {
            r#"<span class="pass">ok</span>"#
        };
        s.push_str(&format!(
            r#"<tr{row_class}><td>{id}</td><td>{from}</td><td>{to}</td>
<td>{slope}</td><td>{tc}</td><td>{i}</td><td>{q}</td><td>{cap}</td>
<td>{pf}</td><td>{v}</td><td>{yn}</td><td>{hup}</td><td>{hdn}</td><td>{status}</td></tr>"#,
            id = esc(&p.id),
            from = esc(&p.from),
            to = esc(&p.to),
            slope = f(p.slope, 4),
            tc = f(p.tc, 1),
            i = f(p.intensity, 2),
            q = f(p.design_q, 2),
            cap = f(p.capacity, 2),
            pf = f(p.pct_full * 100.0, 1),
            v = f(p.velocity, 2),
            yn = esc(&yn),
            hup = esc(&hup),
            hdn = esc(&hdn),
            status = status,
        ));
    }
    s.push_str("</tbody></table>");
    s
}

fn node_table_html(a: &Analysis) -> String {
    let mut s = String::from(
        r#"<table><thead><tr>
<th>Node</th><th>Tc (min)</th><th>Rim (ft)</th><th>HGL (ft)</th><th>Freeboard (ft)</th><th>Status</th>
</tr></thead><tbody>"#,
    );
    for n in &a.nodes {
        let row_class = if n.surcharge_to_surface {
            r#" class="flooding""#
        } else {
            ""
        };
        let fb = n.rim - n.hgl;
        let status = if n.surcharge_to_surface {
            r#"<span class="failtxt">FLOODING</span>"#
        } else {
            r#"<span class="pass">ok</span>"#
        };
        s.push_str(&format!(
            r#"<tr{row_class}><td>{id}</td><td>{tc}</td><td>{rim}</td><td>{hgl}</td><td>{fb}</td><td>{status}</td></tr>"#,
            id = esc(&n.id),
            tc = f(n.tc, 1),
            rim = f(n.rim, 2),
            hgl = f(n.hgl, 2),
            fb = f(fb, 2),
            status = status,
        ));
    }
    s.push_str("</tbody></table>");
    s
}

fn formula_panel(net: &Network, a: &Analysis, params: &StormAnalysisParams) -> String {
    let mut s = String::from(r#"<div class="hc-formula-panel">"#);
    let idf = params.idf.design_curve();
    s.push_str(&formula_step(
        "Rational peak flow",
        r"Q = C \cdot i \cdot A",
        &format!(
            r"Q_{{\text{{design}}}} = C \cdot i \cdot A \quad (\text{{acres}} \rightarrow \text{{cfs via }} 1.008)"
        ),
    ));
    s.push_str(&formula_step(
        "IDF intensity",
        r"i = \frac{a}{(t+b)^c}",
        &format!(
            r"i = \frac{{{a}}}{{({b}+t)^{{{c}}}}} \quad \text{{RP {rp}-yr}}",
            a = idf.a,
            b = idf.b,
            c = idf.c,
            rp = params.idf.design_rp,
        ),
    ));
    s.push_str(&formula_step(
        "Manning (US)",
        r"Q = \frac{1.486}{n} A R^{2/3} S^{1/2}",
        r"V = \frac{Q}{A}",
    ));
    s.push_str(&formula_step(
        "Junction loss",
        r"h_m = K \cdot \frac{V^2}{2g}",
        &format!(r"K = {:.2}", params.hydraulics.junction_k),
    ));

    if let Some(p) = a.pipes.first() {
        if let Some(nd) = net.nodes.iter().find(|n| n.id == p.from) {
            s.push_str(&formula_step(
                &format!("Example: pipe {}", p.id),
                r"Q = C \cdot i \cdot A",
                &format!(
                    r"Q_{{{pid}}} = {q:.2}\ \text{{cfs}},\ i = {i:.2}\ \text{{in/hr}},\ t_c = {tc:.1}\ \text{{min}}",
                    pid = esc(&p.id),
                    q = p.design_q,
                    i = p.intensity,
                    tc = p.tc,
                ),
            ));
            let _ = nd;
        }
    }
    s.push_str("</div>");
    s
}

fn inlet_table_html(net: &Network, a: &Analysis, geom: &InletGeometry) -> String {
    let mut rows = String::new();
    let mut any = false;
    for nd in &net.nodes {
        if nd.kind != NodeKind::Inlet {
            continue;
        }
        let q = a
            .pipes
            .iter()
            .filter(|p| p.from == nd.id)
            .map(|p| p.design_q)
            .fold(0.0f64, f64::max);
        if q <= 0.0 {
            continue;
        }
        any = true;
        let chk = check_inlet_geom(q, geom);
        let status = if chk.ok {
            r#"<span class="pass">ok</span>"#
        } else {
            r#"<span class="failtxt">BYPASS</span>"#
        };
        rows.push_str(&format!(
            r#"<tr><td>{id}</td><td>{kind}</td><td>{q:.2}</td><td>{cap:.2}</td><td>{status}</td></tr>"#,
            id = esc(&nd.id),
            kind = esc(geom.kind.label()),
            q = chk.design_q_cfs,
            cap = chk.capacity_cfs,
            status = status,
        ));
    }
    if !any {
        return String::new();
    }
    format!(
        r#"<h2>Inlet capacity (HEC-22)</h2>
<p class="meta">Grate L={gl:.1} ft, curb L={cl:.1} ft, d={d:.3} ft, S={s:.4} ft/ft</p>
<table><thead><tr><th>Node</th><th>Type</th><th>Q (cfs)</th><th>Cap (cfs)</th><th>Status</th></tr></thead><tbody>{rows}</tbody></table>"#,
        gl = geom.grate_length_ft,
        cl = geom.curb_opening_length_ft,
        d = geom.flow_depth_ft,
        s = geom.gutter_slope,
        rows = rows,
    )
}

/// Build a self-contained HTML document with KaTeX formula panels and result tables.
pub fn format_analysis_html(
    net: &Network,
    a: &Analysis,
    params: &StormAnalysisParams,
    meta: &HtmlReportMeta,
) -> String {
    let geom = params.inlet_geometry();
    let mut out = String::new();
    out.push_str("<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"/>");
    out.push_str(&format!("<title>{}</title>", esc(&meta.title)));
    out.push_str(
        r#"<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.8/dist/katex.min.css">"#,
    );
    out.push_str(
        r#"<script src="https://cdn.jsdelivr.net/npm/katex@0.16.8/dist/katex.min.js"></script>"#,
    );
    append_css(&mut out);
    out.push_str("</head><body>");
    out.push_str(&format!("<h1>{}</h1>", esc(&meta.title)));
    out.push_str(&format!(
        r#"<p class="meta">Drawing: <strong>{}</strong> &nbsp;|&nbsp; Generated: {} &nbsp;|&nbsp; {}"#,
        esc(&meta.drawing_name),
        esc(&meta.generated_utc),
        esc(&params.summary()),
    ));
    out.push_str("</p>");

    out.push_str("<h2>Calculation trace</h2>");
    out.push_str(&formula_panel(net, a, params));

    out.push_str("<h2>Pipe hydraulics</h2>");
    out.push_str(&pipe_table_html(a));

    out.push_str("<h2>Nodes / HGL</h2>");
    out.push_str(&node_table_html(a));

    let inlet = inlet_table_html(net, a, &geom);
    if !inlet.is_empty() {
        out.push_str(&inlet);
    }

    let surcharged: Vec<&str> = a.pipes.iter().filter(|p| p.surcharged).map(|p| p.id.as_str()).collect();
    let flooding: Vec<&str> = a
        .nodes
        .iter()
        .filter(|n| n.surcharge_to_surface)
        .map(|n| n.id.as_str())
        .collect();
    out.push_str("<h2>Summary</h2><ul>");
    if surcharged.is_empty() && flooding.is_empty() {
        out.push_str("<li>All pipes flow open-channel; no surface flooding.</li>");
    } else {
        if !surcharged.is_empty() {
            out.push_str(&format!(
                "<li>Surcharged pipes: <strong>{}</strong></li>",
                esc(&surcharged.join(", "))
            ));
        }
        if !flooding.is_empty() {
            out.push_str(&format!(
                "<li>Structures flooding (HGL &gt; rim): <strong>{}</strong></li>",
                esc(&flooding.join(", "))
            ));
        }
    }
    out.push_str("</ul>");

    out.push_str(
        r#"<div class="disclaimer">Formula-transparent report generated by OpenCAD HydroComplete. Verify inputs and agency criteria before construction.</div>"#,
    );
    out.push_str(KATEX_REHYDRATE);
    out.push_str("</body></html>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idf::IdfCurve;
    use crate::network::{Network, Node, Pipe};
    use crate::params::StormAnalysisParams;

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
    fn html_contains_katex_and_tables() {
        let (net, a) = sample();
        let html = format_analysis_html(
            &net,
            &a,
            &StormAnalysisParams::default(),
            &HtmlReportMeta {
                title: "Test".into(),
                drawing_name: "test.dwg".into(),
                generated_utc: "2026-06-22".into(),
            },
        );
        assert!(html.contains("katex@0.16.8"));
        assert!(html.contains("hc-formula-panel"));
        assert!(html.contains("hc-tex-fallback"));
        assert!(html.contains("<table>"));
        assert!(html.contains("P1"));
    }
}