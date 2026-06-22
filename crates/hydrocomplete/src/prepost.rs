//! Pre/post-development peak flow comparison with optional detention routing.

use std::collections::HashMap;

use crate::detention::{
    build_prismatic_storage_indication_curve, default_nc_detention_outlets, inflow_from_unit_hydrograph,
    route, OutletDefinition, OrificeOutlet, RoutingResult,
};
use crate::scs_runoff::runoff_depth_inches;
use crate::scs_unit_hydrograph;

pub const PASS_TOLERANCE_FACTOR: f64 = 1.01;

#[derive(Debug, Clone)]
pub struct WatershedInput {
    pub area_acres: f64,
    pub curve_number: f64,
    pub tc_hours: f64,
}

#[derive(Debug, Clone)]
pub struct PondConfiguration {
    pub max_storage_ft3: f64,
    pub avg_depth_ft: f64,
    pub outlets: Vec<OutletDefinition>,
    pub routing_timestep_hours: f64,
}

impl Default for PondConfiguration {
    fn default() -> Self {
        Self {
            max_storage_ft3: 50_000.0,
            avg_depth_ft: 8.0,
            outlets: default_nc_detention_outlets(),
            routing_timestep_hours: crate::detention::DEFAULT_TIMESTEP_HOURS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StormPeakDetail {
    pub curve_number: f64,
    pub runoff_depth_in: f64,
    pub peak_flow_cfs: f64,
}

#[derive(Debug, Clone)]
pub struct PostStormPeakDetail {
    pub curve_number: f64,
    pub runoff_depth_in: f64,
    pub peak_unrouted_cfs: f64,
    pub peak_routed_cfs: f64,
    pub peak_reduction_percent: f64,
}

#[derive(Debug, Clone)]
pub struct StormComparisonRow {
    pub return_period: String,
    pub rainfall_in: f64,
    pub pre_development: StormPeakDetail,
    pub post_development: PostStormPeakDetail,
    pub pass: bool,
    pub margin_cfs: f64,
}

#[derive(Debug, Clone)]
pub struct PrePostComparisonResult {
    pub all_pass: bool,
    pub rows: Vec<StormComparisonRow>,
}

pub fn peak_flow_cfs(watershed: &WatershedInput, rainfall_inches: f64) -> f64 {
    let runoff_depth = runoff_depth_inches(rainfall_inches, watershed.curve_number);
    let uh = scs_unit_hydrograph::generate(
        watershed.area_acres.max(1.0),
        (watershed.tc_hours * 60.0).max(1.0),
        None,
        None,
    );
    uh.peak_flow_cfs * runoff_depth
}

fn parse_return_period_order(key: &str) -> i32 {
    key.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(999)
}

fn sort_storm_keys(keys: &[String]) -> Vec<String> {
    let mut sorted = keys.to_vec();
    sorted.sort_by_key(|k| parse_return_period_order(k));
    sorted
}

fn build_pond_curve(config: &PondConfiguration) -> Vec<crate::detention::StorageIndicationPoint> {
    build_prismatic_storage_indication_curve(config.max_storage_ft3, &config.outlets, config.avg_depth_ft)
}

fn evaluate_storm(
    return_period: &str,
    rainfall_in: f64,
    pre_dev: &WatershedInput,
    post_dev: &WatershedInput,
    pond_config: Option<&PondConfiguration>,
) -> StormComparisonRow {
    let pre_cn = if pre_dev.curve_number > 0.0 {
        pre_dev.curve_number
    } else {
        65.0
    };
    let post_cn = if post_dev.curve_number > 0.0 {
        post_dev.curve_number
    } else {
        80.0
    };

    let pre_runoff = runoff_depth_inches(rainfall_in, pre_cn);
    let post_runoff = runoff_depth_inches(rainfall_in, post_cn);

    let pre_uh = scs_unit_hydrograph::generate(
        pre_dev.area_acres.max(1.0),
        (pre_dev.tc_hours.max(0.5) * 60.0),
        None,
        None,
    );
    let post_uh = scs_unit_hydrograph::generate(
        post_dev.area_acres.max(1.0),
        (post_dev.tc_hours.max(0.3) * 60.0),
        None,
        None,
    );

    let pre_peak = pre_uh.peak_flow_cfs * pre_runoff;
    let post_peak_unrouted = post_uh.peak_flow_cfs * post_runoff;
    let mut post_peak_routed = post_peak_unrouted;
    let mut peak_reduction = 0.0;

    if let Some(pond) = pond_config {
        let hydrograph = inflow_from_unit_hydrograph(&post_uh, post_runoff);
        if hydrograph.len() > 2 {
            let curve = build_pond_curve(pond);
            let routing: RoutingResult = route(&hydrograph, &curve, pond.routing_timestep_hours);
            post_peak_routed = routing.peak_outflow_cfs;
            if post_peak_unrouted > 0.0 {
                peak_reduction =
                    (post_peak_unrouted - post_peak_routed) / post_peak_unrouted * 100.0;
            }
        }
    }

    let pass = post_peak_routed <= pre_peak * PASS_TOLERANCE_FACTOR;

    StormComparisonRow {
        return_period: return_period.to_string(),
        rainfall_in,
        pre_development: StormPeakDetail {
            curve_number: pre_cn,
            runoff_depth_in: pre_runoff,
            peak_flow_cfs: pre_peak,
        },
        post_development: PostStormPeakDetail {
            curve_number: post_cn,
            runoff_depth_in: post_runoff,
            peak_unrouted_cfs: post_peak_unrouted,
            peak_routed_cfs: post_peak_routed,
            peak_reduction_percent: peak_reduction,
        },
        pass,
        margin_cfs: pre_peak - post_peak_routed,
    }
}

pub fn run(
    pre_development: &WatershedInput,
    post_development: &WatershedInput,
    storms: &HashMap<String, f64>,
    pond_config: Option<&PondConfiguration>,
) -> PrePostComparisonResult {
    let keys: Vec<String> = storms.keys().cloned().collect();
    let mut rows = Vec::new();
    let mut all_pass = true;

    for key in sort_storm_keys(&keys) {
        let Some(rainfall) = storms.get(&key) else {
            continue;
        };
        if rainfall.is_nan() || rainfall.is_infinite() {
            continue;
        }
        let row = evaluate_storm(&key, *rainfall, pre_development, post_development, pond_config);
        if !row.pass {
            all_pass = false;
        }
        rows.push(row);
    }

    PrePostComparisonResult { all_pass, rows }
}

pub fn watershed_from_catchments(
    catchments: &[crate::models::Catchment],
    pre_cn: Option<f64>,
) -> (WatershedInput, WatershedInput) {
    let area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    let area = area.max(1.0);
    let tc_min = catchments
        .iter()
        .map(|c| c.tc_minutes)
        .fold(0.0_f64, f64::max)
        .max(10.0);

    let composite = crate::scs_runoff::compute_composite(catchments, 1.0);
    let post_cn = if composite.weighted_curve_number > 0.0 {
        composite.weighted_curve_number
    } else {
        75.0
    };
    let pre_cn_val = pre_cn.unwrap_or((post_cn - 15.0).max(55.0));

    let post = WatershedInput {
        area_acres: area,
        curve_number: post_cn,
        tc_hours: tc_min / 60.0,
    };
    let pre = WatershedInput {
        area_acres: area,
        curve_number: pre_cn_val,
        tc_hours: tc_min / 60.0,
    };
    (pre, post)
}

pub fn default_detention_pond() -> PondConfiguration {
    PondConfiguration {
        outlets: vec![OutletDefinition::Orifice(OrificeOutlet {
            name: "primary".into(),
            diameter_inches: 4.0,
            cd: 0.6,
            invert_elev_ft: 0.0,
        })],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storm_suite() -> HashMap<String, f64> {
        HashMap::from([
            ("2-year".into(), 2.5),
            ("10-year".into(), 4.5),
            ("25-year".into(), 5.5),
            ("100-year".into(), 7.0),
        ])
    }

    #[test]
    fn no_detention_post_exceeds_pre() {
        let pre = WatershedInput {
            area_acres: 50.0,
            curve_number: 65.0,
            tc_hours: 0.5,
        };
        let post = WatershedInput {
            area_acres: 50.0,
            curve_number: 80.0,
            tc_hours: 0.3,
        };
        let result = run(&pre, &post, &storm_suite(), None);
        assert!(!result.all_pass);
    }

    #[test]
    fn detention_reduces_post_peak() {
        let pre = WatershedInput {
            area_acres: 50.0,
            curve_number: 65.0,
            tc_hours: 0.5,
        };
        let post = WatershedInput {
            area_acres: 50.0,
            curve_number: 80.0,
            tc_hours: 0.3,
        };
        let pond = PondConfiguration {
            max_storage_ft3: 200_000.0,
            avg_depth_ft: 10.0,
            outlets: vec![
                OutletDefinition::Orifice(OrificeOutlet {
                    name: "primary".into(),
                    diameter_inches: 4.0,
                    cd: 0.6,
                    invert_elev_ft: 0.0,
                }),
            ],
            routing_timestep_hours: 0.1,
        };
        let result = run(&pre, &post, &storm_suite(), Some(&pond));
        for row in &result.rows {
            assert!(row.post_development.peak_routed_cfs <= row.post_development.peak_unrouted_cfs);
        }
    }
}