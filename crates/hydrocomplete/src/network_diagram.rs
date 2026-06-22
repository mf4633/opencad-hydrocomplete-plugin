//! HTML/SVG pipe network schematic export.

use crate::output_paths::{build_report_path, escape_html, trim_label, write_file};

#[derive(Debug, Clone)]
pub struct DiagramNode {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct DiagramPipe {
    pub key: String,
    pub name: String,
    pub upstream_id: String,
    pub downstream_id: String,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub diameter_in: f64,
}

#[derive(Debug, Clone, Default)]
pub struct PipeDiagramStats {
    pub design_flow_cfs: f64,
    pub flow_ratio: f64,
    pub surcharged: bool,
}

pub fn write_network_diagram(
    drawing_name: &str,
    network_name: &str,
    pipes: &[DiagramPipe],
    nodes: &[DiagramNode],
    pipe_stats: Option<&std::collections::HashMap<String, PipeDiagramStats>>,
) -> std::io::Result<std::path::PathBuf> {
    let path = build_report_path(drawing_name, "network-diagram.html");
    let html = build_html(drawing_name, network_name, pipes, nodes, pipe_stats);
    write_file(&path, &html)?;
    Ok(path)
}

fn build_html(
    drawing_name: &str,
    network_name: &str,
    pipes: &[DiagramPipe],
    nodes: &[DiagramNode],
    pipe_stats: Option<&std::collections::HashMap<String, PipeDiagramStats>>,
) -> String {
    let mut sb = String::new();
    sb.push_str("<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"/>\n");
    sb.push_str("<title>HydroComplete Network Diagram</title>\n");
    sb.push_str("<style>body{font-family:Segoe UI,Arial,sans-serif;margin:24px;} .disclaimer{margin-top:24px;padding:12px;background:#fff8e6;border:1px solid #e6c200;}</style>\n");
    sb.push_str("</head><body>\n");
    sb.push_str("<h1>HydroComplete — Pipe Network Diagram</h1>\n");
    sb.push_str(&format!(
        "<p>Drawing: <strong>{}</strong><br/>Network: <strong>{}</strong></p>\n",
        escape_html(drawing_name),
        escape_html(network_name)
    ));
    sb.push_str("<p>Plan-view schematic. Red pipes = surcharged; amber = Q/Q<sub>full</sub> &gt; 0.85.</p>\n");
    sb.push_str(build_legend());
    sb.push_str(&build_svg(pipes, nodes, pipe_stats));
    sb.push_str("<div class=\"disclaimer\"><strong>Note:</strong> Schematic uses drawing plan coordinates.</div>\n");
    sb.push_str("</body></html>\n");
    sb
}

fn build_legend() -> &'static str {
    r#"<div style="margin:12px 0;font-size:0.9rem">
<span style="display:inline-block;width:14px;height:14px;background:#4caf50;border-radius:50%;margin-right:4px"></span> Headwater
<span style="display:inline-block;width:14px;height:14px;background:#f48fb1;transform:rotate(45deg);margin:0 4px 0 16px"></span> Junction
<span style="display:inline-block;width:0;height:0;border-left:8px solid transparent;border-right:8px solid transparent;border-top:14px solid #ef9a9a;margin:0 4px 0 16px"></span> Outfall
<span style="display:inline-block;width:24px;height:4px;background:#555;margin:0 4px 0 16px"></span> Pipe
<span style="display:inline-block;width:24px;height:4px;background:#c62828;margin:0 4px 0 16px"></span> Surcharged
</div>"#
}

fn build_svg(
    pipes: &[DiagramPipe],
    nodes: &[DiagramNode],
    pipe_stats: Option<&std::collections::HashMap<String, PipeDiagramStats>>,
) -> String {
    if pipes.is_empty() {
        return "<p><em>No pipes in this network.</em></p>".into();
    }

    let mut inflow_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut outflow_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for pipe in pipes {
        touch_bounds(pipe.x1, pipe.y1, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
        touch_bounds(pipe.x2, pipe.y2, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
        *outflow_count.entry(pipe.upstream_id.clone()).or_insert(0) += 1;
        *inflow_count.entry(pipe.downstream_id.clone()).or_insert(0) += 1;
    }

    const PAD: f64 = 40.0;
    const WIDTH: f64 = 900.0;
    const HEIGHT: f64 = 600.0;
    let span_x = (max_x - min_x).max(1.0);
    let span_y = (max_y - min_y).max(1.0);
    let scale = ((WIDTH - 2.0 * PAD) / span_x).min((HEIGHT - 2.0 * PAD) / span_y);

    let map_x = |x: f64| PAD + (x - min_x) * scale;
    let map_y = |y: f64| HEIGHT - PAD - (y - min_y) * scale;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {WIDTH} {HEIGHT}\" style=\"max-width:100%;border:1px solid #ddd;background:#fff\">\n"
    ));

    for pipe in pipes {
        let x1 = map_x(pipe.x1);
        let y1 = map_y(pipe.y1);
        let x2 = map_x(pipe.x2);
        let y2 = map_y(pipe.y2);
        let (stroke, stroke_width) = pipe_style(pipe_stats, &pipe.key);
        svg.push_str(&format!(
            "<line x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\" stroke=\"{stroke}\" stroke-width=\"{stroke_width}\"/>\n"
        ));
        let mx = (x1 + x2) / 2.0;
        let my = (y1 + y2) / 2.0;
        let mut label = pipe.name.clone();
        if pipe.diameter_in > 0.0 {
            label.push_str(&format!(" Ø{:.1}\"", pipe.diameter_in));
        }
        if let Some(stats) = pipe_stats.and_then(|m| m.get(&pipe.key)) {
            label.push_str(&format!(" Q={:.1}", stats.design_flow_cfs));
        }
        svg.push_str(&format!(
            "<text x=\"{mx:.2}\" y=\"{:.2}\" font-size=\"10\" fill=\"#333\" text-anchor=\"middle\">{}</text>\n",
            my - 4.0,
            escape_html(&label)
        ));
    }

    for node in nodes {
        let cx = map_x(node.x);
        let cy = map_y(node.y);
        let inflows = *inflow_count.get(&node.id).unwrap_or(&0);
        let outflows = *outflow_count.get(&node.id).unwrap_or(&0);
        let is_outfall = outflows == 0 && inflows > 0;
        let is_headwater = inflows == 0 && outflows > 0;
        let is_junction = inflows > 1;

        if is_outfall {
            svg.push_str(&format!(
                "<polygon points=\"{cx:.2},{:.2} {:.2},{:.2} {:.2},{:.2}\" fill=\"#ef9a9a\" stroke=\"#b71c1c\"/>\n",
                cy + 10.0,
                cx - 8.0,
                cy - 6.0,
                cx + 8.0,
                cy - 6.0
            ));
        } else if is_junction {
            svg.push_str(&format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"14\" height=\"14\" fill=\"#f48fb1\" stroke=\"#ad1457\" transform=\"rotate(45 {cx:.2} {cy:.2})\"/>\n",
                cx - 7.0,
                cy - 7.0
            ));
        } else {
            let (fill, stroke) = if is_headwater {
                ("#4caf50", "#2e7d32")
            } else {
                ("#90caf9", "#1565c0")
            };
            svg.push_str(&format!(
                "<circle cx=\"{cx:.2}\" cy=\"{cy:.2}\" r=\"8\" fill=\"{fill}\" stroke=\"{stroke}\"/>\n"
            ));
        }
        svg.push_str(&format!(
            "<text x=\"{cx:.2}\" y=\"{:.2}\" font-size=\"9\" fill=\"#111\" text-anchor=\"middle\">{}</text>\n",
            cy + 18.0,
            escape_html(&trim_label(&node.label, 18))
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

fn pipe_style(
    stats: Option<&std::collections::HashMap<String, PipeDiagramStats>>,
    key: &str,
) -> (&'static str, f64) {
    if let Some(s) = stats.and_then(|m| m.get(key)) {
        if s.surcharged {
            return ("#c62828", 3.0);
        }
        if s.flow_ratio > 0.85 {
            return ("#f9a825", 3.0);
        }
    }
    ("#555", 3.0)
}

fn touch_bounds(x: f64, y: f64, min_x: &mut f64, min_y: &mut f64, max_x: &mut f64, max_y: &mut f64) {
    *min_x = min_x.min(x);
    *min_y = min_y.min(y);
    *max_x = max_x.max(x);
    *max_y = max_y.max(y);
}