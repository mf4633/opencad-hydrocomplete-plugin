//! Cost-effective BMP selection and treatment-train optimization.

use std::collections::HashMap;

use crate::bmp::pollutant;
use crate::wqv::{runoff_coefficient_from_impervious, SQ_FT_PER_ACRE};

pub const DEFAULT_DESIGN_LIFE_YEARS: f64 = 20.0;
pub const DEFAULT_DISCOUNT_RATE: f64 = 0.05;
pub const DEFAULT_AVG_PONDING_DEPTH_FT: f64 = 2.0;
pub const LITERS_PER_CF: f64 = 28.3168;
pub const MG_PER_LB: f64 = 453_592.0;

#[derive(Debug, Clone)]
pub struct SiteData {
    pub area_acres: f64,
    pub impervious_percent: f64,
    pub rainfall_depth_in: f64,
    pub annual_rainfall_in: f64,
    pub tss_concentration_mg_per_l: f64,
}

impl Default for SiteData {
    fn default() -> Self {
        Self {
            area_acres: 1.0,
            impervious_percent: 50.0,
            rainfall_depth_in: 1.0,
            annual_rainfall_in: 45.0,
            tss_concentration_mg_per_l: 80.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CostBmpDefinition {
    pub key: &'static str,
    pub name: &'static str,
    pub construction_cost_per_sf: f64,
    pub annual_maintenance_pct: f64,
    pub land_cost_per_sf: f64,
    pub typical_removal: HashMap<&'static str, f64>,
    pub sizing_factor: f64,
    pub reference: &'static str,
}

#[derive(Debug, Clone)]
pub struct WqvSizingResult {
    pub wqv_cf: f64,
    pub wqv_acre_ft: f64,
    pub runoff_coefficient_rv: f64,
}

#[derive(Debug, Clone)]
pub struct LifecycleCostResult {
    pub construction_cost: f64,
    pub land_cost: f64,
    pub annual_maintenance: f64,
    pub maintenance_npv: f64,
    pub total_npv: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BmpRankingEntry {
    pub bmp_type: String,
    pub name: String,
    pub meets_target: bool,
    pub removal: HashMap<String, f64>,
    pub footprint_sf: f64,
    pub total_npv: f64,
    pub cost_per_lb: f64,
    pub rank: usize,
}

#[derive(Debug, Clone)]
pub struct BmpSelectionResult {
    pub rankings: Vec<BmpRankingEntry>,
    pub wqv_cf: f64,
    pub annual_tss_load_lbs: f64,
}

#[derive(Debug, Clone)]
pub struct TreatmentTrainEntry {
    pub bmp_types: Vec<String>,
    pub names: Vec<String>,
    pub combined_removal: HashMap<String, f64>,
    pub total_cost: f64,
}

#[derive(Debug, Clone)]
pub struct TreatmentTrainResult {
    pub best_train: Option<TreatmentTrainEntry>,
    pub all_trains: Vec<TreatmentTrainEntry>,
    pub total_evaluated: usize,
}

fn tss(tss: f64, tn: f64, tp: f64) -> HashMap<&'static str, f64> {
    HashMap::from([
        (pollutant::TSS, tss),
        (pollutant::TN, tn),
        (pollutant::TP, tp),
    ])
}

pub fn default_cost_library() -> HashMap<&'static str, CostBmpDefinition> {
    HashMap::from([
        (
            "bioretention",
            CostBmpDefinition {
                key: "bioretention",
                name: "Bioretention Cell",
                construction_cost_per_sf: 28.0,
                annual_maintenance_pct: 0.05,
                land_cost_per_sf: 5.0,
                typical_removal: tss(0.85, 0.45, 0.55),
                sizing_factor: 1.0,
                reference: "NC DEQ (2020)",
            },
        ),
        (
            "constructed-wetland",
            CostBmpDefinition {
                key: "constructed-wetland",
                name: "Constructed Stormwater Wetland",
                construction_cost_per_sf: 12.0,
                annual_maintenance_pct: 0.02,
                land_cost_per_sf: 5.0,
                typical_removal: tss(0.80, 0.35, 0.50),
                sizing_factor: 2.5,
                reference: "NC DEQ (2020)",
            },
        ),
        (
            "wet-pond",
            CostBmpDefinition {
                key: "wet-pond",
                name: "Wet Detention Pond",
                construction_cost_per_sf: 8.5,
                annual_maintenance_pct: 0.03,
                land_cost_per_sf: 5.0,
                typical_removal: tss(0.80, 0.30, 0.45),
                sizing_factor: 3.0,
                reference: "NC DEQ (2020)",
            },
        ),
        (
            "dry-pond",
            CostBmpDefinition {
                key: "dry-pond",
                name: "Dry Extended Detention Basin",
                construction_cost_per_sf: 5.5,
                annual_maintenance_pct: 0.03,
                land_cost_per_sf: 5.0,
                typical_removal: tss(0.60, 0.20, 0.20),
                sizing_factor: 2.0,
                reference: "EPA (2021)",
            },
        ),
        (
            "sand-filter",
            CostBmpDefinition {
                key: "sand-filter",
                name: "Sand Filter",
                construction_cost_per_sf: 35.0,
                annual_maintenance_pct: 0.08,
                land_cost_per_sf: 5.0,
                typical_removal: tss(0.85, 0.35, 0.50),
                sizing_factor: 0.8,
                reference: "EPA (2021)",
            },
        ),
        (
            "grass-swale",
            CostBmpDefinition {
                key: "grass-swale",
                name: "Grassed Swale",
                construction_cost_per_sf: 4.0,
                annual_maintenance_pct: 0.06,
                land_cost_per_sf: 3.0,
                typical_removal: tss(0.50, 0.20, 0.20),
                sizing_factor: 1.5,
                reference: "Int'l BMP Database (2020)",
            },
        ),
        (
            "infiltration-basin",
            CostBmpDefinition {
                key: "infiltration-basin",
                name: "Infiltration Basin",
                construction_cost_per_sf: 10.0,
                annual_maintenance_pct: 0.06,
                land_cost_per_sf: 5.0,
                typical_removal: tss(0.90, 0.55, 0.65),
                sizing_factor: 1.2,
                reference: "EPA (2021)",
            },
        ),
    ])
}

pub fn calculate_wqv(site: &SiteData) -> WqvSizingResult {
    let area = if site.area_acres > 0.0 { site.area_acres } else { 1.0 };
    let rainfall = if site.rainfall_depth_in > 0.0 {
        site.rainfall_depth_in
    } else {
        1.0
    };
    let rv = runoff_coefficient_from_impervious(site.impervious_percent);
    let wqv_cf = rainfall * rv * area * SQ_FT_PER_ACRE / 12.0;
    WqvSizingResult {
        wqv_cf,
        wqv_acre_ft: wqv_cf / SQ_FT_PER_ACRE,
        runoff_coefficient_rv: rv,
    }
}

pub fn present_worth_annuity(discount_rate: f64, years: f64) -> f64 {
    if years <= 0.0 {
        return 0.0;
    }
    if discount_rate <= 0.0 {
        return years;
    }
    (1.0 - (1.0 + discount_rate).powf(-years)) / discount_rate
}

pub fn lifecycle_cost(
    bmp_type: &str,
    footprint_sf: f64,
    design_life_years: f64,
    discount_rate: f64,
) -> LifecycleCostResult {
    let library = default_cost_library();
    let key = bmp_type.trim().to_ascii_lowercase();
    let Some(bmp) = library.get(key.as_str()) else {
        return LifecycleCostResult {
            construction_cost: 0.0,
            land_cost: 0.0,
            annual_maintenance: 0.0,
            maintenance_npv: 0.0,
            total_npv: 0.0,
            error: Some(format!("Unknown BMP type: {bmp_type}")),
        };
    };
    let construction = footprint_sf * bmp.construction_cost_per_sf;
    let land = footprint_sf * bmp.land_cost_per_sf;
    let annual_maintenance = construction * bmp.annual_maintenance_pct;
    let pwa = present_worth_annuity(discount_rate, design_life_years);
    let maintenance_npv = annual_maintenance * pwa;
    LifecycleCostResult {
        construction_cost: construction,
        land_cost: land,
        annual_maintenance,
        maintenance_npv,
        total_npv: construction + land + maintenance_npv,
        error: None,
    }
}

pub fn series_removal(efficiencies: &[f64]) -> f64 {
    if efficiencies.is_empty() {
        return 0.0;
    }
    let product: f64 = efficiencies.iter().map(|e| 1.0 - e).product();
    1.0 - product
}

pub fn estimate_annual_tss_load_lbs(site: &SiteData, runoff_coefficient_rv: f64) -> f64 {
    let annual_rain = if site.annual_rainfall_in > 0.0 {
        site.annual_rainfall_in
    } else {
        45.0
    };
    let area = if site.area_acres > 0.0 { site.area_acres } else { 1.0 };
    let tss_conc = if site.tss_concentration_mg_per_l > 0.0 {
        site.tss_concentration_mg_per_l
    } else {
        80.0
    };
    let annual_runoff_cf =
        annual_rain * runoff_coefficient_rv * area * SQ_FT_PER_ACRE / 12.0;
    let annual_runoff_l = annual_runoff_cf * LITERS_PER_CF;
    tss_conc * annual_runoff_l / MG_PER_LB
}

fn meets_all_targets(
    bmp_removal: &HashMap<&'static str, f64>,
    target_removal: &HashMap<String, f64>,
) -> bool {
    for (pollutant, target) in target_removal {
        let actual = bmp_removal.get(pollutant.as_str()).copied().unwrap_or(0.0);
        if actual < *target {
            return false;
        }
    }
    true
}

pub fn optimize_bmp_selection(
    site: &SiteData,
    target_removal: &HashMap<String, f64>,
) -> BmpSelectionResult {
    let wqv = calculate_wqv(site);
    let annual_tss = estimate_annual_tss_load_lbs(site, wqv.runoff_coefficient_rv);
    let library = default_cost_library();
    let base_footprint = wqv.wqv_cf / DEFAULT_AVG_PONDING_DEPTH_FT;

    let mut entries = Vec::new();
    for (key, bmp) in &library {
        let meets = meets_all_targets(&bmp.typical_removal, target_removal);
        let footprint = base_footprint * bmp.sizing_factor;
        let lc = lifecycle_cost(key, footprint, DEFAULT_DESIGN_LIFE_YEARS, DEFAULT_DISCOUNT_RATE);
        let tss_eta = bmp.typical_removal.get(pollutant::TSS).copied().unwrap_or(0.0);
        let total_removed = annual_tss * tss_eta * DEFAULT_DESIGN_LIFE_YEARS;
        let cost_per_lb = if total_removed > 0.0 {
            lc.total_npv / total_removed
        } else {
            f64::INFINITY
        };
        let mut removal = HashMap::new();
        for (p, e) in &bmp.typical_removal {
            removal.insert(p.to_string(), *e);
        }
        entries.push(BmpRankingEntry {
            bmp_type: (*key).to_string(),
            name: bmp.name.to_string(),
            meets_target: meets,
            removal,
            footprint_sf: footprint,
            total_npv: lc.total_npv,
            cost_per_lb,
            rank: 0,
        });
    }
    entries.sort_by(|a, b| a.cost_per_lb.partial_cmp(&b.cost_per_lb).unwrap());
    for (i, e) in entries.iter_mut().enumerate() {
        e.rank = i + 1;
    }
    BmpSelectionResult {
        rankings: entries,
        wqv_cf: wqv.wqv_cf,
        annual_tss_load_lbs: annual_tss,
    }
}

fn evaluate_train(
    bmp_keys: &[&str],
    library: &HashMap<&'static str, CostBmpDefinition>,
    target_removal: &HashMap<String, f64>,
    base_footprint: f64,
    split_factor: f64,
) -> Option<TreatmentTrainEntry> {
    let mut combined = HashMap::new();
    for (pollutant, target) in target_removal {
        let efficiencies: Vec<f64> = bmp_keys
            .iter()
            .map(|k| {
                library
                    .get(*k)
                    .and_then(|b| b.typical_removal.get(pollutant.as_str()))
                    .copied()
                    .unwrap_or(0.0)
            })
            .collect();
        let eta = series_removal(&efficiencies);
        if eta < *target {
            return None;
        }
        combined.insert(pollutant.clone(), eta);
    }

    let mut train = TreatmentTrainEntry {
        bmp_types: Vec::new(),
        names: Vec::new(),
        combined_removal: combined,
        total_cost: 0.0,
    };
    for key in bmp_keys {
        let bmp = library.get(*key)?;
        let footprint = base_footprint * bmp.sizing_factor * split_factor;
        let lc = lifecycle_cost(key, footprint, DEFAULT_DESIGN_LIFE_YEARS, DEFAULT_DISCOUNT_RATE);
        train.bmp_types.push((*key).to_string());
        train.names.push(bmp.name.to_string());
        train.total_cost += lc.total_npv;
    }
    Some(train)
}

pub fn optimize_treatment_train(
    site: &SiteData,
    target_removal: &HashMap<String, f64>,
) -> TreatmentTrainResult {
    let wqv = calculate_wqv(site);
    let base_footprint = wqv.wqv_cf / DEFAULT_AVG_PONDING_DEPTH_FT;
    let library = default_cost_library();
    let bmp_types: Vec<&str> = library.keys().copied().collect();

    let mut valid_trains = Vec::new();
    for i in 0..bmp_types.len() {
        for j in i..bmp_types.len() {
            if let Some(train) = evaluate_train(
                &[bmp_types[i], bmp_types[j]],
                &library,
                target_removal,
                base_footprint,
                0.6,
            ) {
                valid_trains.push(train);
            }
        }
    }

    let top_singles: Vec<&str> = {
        let mut sorted: Vec<_> = library.iter().collect();
        sorted.sort_by(|a, b| {
            a.1.construction_cost_per_sf
                .partial_cmp(&b.1.construction_cost_per_sf)
                .unwrap()
        });
        sorted.into_iter().take(6).map(|(k, _)| *k).collect()
    };

    for i in 0..top_singles.len() {
        for j in i..top_singles.len() {
            for k in j..top_singles.len() {
                if let Some(train) = evaluate_train(
                    &[top_singles[i], top_singles[j], top_singles[k]],
                    &library,
                    target_removal,
                    base_footprint,
                    0.45,
                ) {
                    valid_trains.push(train);
                }
            }
        }
    }

    valid_trains.sort_by(|a, b| a.total_cost.partial_cmp(&b.total_cost).unwrap());
    let total = valid_trains.len();
    TreatmentTrainResult {
        best_train: valid_trains.first().cloned(),
        all_trains: valid_trains.into_iter().take(20).collect(),
        total_evaluated: total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wqv_matches_schueler() {
        let site = SiteData {
            area_acres: 1.0,
            impervious_percent: 50.0,
            rainfall_depth_in: 1.0,
            ..Default::default()
        };
        let wqv = calculate_wqv(&site);
        assert!((wqv.runoff_coefficient_rv - 0.5).abs() < 1e-9);
        assert!((wqv.wqv_cf - 1815.0).abs() < 1.0);
    }

    #[test]
    fn present_worth_annuity_formula() {
        let pwa = present_worth_annuity(0.05, 20.0);
        assert!((pwa - 12.462).abs() < 0.01);
    }

    #[test]
    fn lifecycle_cost_wet_pond() {
        let lc = lifecycle_cost("wet-pond", 1000.0, 20.0, 0.05);
        assert!((lc.construction_cost - 8500.0).abs() < 1.0);
        assert!((lc.land_cost - 5000.0).abs() < 1.0);
    }

    #[test]
    fn series_removal_two_bmps() {
        let eta = series_removal(&[0.85, 0.80]);
        assert!(eta > 0.85);
        assert!(eta < 1.0);
    }
}