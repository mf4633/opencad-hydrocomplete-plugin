//! Pro PDF hydraulic report export — text tables mirroring `PdfReportWriter.cs`.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use printpdf::{BuiltinFont, Mm, PdfDocument};
use stormsewer::hydraulics::{full_flow_capacity, K_MANNING_US};
use stormsewer::network::{Analysis, Network};
use stormsewer::params::StormAnalysisParams;

use crate::report_html::HtmlReportMeta;

const PAGE_W: f32 = 215.9;
const PAGE_H: f32 = 279.4;
const MARGIN: f32 = 18.0;
const LINE: f32 = 4.8;

pub fn write_hydraulic_report_pdf(
    path: &Path,
    net: &Network,
    analysis: &Analysis,
    params: &StormAnalysisParams,
    meta: &HtmlReportMeta,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let (doc, page, layer_idx) =
        PdfDocument::new(&meta.title, Mm(PAGE_W), Mm(PAGE_H), "Layer 1");
    let layer = doc.get_page(page).get_layer(layer_idx);
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| e.to_string())?;
    let font_bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| e.to_string())?;

    let mut y = PAGE_H - MARGIN;
    layer.use_text(&meta.title, 12.0, Mm(MARGIN), Mm(y), &font_bold);
    y -= LINE * 1.5;
    for line in [
        format!("Drawing: {}", meta.drawing_name),
        format!("Generated: {}", meta.generated_local),
        format!(
            "IDF: {}-yr  i = {:.1}/(t+{:.1})^{:.2}",
            params.idf.design_rp,
            params.idf.design_curve().a,
            params.idf.design_curve().b,
            params.idf.design_curve().c,
        ),
    ] {
        layer.use_text(&line, 9.0, Mm(MARGIN), Mm(y), &font);
        y -= LINE;
    }
    y -= LINE * 0.5;

    let design_q = analysis.pipes.iter().map(|p| p.design_q).fold(0.0_f64, f64::max);
    layer.use_text("Manning Pipe Capacity", 11.0, Mm(MARGIN), Mm(y), &font_bold);
    y -= LINE * 1.3;
    layer.use_text(
        &format!("System design Q = {design_q:.1} cfs"),
        9.0,
        Mm(MARGIN),
        Mm(y),
        &font,
    );
    y -= LINE;
    layer.use_text(
        "Pipe          Dia(ft)  n      L(ft)   S       Q(cfs)  Qfull   D/D",
        8.0,
        Mm(MARGIN),
        Mm(y),
        &font_bold,
    );
    y -= LINE;

    for pr in &analysis.pipes {
        let Some(pipe) = net.pipes.iter().find(|p| p.id == pr.id) else {
            continue;
        };
        let cap = full_flow_capacity(pipe.n, pr.slope, pipe.diameter, K_MANNING_US);
        let ratio = if cap > 0.0 { pr.design_q / cap } else { 0.0 };
        let row = format!(
            "{:<12}  {:>6.2}  {:>5.3}  {:>6.0}  {:>5.4}  {:>6.1}  {:>6.1}  {:>4.2}",
            trim(&pipe.id, 12),
            pipe.diameter,
            pipe.n,
            pipe.length,
            pr.slope,
            pr.design_q,
            cap,
            ratio,
        );
        layer.use_text(&row, 8.0, Mm(MARGIN), Mm(y), &font);
        y -= LINE;
    }

    y -= LINE * 0.5;
    layer.use_text(
        "Steady HGL (normal depth + junction losses)",
        11.0,
        Mm(MARGIN),
        Mm(y),
        &font_bold,
    );
    y -= LINE * 1.3;
    layer.use_text(
        "Node          Rim      Invert   HGL      Surch?",
        8.0,
        Mm(MARGIN),
        Mm(y),
        &font_bold,
    );
    y -= LINE;
    for (node, nr) in net.nodes.iter().zip(analysis.nodes.iter()) {
        let sur = if nr.surcharge_to_surface { "YES" } else { "no" };
        let row = format!(
            "{:<12}  {:>7.2}  {:>7.2}  {:>7.2}  {}",
            trim(&node.id, 12),
            node.rim,
            node.invert,
            nr.hgl,
            sur,
        );
        layer.use_text(&row, 8.0, Mm(MARGIN), Mm(y), &font);
        y -= LINE;
    }

    y -= LINE;
    layer.use_text(
        "Formulas: Q = (1.486/n) A R^(2/3) S^(1/2);  i = a/(t+b)^c",
        8.0,
        Mm(MARGIN),
        Mm(y),
        &font,
    );

    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    doc.save(&mut writer).map_err(|e| e.to_string())?;
    Ok(())
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