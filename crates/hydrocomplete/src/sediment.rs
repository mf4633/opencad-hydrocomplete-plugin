//! RUSLE sediment facade (mirrors `SedimentEngine.cs` / `RusleAnalysis.cs`).

use crate::trace::{CalcStep, TracedResult};

#[derive(Debug, Clone)]
pub struct RusleResult {
    pub name: String,
    pub area_acres: f64,
    pub r_factor: f64,
    pub k_factor: f64,
    pub ls_factor: f64,
    pub c_factor: f64,
    pub p_factor: f64,
    pub soil_loss_tons_per_ac_yr: f64,
    pub risk_level: String,
    pub trace: TracedResult,
}

/// LS slope length-steepness factor (RUSLE / SEDCAD4).
pub fn ls_factor(slope_length_ft: f64, slope_percent: f64) -> f64 {
    assert!(slope_length_ft >= 0.0 && slope_percent >= 0.0);
    let m = if slope_percent < 1.0 {
        0.2
    } else if slope_percent < 3.0 {
        0.3
    } else if slope_percent < 5.0 {
        0.4
    } else {
        0.5
    };
    let l = (slope_length_ft / 72.6).powf(m);
    let slope_rad = (slope_percent / 100.0).atan();
    let s = if slope_percent < 9.0 {
        10.8 * slope_rad.sin() + 0.03
    } else {
        16.8 * slope_rad.sin() - 0.50
    };
    l * s
}

/// RUSLE: A = R × K × LS × C × P (tons/acre/year).
pub fn rusle(
    area_acres: f64,
    slope_percent: f64,
    length_ft: f64,
    runoff_c: f64,
    r_factor: f64,
    k_factor: f64,
    p_factor: f64,
    name: &str,
) -> RusleResult {
    assert!(area_acres >= 0.0 && slope_percent >= 0.0);
    assert!((0.0..=1.0).contains(&runoff_c));
    let ls = ls_factor(length_ft.max(10.0), slope_percent);
    let c = runoff_c.clamp(0.001, 1.0);
    let a = r_factor * k_factor * ls * c * p_factor;
    let risk = classify_risk(a);
    RusleResult {
        name: name.to_string(),
        area_acres,
        r_factor,
        k_factor,
        ls_factor: ls,
        c_factor: c,
        p_factor,
        soil_loss_tons_per_ac_yr: a,
        risk_level: risk,
        trace: TracedResult {
            steps: vec![
                CalcStep::new("R", r_factor, "", "rainfall erosivity"),
                CalcStep::new("K", k_factor, "", "soil erodibility"),
                CalcStep::new("LS", ls, "", "slope length/steepness (SEDCAD4)"),
                CalcStep::new("C", c, "", "cover factor (from runoff C)"),
                CalcStep::new("P", p_factor, "", "support practice"),
                CalcStep::new("A", a, "tons/ac/yr", "R*K*LS*C*P"),
            ],
        },
    }
}

/// Area-weighted average soil loss over catchments.
pub fn weighted_average_soil_loss(results: &[RusleResult]) -> f64 {
    let mut sum_aa = 0.0;
    let mut sum_a = 0.0;
    for r in results {
        sum_aa += r.soil_loss_tons_per_ac_yr * r.area_acres;
        sum_a += r.area_acres;
    }
    if sum_a > 0.0 {
        sum_aa / sum_a
    } else {
        0.0
    }
}

fn classify_risk(soil_loss_tons_per_ac_yr: f64) -> String {
    if soil_loss_tons_per_ac_yr > 10.0 {
        "High".into()
    } else if soil_loss_tons_per_ac_yr > 5.0 {
        "Moderate".into()
    } else {
        "Low".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ls_factor_increases_with_steepness() {
        let moderate = ls_factor(200.0, 8.0);
        let steep = ls_factor(200.0, 12.0);
        assert!(steep > moderate);
    }

    #[test]
    fn rusle_positive_loss() {
        let r = rusle(1.0, 5.0, 300.0, 0.7, 170.0, 0.32, 1.0, "C1");
        assert!(r.soil_loss_tons_per_ac_yr > 0.0);
        assert_eq!(r.risk_level, "High");
    }
}