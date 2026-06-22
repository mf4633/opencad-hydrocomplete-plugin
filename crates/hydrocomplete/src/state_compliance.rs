//! Embedded state regulatory thresholds (mirrors `StateCompliance.cs`).

#[path = "state_compliance_data.rs"]
mod state_compliance_data;

use std::collections::HashMap;

pub use state_compliance_data::EMBEDDED_STATE_COUNT;

#[derive(Debug, Clone)]
pub struct StateComplianceConfig {
    pub code: &'static str,
    pub name: &'static str,
    pub regulatory_body: &'static str,
    pub design_storm_inches: f64,
    pub wq_volume_factor_inches: f64,
    pub peak_attenuation_percent: f64,
    pub drawdown_min_hours: f64,
    pub drawdown_max_hours: f64,
    pub tss_removal_percent: f64,
    pub tn_removal_percent: f64,
    pub tp_removal_percent: f64,
    pub roadway_tss_removal_percent: Option<f64>,
    pub tolerable_soil_loss_tons_per_ac_yr: f64,
    pub default_r_factor: f64,
    pub volume_control_required: bool,
}

pub const DEFAULT_CODE: &str = "DEFAULT";

static CONFIGS: std::sync::OnceLock<HashMap<String, StateComplianceConfig>> =
    std::sync::OnceLock::new();

fn configs() -> &'static HashMap<String, StateComplianceConfig> {
    CONFIGS.get_or_init(|| {
        let mut map = HashMap::new();
        for (code, cfg) in state_compliance_data::all_configs() {
            map.insert(code.to_ascii_uppercase(), cfg.clone());
        }
        map
    })
}

pub fn get(state_code: &str) -> StateComplianceConfig {
    get_config(state_code)
}

pub fn get_config(state_code: &str) -> StateComplianceConfig {
    if state_code.trim().is_empty() {
        return configs()[DEFAULT_CODE].clone();
    }
    let key = state_code.trim().to_ascii_uppercase();
    configs()
        .get(&key)
        .cloned()
        .unwrap_or_else(|| configs()[DEFAULT_CODE].clone())
}

pub fn available_state_codes() -> Vec<String> {
    let mut list: Vec<String> = configs()
        .keys()
        .filter(|k| !k.eq_ignore_ascii_case(DEFAULT_CODE))
        .cloned()
        .collect();
    list.sort();
    list
}

pub fn required_tss_percent(config: &StateComplianceConfig, development_type: &str) -> f64 {
    if development_type.eq_ignore_ascii_case("roadway") {
        if let Some(v) = config.roadway_tss_removal_percent {
            return v;
        }
    }
    config.tss_removal_percent
}

/// 24-hour peak-control storm depths (inches) keyed by return period label.
pub fn peak_storm_suite(state_code: &str) -> HashMap<&'static str, f64> {
    let table: &[(&str, f64)] = match state_code.trim().to_uppercase().as_str() {
        "NC" => &[
            ("2-year", 3.0),
            ("10-year", 4.5),
            ("25-year", 5.5),
            ("100-year", 7.2),
        ],
        "SC" => &[
            ("2-year", 3.2),
            ("10-year", 5.0),
            ("25-year", 6.0),
            ("100-year", 8.0),
        ],
        "VA" => &[
            ("2-year", 3.0),
            ("10-year", 4.8),
            ("25-year", 5.8),
            ("100-year", 7.5),
        ],
        "FL" => &[
            ("2-year", 3.5),
            ("10-year", 5.2),
            ("25-year", 6.2),
            ("100-year", 8.0),
        ],
        "TX" => &[
            ("2-year", 3.2),
            ("10-year", 4.7),
            ("25-year", 5.7),
            ("100-year", 7.5),
        ],
        "CA" => &[
            ("2-year", 2.0),
            ("10-year", 3.5),
            ("25-year", 4.5),
            ("100-year", 6.0),
        ],
        "NY" => &[
            ("2-year", 3.5),
            ("10-year", 5.5),
            ("25-year", 6.5),
            ("100-year", 8.5),
        ],
        _ => &[
            ("2-year", 3.0),
            ("10-year", 4.5),
            ("25-year", 5.5),
            ("100-year", 7.0),
        ],
    };
    table.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_for_unknown_code() {
        let cfg = get("ZZ");
        assert_eq!(cfg.code, "DEFAULT");
        assert!((cfg.tss_removal_percent - 80.0).abs() < 1e-6);
    }

    #[test]
    fn embedded_state_count_is_53() {
        assert_eq!(EMBEDDED_STATE_COUNT, 53);
        assert_eq!(available_state_codes().len(), 53);
    }

    #[test]
    fn tx_wqv_storm_is_150_inches() {
        let tx = get("TX");
        assert!((tx.wq_volume_factor_inches - 1.5).abs() < 0.01);
        assert!((tx.design_storm_inches - 1.5).abs() < 0.01);
    }

    #[test]
    fn nc_requires_85_percent_tss() {
        let nc = get("NC");
        assert!((nc.tss_removal_percent - 85.0).abs() < 1e-6);
        assert!((required_tss_percent(&nc, "roadway") - 80.0).abs() < 1e-6);
    }

    #[test]
    fn peak_suite_nc_has_four_storms() {
        let suite = peak_storm_suite("NC");
        assert_eq!(suite.len(), 4);
        assert_eq!(suite["100-year"], 7.2);
    }
}