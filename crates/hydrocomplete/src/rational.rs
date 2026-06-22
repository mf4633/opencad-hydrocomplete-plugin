//! Rational method peak flow (mirrors `Rational.cs`).

use stormsewer::idf::IdfCurve;

use crate::models::Catchment;
use crate::trace::{CalcStep, TracedResult};

#[derive(Debug, Clone)]
pub struct PeakFlowResult {
    pub peak_flow_cfs: f64,
    pub composite_c: f64,
    pub total_area_acres: f64,
    pub intensity_in_hr: f64,
    pub trace: TracedResult,
}

/// Peak flow for one catchment using its Tc on the IDF curve.
pub fn peak(catchment: &Catchment, idf: &IdfCurve) -> PeakFlowResult {
    let intensity = idf.intensity(catchment.tc_minutes);
    let mut result = peak_values(catchment.runoff_c, intensity, catchment.area_acres);
    if !catchment.name.is_empty() {
        result.trace.steps.insert(
            0,
            CalcStep::new("catchment", 0.0, "", &catchment.name),
        );
    }
    result.trace.steps.insert(
        1,
        CalcStep::new(
            "i",
            intensity,
            "in/hr",
            format!("IDF(Tc={:.1} min)", catchment.tc_minutes),
        ),
    );
    result
}

pub fn peak_values(runoff_c: f64, intensity_in_hr: f64, area_acres: f64) -> PeakFlowResult {
    assert!(
        (0.0..=1.0).contains(&runoff_c),
        "C must be 0..1"
    );
    assert!(intensity_in_hr >= 0.0 && area_acres >= 0.0);
    let q = runoff_c * intensity_in_hr * area_acres;
    PeakFlowResult {
        peak_flow_cfs: q,
        composite_c: runoff_c,
        total_area_acres: area_acres,
        intensity_in_hr,
        trace: TracedResult {
            steps: vec![CalcStep::new(
                "Q",
                q,
                "cfs",
                format!("C*i*A = {runoff_c:.3}*{intensity_in_hr:.3}*{area_acres:.3}"),
            )],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stormsewer::idf::IdfCurve;

    #[test]
    fn peak_uses_catchment_tc() {
        let idf = IdfCurve::new(100.0, 10.0, 0.8);
        let cm = Catchment {
            name: "C1".into(),
            area_acres: 1.0,
            runoff_c: 0.5,
            curve_number: 70.0,
            tc_minutes: 5.0,
            outfall_structure_id: None,
            outfall_structure_name: None,
        };
        let at5 = peak(&cm, &idf);
        let mut cm30 = cm.clone();
        cm30.tc_minutes = 30.0;
        let at30 = peak(&cm30, &idf);
        assert_ne!(at5.intensity_in_hr, at30.intensity_in_hr);
        assert_ne!(at5.peak_flow_cfs, at30.peak_flow_cfs);
    }
}