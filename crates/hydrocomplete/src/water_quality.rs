//! Water quality volume, EMC loads, and BMP treatment trains.

use std::collections::HashMap;

use crate::models::Catchment;
use crate::trace::{CalcStep, TracedResult};

pub const CF_PER_ACRE_INCH: f64 = 3630.0;
pub const SQ_FT_PER_ACRE: f64 = 43_560.0;
pub const GALLONS_PER_CF: f64 = 7.48;
pub const LBS_PER_GALLON: f64 = 8.34;
pub const MG_PER_LB: f64 = 1_000_000.0;

pub const POLLUTANT_TSS: &str = "TSS";
pub const POLLUTANT_TN: &str = "TN";
pub const POLLUTANT_TP: &str = "TP";
pub const CORE_POLLUTANTS: &[&str] = &[POLLUTANT_TSS, POLLUTANT_TN, POLLUTANT_TP];

#[derive(Debug, Clone)]
pub struct WqvResult {
    pub total_area_acres: f64,
    pub impervious_percent: f64,
    pub runoff_coefficient_rv: f64,
    pub design_storm_inches: f64,
    pub wqv_cf: f64,
    pub wqv_acre_ft: f64,
    pub wqv_gallons: f64,
    pub trace: TracedResult,
}

#[derive(Debug, Clone)]
pub struct EmcLoadResult {
    pub pollutant: String,
    pub land_use: String,
    pub emc_mg_per_l: f64,
    pub runoff_depth_in: f64,
    pub drainage_area_acres: f64,
    pub runoff_volume_gallons: f64,
    pub emc_load_lbs: f64,
    pub trace: TracedResult,
}

#[derive(Debug, Clone)]
pub struct TreatmentTrainBmpStep {
    pub bmp_type: String,
    pub influent_lbs: HashMap<String, f64>,
    pub effluent_lbs: HashMap<String, f64>,
    pub removed_lbs: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
pub struct TreatmentTrainResult {
    pub chain_length: usize,
    pub bmp_steps: Vec<TreatmentTrainBmpStep>,
    pub initial_loads_lbs: HashMap<String, f64>,
    pub final_effluent_lbs: HashMap<String, f64>,
    pub total_removed_lbs: HashMap<String, f64>,
    pub overall_removal_efficiency: HashMap<String, f64>,
    pub trace: TracedResult,
}

fn bmp_trapping_efficiency(bmp_type: &str, pollutant: &str) -> f64 {
    match bmp_type.to_ascii_lowercase().as_str() {
        "bioretention" => match pollutant {
            POLLUTANT_TSS => 0.85,
            POLLUTANT_TN => 0.45,
            POLLUTANT_TP => 0.60,
            _ => 0.0,
        },
        "wet-pond" | "pond" => match pollutant {
            POLLUTANT_TSS => 0.80,
            POLLUTANT_TN => 0.40,
            POLLUTANT_TP => 0.50,
            _ => 0.0,
        },
        "sand-filter" => match pollutant {
            POLLUTANT_TSS => 0.85,
            POLLUTANT_TN => 0.35,
            POLLUTANT_TP => 0.50,
            _ => 0.0,
        },
        "vegetated-swale" => match pollutant {
            POLLUTANT_TSS => 0.65,
            POLLUTANT_TN => 0.35,
            POLLUTANT_TP => 0.40,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

fn emc_mg_per_l(land_use: &str, pollutant: &str) -> f64 {
    let lu = land_use.to_ascii_lowercase();
    let (tss, tn, tp) = if lu.contains("commercial") {
        (163.0, 2.7, 0.41)
    } else if lu.contains("industrial") {
        (198.0, 2.9, 0.48)
    } else {
        (101.0, 2.2, 0.38)
    };
    match pollutant {
        POLLUTANT_TSS => tss,
        POLLUTANT_TN => tn,
        POLLUTANT_TP => tp,
        _ => 0.0,
    }
}

pub fn runoff_coefficient_from_impervious(impervious_percent: f64) -> f64 {
    assert!((0.0..=100.0).contains(&impervious_percent));
    0.05 + 0.009 * impervious_percent
}

pub fn impervious_from_runoff_c(runoff_c: f64) -> f64 {
    assert!((0.0..=1.0).contains(&runoff_c));
    let i = (runoff_c - 0.05) / 0.009;
    i.clamp(0.0, 100.0)
}

pub fn compute_wqv(total_area_acres: f64, design_storm_inches: f64, runoff_coefficient_rv: f64) -> WqvResult {
    assert!(total_area_acres >= 0.0 && design_storm_inches >= 0.0);
    assert!((0.0..=1.0).contains(&runoff_coefficient_rv));
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
        trace: TracedResult {
            steps: vec![
                CalcStep::new("Rv", runoff_coefficient_rv, "", "0.05 + 0.009*I"),
                CalcStep::new("P", design_storm_inches, "in", "WQ design storm"),
                CalcStep::new("A", total_area_acres, "ac", "drainage area"),
                CalcStep::new("WQV", wqv_cf, "cf", "Rv*P*A*3630"),
            ],
        },
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

pub fn calculate_emc_load(
    pollutant: &str,
    land_use: &str,
    runoff_depth_in: f64,
    drainage_area_acres: f64,
) -> EmcLoadResult {
    assert!(runoff_depth_in >= 0.0 && drainage_area_acres >= 0.0);
    let emc = emc_mg_per_l(land_use, pollutant);
    let volume_cf = runoff_depth_in * drainage_area_acres * SQ_FT_PER_ACRE / 12.0;
    let volume_gal = volume_cf * GALLONS_PER_CF;
    let load = emc * volume_gal * LBS_PER_GALLON / MG_PER_LB;
    EmcLoadResult {
        pollutant: pollutant.to_string(),
        land_use: land_use.to_string(),
        emc_mg_per_l: emc,
        runoff_depth_in,
        drainage_area_acres,
        runoff_volume_gallons: volume_gal,
        emc_load_lbs: load,
        trace: TracedResult {
            steps: vec![
                CalcStep::new("EMC", emc, "mg/L", format!("EMC ({land_use}, {pollutant})")),
                CalcStep::new(
                    "V_runoff",
                    volume_gal,
                    "gal",
                    format!(
                        "V = Q*A*43560/12*7.48 = {runoff_depth_in:.3}*{drainage_area_acres:.3}*43560/12*7.48"
                    ),
                ),
                CalcStep::new(
                    "L_EMC",
                    load,
                    "lbs",
                    format!("L = EMC*V_gal*8.34/1e6 = {emc:.3}*{volume_gal:.3}*8.34/1e6"),
                ),
            ],
        },
    }
}

pub fn apply_treatment_train(
    initial_loads_lbs: &HashMap<String, f64>,
    bmp_chain: &[String],
) -> TreatmentTrainResult {
    assert!(!bmp_chain.is_empty(), "at least one BMP required");
    let mut result = TreatmentTrainResult {
        chain_length: bmp_chain.len(),
        bmp_steps: Vec::new(),
        initial_loads_lbs: initial_loads_lbs.clone(),
        final_effluent_lbs: HashMap::new(),
        total_removed_lbs: initial_loads_lbs.keys().map(|k| (k.clone(), 0.0)).collect(),
        overall_removal_efficiency: HashMap::new(),
        trace: TracedResult::default(),
    };
    let mut current = initial_loads_lbs.clone();
    let mut tss_etas = Vec::new();

    for bmp_type in bmp_chain {
        let mut step = TreatmentTrainBmpStep {
            bmp_type: bmp_type.clone(),
            influent_lbs: HashMap::new(),
            effluent_lbs: HashMap::new(),
            removed_lbs: HashMap::new(),
        };
        let mut next = HashMap::new();
        for (pollutant, &influent) in &current {
            let eta = bmp_trapping_efficiency(bmp_type, pollutant);
            let removed = influent * eta;
            let treated = influent - removed;
            step.influent_lbs.insert(pollutant.clone(), influent);
            step.removed_lbs.insert(pollutant.clone(), removed);
            step.effluent_lbs.insert(pollutant.clone(), treated);
            *result.total_removed_lbs.entry(pollutant.clone()).or_insert(0.0) += removed;
            next.insert(pollutant.clone(), treated);
            if pollutant == POLLUTANT_TSS {
                tss_etas.push(eta);
            }
            result.trace.steps.push(CalcStep::new(
                format!("{pollutant}_eta"),
                eta,
                "-",
                format!("{bmp_type} trapping efficiency"),
            ));
        }
        result.bmp_steps.push(step);
        current = next;
    }

    for (pollutant, &initial) in &result.initial_loads_lbs {
        let effluent = *current.get(pollutant).unwrap_or(&0.0);
        result.final_effluent_lbs.insert(pollutant.clone(), effluent);
        let eta = if initial > 0.0 {
            result.total_removed_lbs.get(pollutant).copied().unwrap_or(0.0) / initial
        } else {
            0.0
        };
        result.overall_removal_efficiency.insert(pollutant.clone(), eta);
    }

    if !tss_etas.is_empty() {
        let product: f64 = tss_etas.iter().map(|e| 1.0 - e).product();
        let eta_total = 1.0 - product;
        result.trace.steps.push(CalcStep::new(
            "eta_total_TSS",
            eta_total,
            "-",
            format!("eta_total = 1 - prod(1-eta_i) = 1 - {product:.5}"),
        ));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wqv_formula() {
        let w = compute_wqv(2.0, 1.0, 0.77);
        assert!(w.wqv_cf > 0.0);
        assert!((w.wqv_cf - 0.77 * 1.0 * 2.0 * CF_PER_ACRE_INCH).abs() < 1e-6);
    }

    #[test]
    fn bioretention_tss_removal_is_85_percent() {
        let mut loads = HashMap::new();
        loads.insert(POLLUTANT_TSS.to_string(), 100.0);
        let train = apply_treatment_train(&loads, &[String::from("bioretention")]);
        assert!((train.overall_removal_efficiency[POLLUTANT_TSS] - 0.85).abs() < 1e-6);
    }
}