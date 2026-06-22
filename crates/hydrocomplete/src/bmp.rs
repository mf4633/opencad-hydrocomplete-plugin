//! BMP library, sizing, treatment trains, sediment, and BMP routing.

use std::collections::HashMap;

use crate::models::Catchment;
use crate::scs_runoff::{cumulative_runoff_depth, resolve_curve_number};
use crate::wqv::{calculate_wqv, SQ_FT_PER_ACRE};

pub const INCHES_PER_FOOT: f64 = 12.0;
pub const GALLONS_PER_CF: f64 = 7.48;
pub const LBS_PER_GALLON: f64 = 8.34;
pub const MG_PER_LB: f64 = 1_000_000.0;

pub mod pollutant {
    pub const TSS: &str = "TSS";
    pub const TN: &str = "TN";
    pub const TP: &str = "TP";
    pub const CORE: [&str; 3] = [TSS, TN, TP];
}

pub mod land_use {
    pub const RESIDENTIAL: &str = "residential-medium";
    pub const COMMERCIAL: &str = "commercial";
    pub const INDUSTRIAL: &str = "industrial";
}

pub mod bmp_type {
    pub const BIORETENTION: &str = "bioretention";
    pub const WET_POND: &str = "wet-pond";
    pub const SAND_FILTER: &str = "sand-filter";
    pub const VEGETATED_SWALE: &str = "vegetated-swale";
}

#[derive(Debug, Clone)]
pub struct BmpDefinition {
    pub key: &'static str,
    pub name: &'static str,
    pub trapping_efficiency: HashMap<&'static str, f64>,
    pub volume_reduction: f64,
    pub avg_depth_ft: Option<f64>,
    pub surface_area_ratio: Option<f64>,
    pub surface_loading_rate_gal_per_min_per_sf: Option<f64>,
    pub bottom_width_ft: Option<f64>,
    pub depth_ft: Option<f64>,
    pub min_length_ft: Option<f64>,
}

fn bmp_defs() -> HashMap<&'static str, BmpDefinition> {
    let mut m = HashMap::new();
    m.insert(
        bmp_type::BIORETENTION,
        BmpDefinition {
            key: bmp_type::BIORETENTION,
            name: "Bioretention Cell (Rain Garden)",
            trapping_efficiency: HashMap::from([
                (pollutant::TSS, 0.85),
                (pollutant::TN, 0.45),
                (pollutant::TP, 0.60),
            ]),
            volume_reduction: 0.0,
            avg_depth_ft: None,
            surface_area_ratio: Some(0.05),
            surface_loading_rate_gal_per_min_per_sf: None,
            bottom_width_ft: None,
            depth_ft: Some(2.5),
            min_length_ft: None,
        },
    );
    m.insert(
        bmp_type::WET_POND,
        BmpDefinition {
            key: bmp_type::WET_POND,
            name: "Wet Retention Pond",
            trapping_efficiency: HashMap::from([
                (pollutant::TSS, 0.80),
                (pollutant::TN, 0.40),
                (pollutant::TP, 0.50),
            ]),
            volume_reduction: 0.0,
            avg_depth_ft: Some(4.0),
            surface_area_ratio: None,
            surface_loading_rate_gal_per_min_per_sf: None,
            bottom_width_ft: None,
            depth_ft: None,
            min_length_ft: None,
        },
    );
    m.insert(
        bmp_type::SAND_FILTER,
        BmpDefinition {
            key: bmp_type::SAND_FILTER,
            name: "Sand Filter",
            trapping_efficiency: HashMap::from([
                (pollutant::TSS, 0.85),
                (pollutant::TN, 0.35),
                (pollutant::TP, 0.50),
            ]),
            volume_reduction: 0.0,
            avg_depth_ft: Some(2.5),
            surface_area_ratio: None,
            surface_loading_rate_gal_per_min_per_sf: Some(3.5),
            bottom_width_ft: None,
            depth_ft: None,
            min_length_ft: None,
        },
    );
    m.insert(
        bmp_type::VEGETATED_SWALE,
        BmpDefinition {
            key: bmp_type::VEGETATED_SWALE,
            name: "Vegetated Swale",
            trapping_efficiency: HashMap::from([
                (pollutant::TSS, 0.65),
                (pollutant::TN, 0.35),
                (pollutant::TP, 0.40),
            ]),
            volume_reduction: 0.0,
            avg_depth_ft: None,
            surface_area_ratio: None,
            surface_loading_rate_gal_per_min_per_sf: None,
            bottom_width_ft: Some(2.0),
            depth_ft: Some(1.5),
            min_length_ft: Some(50.0),
        },
    );
    m
}

pub fn get_bmp(bmp_type: &str) -> Result<BmpDefinition, String> {
    let key = bmp_type.trim().to_ascii_lowercase();
    let defs = bmp_defs();
    for (k, v) in &defs {
        if k.eq_ignore_ascii_case(&key) || v.name.eq_ignore_ascii_case(bmp_type) {
            return Ok(v.clone());
        }
    }
    defs.get(key.as_str())
        .cloned()
        .ok_or_else(|| format!("Unknown BMP type: {bmp_type}"))
}

fn emc(land_use: &str, pollutant: &str) -> f64 {
    let table: HashMap<_, HashMap<_, f64>> = HashMap::from([
        (
            land_use::RESIDENTIAL,
            HashMap::from([(pollutant::TSS, 101.0), (pollutant::TN, 2.2), (pollutant::TP, 0.38)]),
        ),
        (
            land_use::COMMERCIAL,
            HashMap::from([(pollutant::TSS, 163.0), (pollutant::TN, 2.7), (pollutant::TP, 0.41)]),
        ),
        (
            land_use::INDUSTRIAL,
            HashMap::from([(pollutant::TSS, 198.0), (pollutant::TN, 2.9), (pollutant::TP, 0.48)]),
        ),
    ]);
    table
        .get(land_use)
        .or_else(|| table.get(land_use::RESIDENTIAL))
        .and_then(|lu| lu.get(pollutant))
        .copied()
        .unwrap_or(0.0)
}

#[derive(Debug, Clone)]
pub struct BmpSizingResult {
    pub bmp_type: String,
    pub bmp_name: String,
    pub total_wqv_cf: f64,
    pub treated_volume_cf: f64,
    pub surface_area_sf: f64,
    pub footprint_percent: f64,
    pub length_ft: Option<f64>,
    pub width_ft: Option<f64>,
    pub volume_reduction_credit: f64,
}

pub fn size_bmp(
    bmp_type: &str,
    design_rainfall_in: f64,
    drainage_area_acres: f64,
    impervious_percent: f64,
) -> Result<BmpSizingResult, String> {
    let bmp = get_bmp(bmp_type)?;
    let wqv = calculate_wqv(design_rainfall_in, drainage_area_acres, impervious_percent);
    let treated_volume = wqv.wqv_cf * (1.0 - bmp.volume_reduction);
    let site_sf = drainage_area_acres * SQ_FT_PER_ACRE;

    let (surface_area, length_ft, width_ft) = match bmp.key {
        bmp_type::BIORETENTION => {
            let ratio = bmp.surface_area_ratio.unwrap_or(0.05);
            (site_sf * ratio, None, None)
        }
        bmp_type::VEGETATED_SWALE => {
            let bottom_width = bmp.bottom_width_ft.unwrap_or(2.0);
            let depth = bmp.depth_ft.unwrap_or(1.5);
            let cross_section = bottom_width * depth;
            let min_len = bmp.min_length_ft.unwrap_or(50.0);
            let length = (treated_volume / cross_section).max(min_len);
            (length * bottom_width, Some(length), Some(bottom_width))
        }
        bmp_type::SAND_FILTER => {
            let avg_depth = bmp.avg_depth_ft.unwrap_or(2.5);
            let area_from_volume = treated_volume / avg_depth;
            let surface_area = if let Some(slr) = bmp.surface_loading_rate_gal_per_min_per_sf {
                let volume_gal = treated_volume * GALLONS_PER_CF;
                let drawdown_hr = 40.0;
                let peak_flow_gpm = volume_gal / (drawdown_hr * 60.0);
                let area_from_loading = peak_flow_gpm / slr;
                area_from_volume.max(area_from_loading)
            } else {
                area_from_volume
            };
            (surface_area, None, None)
        }
        _ => {
            let pond_depth = bmp.avg_depth_ft.unwrap_or(3.0);
            (treated_volume / pond_depth, None, None)
        }
    };

    Ok(BmpSizingResult {
        bmp_type: bmp.key.to_string(),
        bmp_name: bmp.name.to_string(),
        total_wqv_cf: wqv.wqv_cf,
        treated_volume_cf: treated_volume,
        surface_area_sf: surface_area,
        footprint_percent: if site_sf > 0.0 {
            surface_area / site_sf * 100.0
        } else {
            0.0
        },
        length_ft,
        width_ft,
        volume_reduction_credit: bmp.volume_reduction,
    })
}

pub fn calculate_emc_load(
    pollutant: &str,
    land_use: &str,
    runoff_depth_in: f64,
    drainage_area_acres: f64,
) -> f64 {
    let emc_mg = emc(land_use, pollutant);
    let volume_cf = runoff_depth_in * drainage_area_acres * SQ_FT_PER_ACRE / INCHES_PER_FOOT;
    let volume_gal = volume_cf * GALLONS_PER_CF;
    emc_mg * volume_gal * LBS_PER_GALLON / MG_PER_LB
}

pub fn calculate_event_pollutant_loads(
    runoff_depth_in: f64,
    drainage_area_acres: f64,
    land_use: &str,
) -> HashMap<String, f64> {
    let mut loads = HashMap::new();
    for p in pollutant::CORE {
        loads.insert(p.to_string(), calculate_emc_load(p, land_use, runoff_depth_in, drainage_area_acres));
    }
    loads
}

#[derive(Debug, Clone)]
pub struct BmpTreatmentResult {
    pub bmp_type: String,
    pub bmp_name: String,
    pub influent_lbs: HashMap<String, f64>,
    pub treated_lbs: HashMap<String, f64>,
    pub removed_lbs: HashMap<String, f64>,
    pub removal_efficiency: HashMap<String, f64>,
}

pub fn apply_bmp_treatment(loads_lbs: &HashMap<String, f64>, bmp_type: &str) -> Result<BmpTreatmentResult, String> {
    let bmp = get_bmp(bmp_type)?;
    let mut result = BmpTreatmentResult {
        bmp_type: bmp.key.to_string(),
        bmp_name: bmp.name.to_string(),
        influent_lbs: HashMap::new(),
        treated_lbs: HashMap::new(),
        removed_lbs: HashMap::new(),
        removal_efficiency: HashMap::new(),
    };
    for (pollutant, influent) in loads_lbs {
        let eta = bmp.trapping_efficiency.get(pollutant.as_str()).copied().unwrap_or(0.0);
        let removed = influent * eta;
        let treated = influent - removed;
        result.influent_lbs.insert(pollutant.clone(), *influent);
        result.removed_lbs.insert(pollutant.clone(), removed);
        result.treated_lbs.insert(pollutant.clone(), treated);
        result.removal_efficiency.insert(pollutant.clone(), eta);
    }
    Ok(result)
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
}

pub fn apply_treatment_train(
    initial_loads_lbs: &HashMap<String, f64>,
    bmp_chain: &[String],
) -> Result<TreatmentTrainResult, String> {
    if bmp_chain.is_empty() {
        return Err("At least one BMP is required.".into());
    }
    let mut result = TreatmentTrainResult {
        chain_length: bmp_chain.len(),
        bmp_steps: Vec::new(),
        initial_loads_lbs: initial_loads_lbs.clone(),
        final_effluent_lbs: HashMap::new(),
        total_removed_lbs: initial_loads_lbs.keys().map(|k| (k.clone(), 0.0)).collect(),
        overall_removal_efficiency: HashMap::new(),
    };
    let mut current = initial_loads_lbs.clone();
    for bmp_type in bmp_chain {
        let step = apply_bmp_treatment(&current, bmp_type)?;
        let train_step = TreatmentTrainBmpStep {
            bmp_type: bmp_type.clone(),
            influent_lbs: step.influent_lbs.clone(),
            effluent_lbs: step.treated_lbs.clone(),
            removed_lbs: step.removed_lbs.clone(),
        };
        for (k, v) in &step.removed_lbs {
            *result.total_removed_lbs.entry(k.clone()).or_insert(0.0) += v;
        }
        current = step.treated_lbs;
        result.bmp_steps.push(train_step);
    }
    for (k, initial) in &result.initial_loads_lbs {
        let effluent = current.get(k).copied().unwrap_or(0.0);
        let removed = result.total_removed_lbs.get(k).copied().unwrap_or(0.0);
        result.final_effluent_lbs.insert(k.clone(), effluent);
        result.overall_removal_efficiency.insert(
            k.clone(),
            if *initial > 0.0 { removed / initial } else { 0.0 },
        );
    }
    Ok(result)
}

pub fn default_treatment_train() -> Vec<String> {
    vec![bmp_type::BIORETENTION.to_string(), bmp_type::WET_POND.to_string()]
}

// --- RUSLE / sediment ---

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
    let s = if slope_percent < 9.0 {
        10.8 * (slope_percent / 100.0).atan().sin() + 0.03
    } else {
        16.8 * (slope_percent / 100.0).atan().sin() - 0.50
    };
    l * s
}

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
}

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
    let ls = ls_factor(length_ft.max(10.0), slope_percent);
    let c = runoff_c.clamp(0.001, 1.0);
    let a = r_factor * k_factor * ls * c * p_factor;
    let risk = if a > 10.0 {
        "High"
    } else if a > 5.0 {
        "Moderate"
    } else {
        "Low"
    };
    RusleResult {
        name: name.to_string(),
        area_acres,
        r_factor,
        k_factor,
        ls_factor: ls,
        c_factor: c,
        p_factor,
        soil_loss_tons_per_ac_yr: a,
        risk_level: risk.to_string(),
    }
}

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

#[derive(Debug, Clone)]
pub struct SedimentBasinDesign {
    pub surface_area_sf: f64,
    pub length_ft: f64,
    pub width_ft: f64,
    pub depth_ft: f64,
    pub pool_volume_cf: f64,
    pub sediment_storage_cf: f64,
    pub total_volume_cf: f64,
    pub trapping_efficiency_pct: f64,
    pub dewatering_time_hr: f64,
    pub forebay_volume_cf: f64,
    pub forebay_length_ft: f64,
    pub forebay_width_ft: f64,
}

pub fn design_sediment_basin(
    design_flow_cfs: f64,
    drainage_area_ac: f64,
    sediment_yield_tons_per_acre_yr: f64,
) -> SedimentBasinDesign {
    const SURFACE_AREA_RATIO_SF_PER_CFS: f64 = 435.0;
    const MINIMUM_DEPTH_FT: f64 = 3.0;
    const LENGTH_WIDTH_RATIO: f64 = 2.0;
    const DEWATERING_HOURS: f64 = 48.0;
    const FOREBAY_FRACTION: f64 = 0.15;
    const BULK_DENSITY_LB_PER_CF: f64 = 80.0;

    let surface_area = design_flow_cfs * SURFACE_AREA_RATIO_SF_PER_CFS;
    let depth = MINIMUM_DEPTH_FT;
    let volume = surface_area * depth;
    let width = (surface_area / LENGTH_WIDTH_RATIO).sqrt();
    let length = width * LENGTH_WIDTH_RATIO;
    let sediment_storage =
        sediment_yield_tons_per_acre_yr * drainage_area_ac * 2000.0 / BULK_DENSITY_LB_PER_CF * 0.5;
    let forebay_volume = volume * FOREBAY_FRACTION;
    let forebay_surface = forebay_volume / 4.0;
    let forebay_width = (forebay_surface / LENGTH_WIDTH_RATIO).sqrt();
    let forebay_length = forebay_width * LENGTH_WIDTH_RATIO;

    SedimentBasinDesign {
        surface_area_sf: surface_area,
        length_ft: length,
        width_ft: width,
        depth_ft: depth,
        pool_volume_cf: volume,
        sediment_storage_cf: sediment_storage,
        total_volume_cf: volume + sediment_storage,
        trapping_efficiency_pct: 75.0,
        dewatering_time_hr: DEWATERING_HOURS,
        forebay_volume_cf: forebay_volume,
        forebay_length_ft: forebay_length,
        forebay_width_ft: forebay_width,
    }
}

// --- Bioretention routing ---

#[derive(Debug, Clone)]
pub struct BioretentionConfig {
    pub ksat_in_per_hr: f64,
    pub media_depth_ft: f64,
    pub ponding_depth_ft: f64,
}

#[derive(Debug, Clone)]
pub struct PollutantRemovalEfficiency {
    pub treated_percent: f64,
    pub blended_percent: f64,
}

#[derive(Debug, Clone)]
pub struct BioretentionRoutingResult {
    pub design_volume_cf: f64,
    pub treated_volume_cf: f64,
    pub overflow_volume_cf: f64,
    pub bypass_fraction_percent: f64,
    pub drawdown_time_hr: f64,
    pub residence_time_hr: f64,
    pub removal_efficiency: HashMap<String, PollutantRemovalEfficiency>,
}

pub fn route_bioretention(
    config: &BioretentionConfig,
    design_volume_cf: f64,
    surface_area_sf: f64,
) -> BioretentionRoutingResult {
    const GRAVITY: f64 = 32.2;
    let ksat_ft_per_hr = config.ksat_in_per_hr / INCHES_PER_FOOT;
    let porosity: f64 = 0.40;
    let field_capacity: f64 = 0.20;
    let ud_dia_ft = 6.0 / INCHES_PER_FOOT;
    let ud_cd = 0.6;

    let media_storage = surface_area_sf * config.media_depth_ft * (porosity - field_capacity).max(0.0_f64);
    let ponding_storage = surface_area_sf * config.ponding_depth_ft;
    let total_capacity = media_storage + ponding_storage;
    let treated_volume = design_volume_cf.min(total_capacity);
    let overflow = (design_volume_cf - total_capacity).max(0.0);
    let avg_head = config.ponding_depth_ft / 2.0 + config.media_depth_ft;
    let q_media = ksat_ft_per_hr * surface_area_sf * avg_head / config.media_depth_ft;
    let q_ud = ud_cd * std::f64::consts::PI * (ud_dia_ft / 2.0).powi(2) * (2.0 * GRAVITY * config.media_depth_ft / 2.0).sqrt();
    let q_total = q_media + q_ud;
    let drawdown = if q_total > 0.0 { total_capacity / q_total } else { 999.0 };
    let residence = if q_total > 0.0 { treated_volume / q_total } else { 0.0 };
    let treated_frac = if design_volume_cf > 0.0 { treated_volume / design_volume_cf } else { 0.0 };

    let curves: [(&str, f64, f64); 3] = [
        (pollutant::TSS, 0.92, 0.50),
        (pollutant::TN, 0.50, 0.20),
        (pollutant::TP, 0.65, 0.30),
    ];
    let mut removal = HashMap::new();
    for (pollutant, emax, alpha) in curves {
        let e_treated = emax * (1.0 - (-alpha * residence).exp());
        removal.insert(
            pollutant.to_string(),
            PollutantRemovalEfficiency {
                treated_percent: e_treated * 100.0,
                blended_percent: e_treated * treated_frac * 100.0,
            },
        );
    }

    BioretentionRoutingResult {
        design_volume_cf,
        treated_volume_cf: treated_volume,
        overflow_volume_cf: overflow,
        bypass_fraction_percent: if design_volume_cf > 0.0 {
            overflow / design_volume_cf * 100.0
        } else {
            0.0
        },
        drawdown_time_hr: drawdown,
        residence_time_hr: residence,
        removal_efficiency: removal,
    }
}

// --- Constructed wetland routing ---

#[derive(Debug, Clone)]
pub struct ZoneTreatmentStep {
    pub zone: String,
    pub influent_concentration: f64,
    pub effluent_concentration: f64,
    pub removal_percent: f64,
}

#[derive(Debug, Clone)]
pub struct WetlandPollutantRemoval {
    pub treated_percent: f64,
    pub zones: Vec<ZoneTreatmentStep>,
}

#[derive(Debug, Clone)]
pub struct ConstructedWetlandRoutingResult {
    pub design_volume_cf: f64,
    pub treated_volume_cf: f64,
    pub total_area_sf: f64,
    pub zone_count: usize,
    pub method: String,
    pub removal_efficiency: HashMap<String, WetlandPollutantRemoval>,
}

pub fn route_constructed_wetland(design_volume_cf: f64, surface_area_sf: f64) -> ConstructedWetlandRoutingResult {
    let zones: [(&str, f64); 4] = [
        ("forebay", 0.10),
        ("deepPool", 0.15),
        ("shallowMarsh", 0.40),
        ("shallowLand", 0.35),
    ];
    let default_conc: HashMap<&str, f64> = HashMap::from([
        (pollutant::TSS, 150.0),
        (pollutant::TN, 2.5),
        (pollutant::TP, 0.40),
    ]);
    let k_decay: HashMap<&str, f64> = HashMap::from([
        (pollutant::TSS, 20.0),
        (pollutant::TN, 10.0),
        (pollutant::TP, 12.0),
    ]);
    let c_star: HashMap<&str, f64> = HashMap::from([
        (pollutant::TSS, 5.0),
        (pollutant::TN, 1.0),
        (pollutant::TP, 0.05),
    ]);

    const CF_TO_M3: f64 = 0.0283168;
    const SF_TO_M2: f64 = 0.0929;
    let q_m3_per_year = design_volume_cf * CF_TO_M3 * 52.0;
    let total_area_m2 = surface_area_sf * SF_TO_M2;

    let mut removal = HashMap::new();
    for pollutant in pollutant::CORE {
        let c_in = *default_conc.get(pollutant).unwrap_or(&100.0);
        let k = *k_decay.get(pollutant).unwrap_or(&10.0);
        let background = *c_star.get(pollutant).unwrap_or(&0.0);
        let mut c = c_in;
        let mut zone_steps = Vec::new();
        for (zone_name, frac) in &zones {
            let zone_area_m2 = frac * total_area_m2;
            let exponent = if q_m3_per_year > 0.0 {
                -k * zone_area_m2 / q_m3_per_year
            } else {
                0.0
            };
            let c_out = (background + (c - background) * exponent.exp()).max(background);
            zone_steps.push(ZoneTreatmentStep {
                zone: zone_name.to_string(),
                influent_concentration: c,
                effluent_concentration: c_out,
                removal_percent: if c > 0.0 { (1.0 - c_out / c) * 100.0 } else { 0.0 },
            });
            c = c_out;
        }
        let overall = if c_in > 0.0 { (1.0 - c / c_in) * 100.0 } else { 0.0 };
        removal.insert(
            pollutant.to_string(),
            WetlandPollutantRemoval {
                treated_percent: overall,
                zones: zone_steps,
            },
        );
    }

    ConstructedWetlandRoutingResult {
        design_volume_cf,
        treated_volume_cf: design_volume_cf,
        total_area_sf: surface_area_sf,
        zone_count: zones.len(),
        method: "Kadlec & Wallace (2009) k-C* model, 4-zone series".into(),
        removal_efficiency: removal,
    }
}

pub fn runoff_volume_cf(runoff_depth_in: f64, area_acres: f64) -> f64 {
    runoff_depth_in * area_acres * SQ_FT_PER_ACRE / INCHES_PER_FOOT
}

pub fn composite_runoff_depth(catchments: &[Catchment], rainfall_in: f64) -> (f64, f64, f64) {
    let mut sum_a = 0.0;
    let mut sum_cna = 0.0;
    let mut sum_qa = 0.0;
    for c in catchments {
        let cn = resolve_curve_number(c);
        let depth = cumulative_runoff_depth(rainfall_in, cn);
        sum_a += c.area_acres;
        sum_cna += cn * c.area_acres;
        sum_qa += depth * c.area_acres;
    }
    let weighted_cn = if sum_a > 0.0 { sum_cna / sum_a } else { 75.0 };
    let composite_depth = if sum_a > 0.0 { sum_qa / sum_a } else { 0.0 };
    (sum_a, weighted_cn, composite_depth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_bioretention_five_percent_footprint() {
        let sizing = size_bmp(bmp_type::BIORETENTION, 1.0, 2.0, 50.0).unwrap();
        assert!((sizing.footprint_percent - 5.0).abs() < 0.1);
    }

    #[test]
    fn treatment_train_reduces_tss() {
        let loads = HashMap::from([(pollutant::TSS.to_string(), 100.0)]);
        let train = apply_treatment_train(&loads, &[bmp_type::BIORETENTION.to_string()]).unwrap();
        let effluent = train.final_effluent_lbs[pollutant::TSS];
        assert!(effluent < 100.0);
    }

    #[test]
    fn rusle_positive_loss() {
        let r = rusle(1.0, 5.0, 300.0, 0.7, 170.0, 0.32, 1.0, "test");
        assert!(r.soil_loss_tons_per_ac_yr > 0.0);
    }

    #[test]
    fn sediment_basin_scales_with_flow() {
        let d = design_sediment_basin(10.0, 5.0, 2.0);
        assert!((d.surface_area_sf - 4350.0).abs() < 1.0);
    }
}