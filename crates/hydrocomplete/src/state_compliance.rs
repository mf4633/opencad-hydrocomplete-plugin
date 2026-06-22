//! Embedded state regulatory thresholds (mirrors `StateCompliance.cs`).

mod state_compliance_data;

use std::collections::HashMap;

pub use state_compliance_data::EMBEDDED_STATE_COUNT;

#[derive(Debug, Clone)]
pub struct StateComplianceConfig {
    pub code: String,
    pub name: String,
    pub regulatory_body: String,
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

/// Returns the config for `state_code` or DEFAULT.
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

/// All configured state codes except DEFAULT.
pub fn available_state_codes() -> Vec<String> {
    let mut list: Vec<String> = configs()
        .keys()
        .filter(|k| !k.eq_ignore_ascii_case(DEFAULT_CODE))
        .cloned()
        .collect();
    list.sort_by(|a, b| a.cmp(b));
    list
}

/// TSS removal requirement for a development type.
pub fn required_tss_percent(config: &StateComplianceConfig, development_type: &str) -> f64 {
    if development_type.eq_ignore_ascii_case("roadway") {
        if let Some(v) = config.roadway_tss_removal_percent {
            return v;
        }
    }
    config.tss_removal_percent
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
}