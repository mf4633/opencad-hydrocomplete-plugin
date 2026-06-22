//! TR-20 style hydrograph generation (convolution).

use crate::clark_unit_hydrograph;
use crate::models::Catchment;
use crate::scs_runoff::{cumulative_runoff_depth, initial_abstraction_from_cn};
use crate::scs_unit_hydrograph;
use crate::snyder_unit_hydrograph;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitHydrographMethod {
    Scs,
    Snyder,
    Clark,
}

#[derive(Debug, Clone)]
pub struct HydrographOrdinate {
    pub time_hours: f64,
    pub flow_cfs: f64,
}

#[derive(Debug, Clone)]
pub struct UnitHydrographInput {
    pub time_hours: f64,
    pub flow_cfs_per_in: f64,
}

#[derive(Debug, Clone)]
pub struct ConvolutionResult {
    pub area_acres: f64,
    pub timestep_hours: f64,
    pub total_excess_rainfall_in: f64,
    pub peak_flow_cfs: f64,
    pub time_to_peak_hours: f64,
    pub volume_acre_ft: f64,
    pub ordinates: Vec<HydrographOrdinate>,
}

pub fn interpolate_unit_hydrograph(ordinates: &[UnitHydrographInput], elapsed_hours: f64) -> f64 {
    if ordinates.is_empty() || elapsed_hours < ordinates[0].time_hours {
        return 0.0;
    }
    if elapsed_hours >= ordinates.last().unwrap().time_hours {
        return 0.0;
    }
    for w in ordinates.windows(2) {
        let t1 = w[0].time_hours;
        let t2 = w[1].time_hours;
        if elapsed_hours >= t1 && elapsed_hours <= t2 {
            let q1 = w[0].flow_cfs_per_in;
            let q2 = w[1].flow_cfs_per_in;
            let frac = if t2 > t1 {
                (elapsed_hours - t1) / (t2 - t1)
            } else {
                0.0
            };
            return q1 + frac * (q2 - q1);
        }
    }
    0.0
}

pub fn convolve(
    excess_rainfall_in: &[f64],
    excess_start_time_hours: f64,
    timestep_hours: f64,
    unit_hydro_ordinates: &[UnitHydrographInput],
    area_acres: f64,
) -> ConvolutionResult {
    assert!(!unit_hydro_ordinates.is_empty() && timestep_hours > 0.0 && area_acres > 0.0);
    let max_uh_time = unit_hydro_ordinates.last().unwrap().time_hours;
    let storm_end = excess_start_time_hours + excess_rainfall_in.len() as f64 * timestep_hours;
    let max_time = storm_end + max_uh_time;
    let out_steps = (max_time / timestep_hours).ceil() as usize + 1;
    let mut flows = vec![0.0; out_steps];

    for (j, &excess) in excess_rainfall_in.iter().enumerate() {
        if excess <= 0.0 {
            continue;
        }
        let start_time = excess_start_time_hours + j as f64 * timestep_hours;
        for uh in unit_hydro_ordinates.iter().take(unit_hydro_ordinates.len().saturating_sub(1)) {
            let out_idx = ((start_time + uh.time_hours) / timestep_hours).round() as isize;
            if out_idx >= 0 && (out_idx as usize) < out_steps {
                flows[out_idx as usize] += excess * uh.flow_cfs_per_in;
            }
        }
    }

    let total_excess: f64 = excess_rainfall_in.iter().sum();
    let mut ordinates = Vec::new();
    for (i, &q) in flows.iter().enumerate() {
        let t = i as f64 * timestep_hours;
        let q = q.max(0.0);
        if q > 0.001 || t < 1.0 {
            ordinates.push(HydrographOrdinate {
                time_hours: t,
                flow_cfs: q,
            });
        }
    }
    if ordinates.is_empty() {
        ordinates.push(HydrographOrdinate {
            time_hours: 0.0,
            flow_cfs: 0.0,
        });
    }
    let peak = ordinates
        .iter()
        .max_by(|a, b| a.flow_cfs.partial_cmp(&b.flow_cfs).unwrap())
        .cloned()
        .unwrap();
    ConvolutionResult {
        area_acres,
        timestep_hours,
        total_excess_rainfall_in: total_excess,
        peak_flow_cfs: peak.flow_cfs,
        time_to_peak_hours: peak.time_hours,
        volume_acre_ft: hydrograph_volume_acre_ft(&flows, timestep_hours),
        ordinates,
    }
}

pub fn hydrograph_volume_acre_ft(flows: &[f64], dt_hours: f64) -> f64 {
    let dt_sec = dt_hours * 3600.0;
    let cf: f64 = flows.iter().map(|&q| q * dt_sec).sum();
    cf / 43560.0
}

pub fn build_unit_hydrograph(
    method: UnitHydrographMethod,
    area_acres: f64,
    tc_minutes: f64,
    timestep_hours: f64,
) -> Vec<UnitHydrographInput> {
    match method {
        UnitHydrographMethod::Snyder => snyder_unit_hydrograph::generate(area_acres, None, None, 1.8, 0.6, Some(timestep_hours))
            .ordinates
            .into_iter()
            .map(|o| UnitHydrographInput {
                time_hours: o.time_hours,
                flow_cfs_per_in: o.flow_cfs,
            })
            .collect(),
        UnitHydrographMethod::Clark => clark_unit_hydrograph::generate(
            area_acres,
            tc_minutes,
            timestep_hours * 60.0,
            0.4,
            None,
        )
        .ordinates
        .into_iter()
        .map(|o| UnitHydrographInput {
            time_hours: o.time_minutes / 60.0,
            flow_cfs_per_in: o.flow_cfs,
        })
        .collect(),
        UnitHydrographMethod::Scs => scs_unit_hydrograph::generate(
            area_acres,
            tc_minutes,
            None,
            Some(timestep_hours * 60.0),
        )
        .ordinates
        .into_iter()
        .map(|o| UnitHydrographInput {
            time_hours: o.time_minutes / 60.0,
            flow_cfs_per_in: o.flow_cfs,
        })
        .collect(),
    }
}

/// Uniform 24-hr storm hyetograph with SCS incremental losses.
fn incremental_excess_rainfall(
    total_rainfall_in: f64,
    curve_number: f64,
    timestep_hours: f64,
    storm_duration_hours: f64,
) -> Vec<f64> {
    let steps = (storm_duration_hours / timestep_hours).ceil() as usize;
    let depth_per_step = total_rainfall_in / steps as f64;
    let ia = initial_abstraction_from_cn(curve_number);
    let mut cumulative = 0.0;
    let mut prev_runoff = 0.0;
    let mut excess = Vec::new();
    for _ in 0..steps {
        cumulative += depth_per_step;
        let runoff = cumulative_runoff_depth(cumulative, curve_number);
        let incr = (runoff - prev_runoff).max(0.0);
        if cumulative > ia || incr > 0.0 {
            excess.push(incr);
        } else {
            excess.push(0.0);
        }
        prev_runoff = runoff;
    }
    excess
}

pub fn resolve_curve_number(catchment: &Catchment) -> f64 {
    if catchment.curve_number > 0.0 {
        catchment.curve_number
    } else {
        (207.0 / catchment.runoff_c - 10.0).clamp(30.0, 98.0)
    }
}

pub fn generate_tr20_hydrograph(
    area_acres: f64,
    curve_number: f64,
    tc_minutes: f64,
    total_rainfall_in: f64,
    timestep_hours: f64,
    unit_hydro_method: UnitHydrographMethod,
) -> ConvolutionResult {
    assert!(curve_number > 0.0 && curve_number <= 100.0 && total_rainfall_in >= 0.0);
    let excess = incremental_excess_rainfall(total_rainfall_in, curve_number, timestep_hours, 24.0);
    let uh = build_unit_hydrograph(unit_hydro_method, area_acres, tc_minutes, timestep_hours);
    convolve(&excess, 0.0, timestep_hours, &uh, area_acres)
}

pub fn cn_from_catchments(catchments: &[Catchment]) -> f64 {
    let total_area: f64 = catchments.iter().map(|c| c.area_acres).sum();
    if total_area <= 0.0 {
        return 75.0;
    }
    catchments
        .iter()
        .map(|c| resolve_curve_number(c) * c.area_acres)
        .sum::<f64>()
        / total_area
}