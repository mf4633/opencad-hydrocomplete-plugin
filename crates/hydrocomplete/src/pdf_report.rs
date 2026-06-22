//! Pro PDF hydraulic report export — text tables mirroring HTML report sections.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use printpdf::{BuiltinFont, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference, PdfPageIndex};

use stormsewer::network::{Analysis, Network, PipeResult};
use stormsewer::params::StormAnalysisParams;

use crate::report_html::HtmlReportMeta;

const PAGE_W: f32 = 215.9;
const PAGE_H: f32 = 279.4;
const MARGIN: f32 = 18.0;
const LINE: f32 = 4.6;
const FOOTER_RESERVE: f32 = 22.0;

struct PdfWriter {
    doc: PdfDocumentReference,
    page: PdfPageIndex,
    layer: PdfLayerReference,
    y: f32,
    font: printpdf::IndirectFontRef,
    font_bold: printpdf::IndirectFontRef,
}

impl PdfWriter {
    fn new(doc: PdfDocumentReference, page: PdfPageIndex, layer: PdfLayerReference) -> Self {
        let font = doc
            .add_builtin_font(BuiltinFont::Helvetica)
            .expect("Helvetica");
        let font_bold = doc
            .add_builtin_font(BuiltinFont::HelveticaBold)
            .expect("HelveticaBold");
        Self {
            doc,
            page,
            layer,
            y: PAGE_H - MARGIN,
            font,
            font_bold,
        }
    }

    fn ensure(&mut self, need: f32) {
        if self.y - need >= MARGIN + FOOTER_RESERVE {
            return;
        }
        let (page, layer_idx) = self
            .doc
            .add_page(Mm(PAGE_W), Mm(PAGE_H), "continued");
        self.page = page;
        self.layer = self.doc.get_page(page).get_layer(layer_idx);
        self.y = PAGE_H - MARGIN;
    }

    fn text(&mut self, s: &str, size: f32, bold: bool) {
        self.ensure(LINE);
        let font = if bold { &self.font_bold } else { &self.font };
        self.layer.use_text(s, size, Mm(MARGIN), Mm(self.y), font);
        self.y -= LINE;
    }

    fn gap(&mut self, lines: f32) {
        self.y -= LINE * lines;
    }
}

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
    let mut w = PdfWriter::new(doc, page, layer);

    w.text(&meta.title, 12.0, true);
    w.gap(0.4);
    w.text(&format!("Drawing: {}", meta.drawing_name), 9.0, false);
    w.text(&format!("Generated: {}", meta.generated_local), 9.0, false);
    w.text(&params.summary(), 9.0, false);
    w.gap(0.6);

    let design_q = analysis
        .pipes
        .iter()
        .map(|p| p.design_q)
        .fold(0.0_f64, f64::max);
    let flat_assumed = analysis.pipes.iter().any(|p| p.slope <= 0.0 && p.manning_slope > 0.0);

    w.text("Manning Pipe Capacity", 11.0, true);
    w.gap(0.3);
    w.text(&format!("System design Q = {design_q:.2} cfs"), 9.0, false);
    if flat_assumed {
        w.text(
            "Note: flat/missing invert drop — Manning uses minimum assumed slope (*).",
            8.0,
            false,
        );
    }
    w.text(
        "Pipe       Dia   Slope     Qfull   Qdes   d/D    Status",
        8.0,
        true,
    );

    for pr in &analysis.pipes {
        let Some(pipe) = net.pipes.iter().find(|p| p.id == pr.id) else {
            continue;
        };
        w.text(
            &format!(
                "{:<10} {:>4.2} {:>8} {:>6.1} {:>6.1} {:>5}  {}",
                trim(&pipe.id, 10),
                pipe.diameter,
                format_slope(pr),
                pr.capacity,
                pr.design_q,
                format_dd(pr),
                pipe_status(pr),
            ),
            8.0,
            false,
        );
    }

    w.gap(0.6);
    w.text("Steady HGL Profile", 11.0, true);
    w.gap(0.3);
    w.text(
        "Pipe       HGL_US   HGL_DS   hf+hm*   Status",
        8.0,
        true,
    );
    for pr in &analysis.pipes {
        let hup = pr.hgl_up.map(|h| format!("{h:7.2}")).unwrap_or_else(|| "   --  ".into());
        let hdn = pr.hgl_dn.map(|h| format!("{h:7.2}")).unwrap_or_else(|| "   --  ".into());
        let drop = match (pr.hgl_up, pr.hgl_dn) {
            (Some(u), Some(d)) => format!("{:>7.2}", (u - d).max(0.0)),
            _ => "   --  ".into(),
        };
        w.text(
            &format!(
                "{:<10} {} {} {}  {}",
                trim(&pr.id, 10),
                hup,
                hdn,
                drop,
                pipe_status(pr),
            ),
            8.0,
            false,
        );
    }

    w.gap(0.5);
    w.text("Structure Elevations", 11.0, true);
    w.gap(0.3);
    w.text("Node       Rim      Invert   HGL      Surch?", 8.0, true);
    for (node, nr) in net.nodes.iter().zip(analysis.nodes.iter()) {
        let sur = if nr.surcharge_to_surface { "YES" } else { "no" };
        w.text(
            &format!(
                "{:<10} {:>7.2} {:>7.2} {:>7.2}  {sur}",
                trim(&node.id, 10),
                node.rim,
                node.invert,
                nr.hgl,
            ),
            8.0,
            false,
        );
    }

    w.ensure(FOOTER_RESERVE);
    w.gap(0.4);
    w.text(
        "Formulas: Q = (1.486/n) A R^(2/3) S^(1/2);  i = a/(t+b)^c",
        8.0,
        false,
    );
    w.text(
        "Disclaimer: Preliminary review only — verify inputs against design basis.",
        7.5,
        false,
    );

    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    w.doc.save(&mut writer).map_err(|e| e.to_string())?;
    Ok(())
}

fn format_slope(pr: &PipeResult) -> String {
    if pr.capacity_unavailable() && pr.manning_slope < 0.0 {
        return format!("{:>7.4}", pr.manning_slope);
    }
    if pr.slope <= 0.0 && pr.manning_slope > 0.0 {
        return format!("{:.4}*", pr.manning_slope);
    }
    format!("{:>7.4}", pr.slope)
}

fn format_dd(pr: &PipeResult) -> String {
    if pr.capacity_unavailable() {
        "N/A".into()
    } else if pr.report_surcharged() {
        "SURCH".into()
    } else {
        format!("{:.2}", pr.pct_full)
    }
}

fn pipe_status(pr: &PipeResult) -> &'static str {
    if pr.capacity_unavailable() {
        if pr.manning_slope < 0.0 {
            "ADVERSE"
        } else {
            "N/A"
        }
    } else if pr.report_surcharged() {
        "SURCH"
    } else {
        "ok"
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use stormsewer::idf::IdfCurve;
    use stormsewer::network::{Network, Node, Pipe};
    use stormsewer::params::StormAnalysisParams;

    fn sample_analysis() -> (Network, Analysis) {
        let net = Network {
            nodes: vec![
                Node::inlet("N1", 104.0, 110.0, 2.0, 0.75),
                Node::outfall("OUT", 100.0, 106.0),
            ],
            pipes: vec![Pipe::new("P1", "N1", "OUT", 100.0, 1.25, 0.013)],
        };
        let params = StormAnalysisParams::municipal();
        let a = net
            .analyze(params.idf.design_curve(), &params.hydraulics)
            .unwrap();
        (net, a)
    }

    #[test]
    fn pdf_writes_non_empty_file() {
        let (net, a) = sample_analysis();
        let params = StormAnalysisParams::municipal();
        let path = std::env::temp_dir().join("hc_pdf_test.pdf");
        write_hydraulic_report_pdf(
            &path,
            &net,
            &a,
            &params,
            &HtmlReportMeta {
                title: "Test".into(),
                drawing_name: "tab-0".into(),
                generated_local: "test".into(),
            },
        )
        .unwrap();
        let len = std::fs::metadata(&path).unwrap().len();
        assert!(len > 500, "PDF too small: {len} bytes");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn flat_invert_slope_shows_assumed_marker() {
        let net = Network {
            nodes: vec![
                Node::inlet("N1", 100.0, 106.0, 1.0, 0.7),
                Node::outfall("OUT", 100.0, 106.0),
            ],
            pipes: vec![Pipe::new("P1", "N1", "OUT", 100.0, 1.5, 0.013)],
        };
        let mut params = StormAnalysisParams::municipal();
        params.hydraulics.intensity_override = Some(4.0);
        let a = net
            .analyze(params.idf.design_curve(), &params.hydraulics)
            .unwrap();
        assert!((a.pipes[0].manning_slope - 0.001).abs() < 1e-9);
        assert_eq!(format_slope(&a.pipes[0]), "0.0010*");
    }
}