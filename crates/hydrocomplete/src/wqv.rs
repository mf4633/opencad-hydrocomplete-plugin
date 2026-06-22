//! Water quality volume (WQV) — Schueler Rv and WQV = Rv × P × A × 3630.

use crate::models::Catchment;

pub const CF_PER_ACRE_INCH: f64 = 3630.0;
pub const SQ_FT_PER_ACRE: f64 = 43_560.0;
pub const GALLONS_PER_CF: f64 = 7.48;

#[derive(Debug, Clone)]
pub struct WqvResult {
    pub total_area_acres: f64,
    pub impervious_percent: f64,
    pub runoff_coefficient_rv: f64,
    pub design_storm_inches: f64,
    pub wqv_cf: f64,
    pub wqv_acre_ft: f64,
    pub wqv_gallons: f64,
}

pub fn runoff_coefficient_from_impervious(impervious_percent: f64) -> f64 {
    assert!(
        (0.0..=100.0).contains(&impervious_percent),
        "impervious percent must be 0..100"
    );
    0.05 + 0.009 * impervious_percent
}

pub fn impervious_from_runoff_c(runoff_c: f64) -> f64 {
    assert!((0.0..=1.0).contains(&runoff_c), "C must be 0..1");
    let i = (runoff_c - 0.05) / 0.009;
    i.clamp(0.0, 100.0)
}

pub fn compute_wqv(
    total_area_acres: f64,
    design_storm_inches: f64,
    runoff_coefficient_rv: f64,
) -> WqvResult {
    let wqv_cf = runoff_coefficient_rv * design_storm_inches * total_area_acres * CF_PER_ACRE_INCH;
    let impervious = impervious_from_runoff_c(runoff_coefficient_rv);
    WqvResult {
        total_area_acres,
        impervious_percent: impervious,
        runoff_coefficient_rv,
        design_storm_inches,
        wqv_cf,
        wqv_acre_ft: wqv_cf / SQ_FT_PER_ACRE,
        wqv_gallons: wqv_cf / 0.133681,
    }
}

pub fn compute_wqv_from_catchments(catchments: &[Catchment], design_storm_inches: f64) -> WqvResult {
    let mut sum_a = 0.0;
    let mut sum_ca = 0.0;
    for cm in catchments {
        sum_a += cm.area_acres;
        sum_ca += cm.runoff_c * cm.area_acres;
    }
    let rv = if sum_a > 0.0 { sum_ca / sum_a } else { 0.0 };
    compute_wqv(sum_a, design_storm_inches, rv)
}

pub fn calculate_wqv(
    design_rainfall_in: f64,
    drainage_area_acres: f64,
    impervious_percent: f64,
) -> WqvResult {
    let rv = runoff_coefficient_from_impervious(impervious_percent);
    let mut result = compute_wqv(drainage_area_acres, design_rainfall_in, rv);
    result.impervious_percent = impervious_percent;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schueler_rv_at_fifty_percent() {
        assert!((runoff_coefficient_from_impervious(50.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn wqv_one_acre_one_inch() {
        let wqv = compute_wqv(1.0, 1.0, 0.5);
        assert!((wqv.wqv_cf - 1815.0).abs() < 1.0);
    }

    #[test]
    fn wqv_from_catchments_area_weighted() {
        let catchments = vec![
            Catchment {
                name: "a".into(),
                area_acres: 1.0,
                runoff_c: 0.8,
                curve_number: 0.0,
                tc_minutes: 10.0,
                outfall_structure_id: None,
                outfall_structure_name: None,
            },
            Catchment {
                name: "b".into(),
                area_acres: 1.0,
                runoff_c: 0.2,
                curve_number: 0.0,
                tc_minutes: 10.0,
                outfall_structure_id: None,
                outfall_structure_name: None,
            },
        ];
        let wqv = compute_wqv_from_catchments(&catchments, 1.0);
        assert!((wqv.runoff_coefficient_rv - 0.5).abs() < 1e-9);
        assert!((wqv.wqv_cf - 3630.0).abs() < 1.0);
    }
}