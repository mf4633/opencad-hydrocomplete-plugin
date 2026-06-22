//! SCS/TR-55 curve-number runoff (mirrors `ScsRunoff.cs`).

use crate::models::Catchment;

pub const INITIAL_ABSTRACTION_RATIO: f64 = 0.2;

#[derive(Debug, Clone)]
pub struct CatchmentRunoffResult {
    pub catchment_name: String,
    pub area_acres: f64,
    pub curve_number: f64,
    pub rainfall_inches: f64,
    pub initial_abstraction_inches: f64,
    pub potential_retention_inches: f64,
    pub runoff_depth_inches: f64,
    pub runoff_volume_cf: f64,
    pub runoff_volume_acre_ft: f64,
}

pub fn max_retention_from_cn(curve_number: f64) -> f64 {
    assert!(curve_number > 0.0 && curve_number <= 100.0, "CN must be in (0, 100]");
    1000.0 / curve_number - 10.0
}

pub fn initial_abstraction_from_cn(curve_number: f64) -> f64 {
    INITIAL_ABSTRACTION_RATIO * max_retention_from_cn(curve_number)
}

pub fn cumulative_runoff_depth(cumulative_rainfall_in: f64, curve_number: f64) -> f64 {
    if cumulative_rainfall_in < 0.0 {
        return 0.0;
    }
    let s = max_retention_from_cn(curve_number);
    let ia = INITIAL_ABSTRACTION_RATIO * s;
    if cumulative_rainfall_in <= ia {
        return 0.0;
    }
    let p_eff = cumulative_rainfall_in - ia;
    (p_eff * p_eff) / (p_eff + s)
}

pub fn catchment_runoff(catchment: &Catchment, rainfall_inches: f64) -> CatchmentRunoffResult {
    let cn = if catchment.curve_number > 0.0 {
        catchment.curve_number
    } else {
        (207.0 / catchment.runoff_c - 10.0).clamp(30.0, 98.0)
    };
    let s = max_retention_from_cn(cn);
    let ia = initial_abstraction_from_cn(cn);
    let depth = cumulative_runoff_depth(rainfall_inches, cn);
    let vol_cf = depth / 12.0 * catchment.area_acres * 43560.0;
    CatchmentRunoffResult {
        catchment_name: catchment.name.clone(),
        area_acres: catchment.area_acres,
        curve_number: cn,
        rainfall_inches,
        initial_abstraction_inches: ia,
        potential_retention_inches: s,
        runoff_depth_inches: depth,
        runoff_volume_cf: vol_cf,
        runoff_volume_acre_ft: vol_cf / 43560.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scs_runoff_matches_tr55_shape() {
        let c = Catchment {
            name: "test".into(),
            area_acres: 1.0,
            runoff_c: 0.7,
            curve_number: 70.0,
            tc_minutes: 10.0,
        };
        let r = catchment_runoff(&c, 3.0);
        assert!(r.runoff_depth_inches > 0.0);
        assert!(r.runoff_volume_cf > 0.0);
    }
}