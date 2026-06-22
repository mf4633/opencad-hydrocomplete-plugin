//! Write formula-transparent HTML reports to the user's Documents/HydroComplete folder.

use std::path::PathBuf;

use hydrocomplete::output_paths::{build_report_path, write_file};
use hydrocomplete::report_html::{format_hydraulic_report_html, HtmlReportMeta};
use stormsewer::params::StormAnalysisParams;

use super::analysis;
use super::data;
use acadrust::EntityType;

/// Build `report-{drawing}-{stamp}.html` under Documents/HydroComplete.
pub fn build_html_report_path(drawing_name: &str) -> PathBuf {
    build_report_path(drawing_name, "html")
}

fn generated_local_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs} (UTC epoch)")
}

/// Write UTF-8 HTML to the report path; returns the absolute path written.
pub fn write_html_report(drawing_name: &str, html: &str) -> Result<PathBuf, String> {
    let path = build_html_report_path(drawing_name);
    write_file(&path, html).map_err(|e| format!("cannot write report {}: {e}", path.display()))?;
    Ok(path)
}

/// Analyze the drawn network and export the hydraulic HTML report.
/// Returns `(path, peak design Q cfs)`.
pub fn export_hydraulic_report<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
    drawing_name: &str,
) -> Result<(PathBuf, f64), String> {
    let net = data::network_from_entities(entities)?;
    let a = analysis::run_analysis_on_network(&net, params)?;
    let design_q = a.pipes.iter().map(|p| p.design_q).fold(0.0f64, f64::max);
    let html = format_hydraulic_report_html(
        &net,
        &a,
        params,
        &HtmlReportMeta {
            title: "HydroComplete Report".into(),
            drawing_name: drawing_name.to_string(),
            generated_local: generated_local_label(),
        },
    );
    let path = write_html_report(drawing_name, &html)?;
    Ok((path, design_q))
}

pub fn report_pdf_stub_lines() -> Vec<String> {
    pro_required_lines()
}

pub fn pro_required_lines() -> Vec<String> {
    vec![
        "--- HydroComplete: PDF export is a Pro feature ---".into(),
        format!(
            "  Activate at {}  (HC_ACTIVATE)",
            hydrocomplete::license::PURCHASE_URL
        ),
        "  Free alternative: HC_REPORT exports the same Manning + HGL report as HTML.".into(),
    ]
}

/// Analyze and export PDF report (Pro only). Returns `(path, peak design Q cfs)`.
pub fn export_hydraulic_report_pdf<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    params: &StormAnalysisParams,
    drawing_name: &str,
) -> Result<(PathBuf, f64), String> {
    if !hydrocomplete::license::is_pro_enabled() {
        return Err(format!(
            "PDF export requires a {} Pro license. Run HC_ACTIVATE or purchase at {}.",
            hydrocomplete::license::PRODUCT_LABEL,
            hydrocomplete::license::PURCHASE_URL
        ));
    }
    let net = data::network_from_entities(entities)?;
    let a = analysis::run_analysis_on_network(&net, params)?;
    let design_q = a.pipes.iter().map(|p| p.design_q).fold(0.0f64, f64::max);
    let path = build_report_path(drawing_name, "pdf");
    hydrocomplete::pdf_report::write_hydraulic_report_pdf(
        &path,
        &net,
        &a,
        params,
        &HtmlReportMeta {
            title: "HydroComplete Report".into(),
            drawing_name: drawing_name.to_string(),
            generated_local: generated_local_label(),
        },
    )?;
    Ok((path, design_q))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_via_output_paths() {
        use hydrocomplete::output_paths::sanitize_file_name;
        assert_eq!(sanitize_file_name("my:drawing?.dwg"), "my_drawing_.dwg");
        assert_eq!(sanitize_file_name("   "), "drawing");
    }

    #[test]
    fn report_path_format() {
        let path = build_report_path("site-plan.dwg", "html");
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("report-site-plan.dwg-"));
        assert!(name.ends_with(".html"));
        assert!(path.parent().unwrap().ends_with("HydroComplete"));
    }

    #[test]
    fn export_sample_network_html() {
        use acadrust::types::Vector3;
        use acadrust::{Circle, EntityType, Handle, Line};
        use stormsewer::network::NodeKind;

        use crate::data::{pipe_xdata, structure_xdata};

        let mut s1 = EntityType::Circle(Circle {
            center: Vector3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s1.common_mut().handle = Handle::new(1);
        s1.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Inlet, 100.0, 106.0, 1.0, 0.7));

        let mut s2 = EntityType::Circle(Circle {
            center: Vector3::new(100.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s2.common_mut().handle = Handle::new(2);
        s2.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Outfall, 99.0, 104.0, 0.0, 0.0));

        let mut p = EntityType::Line(Line::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(100.0, 0.0, 0.0),
        ));
        p.common_mut().handle = Handle::new(3);
        p.common_mut()
            .extended_data
            .add_record(pipe_xdata(1.5, 0.013, Handle::new(1), Handle::new(2)));

        let ents = vec![s1, s2, p];
        let params = StormAnalysisParams::municipal();
        let dir = std::env::temp_dir().join("hc_report_test");
        let _ = std::fs::create_dir_all(&dir);
        let (path, q) = {
            let net = data::network_from_entities(ents.iter()).unwrap();
            let a = analysis::run_analysis_on_network(&net, &params).unwrap();
            let html = format_hydraulic_report_html(
                &net,
                &a,
                &params,
                &HtmlReportMeta {
                    title: "Test".into(),
                    drawing_name: "integration".into(),
                    generated_local: "test".into(),
                },
            );
            let path = dir.join("report-integration-test.html");
            write_file(&path, &html).unwrap();
            (path, a.pipes.iter().map(|p| p.design_q).fold(0.0f64, f64::max))
        };
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("Manning Pipe Capacity"));
        assert!(q > 0.0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_pdf_with_pro_bypass() {
        use acadrust::types::Vector3;
        use acadrust::{Circle, EntityType, Handle, Line};
        use stormsewer::network::NodeKind;

        use crate::data::{pipe_xdata, structure_xdata};

        std::env::set_var("HYDROCOMPLETE_PRO", "1");

        let mut s1 = EntityType::Circle(Circle {
            center: Vector3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s1.common_mut().handle = Handle::new(1);
        s1.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Inlet, 104.0, 110.0, 2.0, 0.75));

        let mut s2 = EntityType::Circle(Circle {
            center: Vector3::new(100.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s2.common_mut().handle = Handle::new(2);
        s2.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Outfall, 100.0, 106.0, 0.0, 0.0));

        let mut p = EntityType::Line(Line::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(100.0, 0.0, 0.0),
        ));
        p.common_mut().handle = Handle::new(3);
        p.common_mut()
            .extended_data
            .add_record(pipe_xdata(1.25, 0.013, Handle::new(1), Handle::new(2)));

        let ents = vec![s1, s2, p];
        let params = StormAnalysisParams::municipal();
        let (path, q) =
            export_hydraulic_report_pdf(ents.iter(), &params, "pdf-integration").unwrap();
        assert!(path.extension().and_then(|e| e.to_str()) == Some("pdf"));
        assert!(std::fs::metadata(&path).unwrap().len() > 500);
        assert!(q > 0.0);
        let _ = std::fs::remove_file(&path);
    }
}