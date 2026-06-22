//! Simplified hydrograph routing through a pipe network (lag + superposition).

use std::collections::{HashMap, VecDeque};

use crate::hydrograph_convolution::{
    build_unit_hydrograph, generate_tr20_hydrograph, resolve_curve_number, UnitHydrographMethod,
};
use crate::manning;
use crate::models::{Catchment, PipeSegment};

#[derive(Debug, Clone)]
pub struct RoutedHydrographOrdinate {
    pub time_minutes: f64,
    pub flow_cfs: f64,
}

#[derive(Debug, Clone)]
pub struct HydrographRouterOptions {
    pub storm_depth_in: f64,
    pub timestep_hours: f64,
    pub unit_hydro_method: UnitHydrographMethod,
    pub default_tc_minutes: f64,
}

impl Default for HydrographRouterOptions {
    fn default() -> Self {
        Self {
            storm_depth_in: 5.0,
            timestep_hours: 0.25,
            unit_hydro_method: UnitHydrographMethod::Scs,
            default_tc_minutes: 10.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RouterPipe {
    pub pipe_key: String,
    pub network_name: String,
    pub pipe_name: String,
    pub upstream_node_id: String,
    pub downstream_node_id: String,
    pub segment: PipeSegment,
    pub length_ft: f64,
}

#[derive(Debug, Clone)]
pub struct PipeHydrographResult {
    pub pipe_key: String,
    pub network_name: String,
    pub pipe_name: String,
    pub peak_flow_cfs: f64,
    pub time_to_peak_minutes: f64,
    pub volume_acre_ft: f64,
    pub travel_time_minutes: f64,
    pub ordinates: Vec<RoutedHydrographOrdinate>,
}

#[derive(Debug, Clone)]
pub struct CatchmentHydrographResult {
    pub catchment_name: String,
    pub curve_number: f64,
    pub assigned_structure_id: Option<String>,
    pub peak_flow_cfs: f64,
}

#[derive(Debug, Clone)]
pub struct HydrographRouterResult {
    pub catchment_hydrographs: Vec<CatchmentHydrographResult>,
    pub pipe_hydrographs: HashMap<String, PipeHydrographResult>,
}

pub fn combine_hydrographs(a: &[f64], b: &[f64]) -> Vec<f64> {
    let n = a.len().max(b.len());
    let mut combined = vec![0.0; n];
    for i in 0..n {
        let qa = if i < a.len() { a[i] } else { 0.0 };
        let qb = if i < b.len() { b[i] } else { 0.0 };
        combined[i] = (qa + qb).max(0.0);
    }
    combined
}

pub fn combine_at_junction(branches: &[Vec<f64>]) -> Vec<f64> {
    let mut combined = Vec::new();
    for branch in branches {
        if branch.is_empty() {
            continue;
        }
        combined = if combined.is_empty() {
            branch.clone()
        } else {
            combine_hydrographs(&combined, branch)
        };
    }
    combined
}

pub fn shift_lag_hydrograph(flows: &[f64], dt_hours: f64, lag_hours: f64) -> Vec<f64> {
    assert!(dt_hours > 0.0 && lag_hours >= 0.0);
    if flows.is_empty() {
        return Vec::new();
    }
    let shift_steps = (lag_hours / dt_hours).round() as usize;
    if shift_steps == 0 {
        return flows.iter().map(|&q| q.max(0.0)).collect();
    }
    let mut shifted = vec![0.0; flows.len() + shift_steps];
    for (i, &q) in flows.iter().enumerate() {
        shifted[i + shift_steps] = q.max(0.0);
    }
    shifted
}

fn pipe_travel_time_minutes(pipe: &RouterPipe) -> f64 {
    let velocity = resolve_velocity_fps(pipe);
    if pipe.length_ft <= 0.0 || velocity <= 0.0 {
        0.0
    } else {
        pipe.length_ft / velocity / 60.0
    }
}

fn resolve_velocity_fps(pipe: &RouterPipe) -> f64 {
    let slope = resolve_slope(pipe);
    if pipe.segment.diameter_ft > 0.0 && slope > 0.0 && pipe.segment.manning_n > 0.0 {
        manning::capacity(&pipe.segment).full_velocity_fps
    } else {
        4.0
    }
}

fn resolve_slope(pipe: &RouterPipe) -> f64 {
    if pipe.segment.slope > 0.0 {
        pipe.segment.slope
    } else if pipe.length_ft > 0.0 {
        let drop = pipe.segment.start_invert_ft - pipe.segment.end_invert_ft;
        if drop > 0.0 {
            drop / pipe.length_ft
        } else {
            0.0
        }
    } else {
        0.0
    }
}

fn to_ordinates(flows: &[f64], dt_hours: f64) -> Vec<RoutedHydrographOrdinate> {
    let mut ordinates = Vec::new();
    for (i, &q) in flows.iter().enumerate() {
        let q = q.max(0.0);
        if q > 0.001 || i == 0 {
            ordinates.push(RoutedHydrographOrdinate {
                time_minutes: i as f64 * dt_hours * 60.0,
                flow_cfs: q,
            });
        }
    }
    if ordinates.is_empty() {
        ordinates.push(RoutedHydrographOrdinate {
            time_minutes: 0.0,
            flow_cfs: 0.0,
        });
    }
    ordinates
}

fn hydro_to_flow_series(
    hydro: &crate::hydrograph_convolution::ConvolutionResult,
    dt_hours: f64,
) -> Vec<f64> {
    if hydro.ordinates.is_empty() {
        return Vec::new();
    }
    let max_time = hydro
        .ordinates
        .iter()
        .map(|o| o.time_hours)
        .fold(0.0_f64, f64::max);
    let steps = (max_time / dt_hours).ceil() as usize + 1;
    let mut flows = vec![0.0; steps];
    for ord in &hydro.ordinates {
        let idx = (ord.time_hours / dt_hours).round() as isize;
        if idx >= 0 && (idx as usize) < steps {
            flows[idx as usize] = f64::max(flows[idx as usize], ord.flow_cfs);
        }
    }
    flows
}

pub fn route(
    catchments: &[Catchment],
    pipes: &[RouterPipe],
    options: &HydrographRouterOptions,
) -> HydrographRouterResult {
    assert!(!catchments.is_empty() && options.timestep_hours > 0.0);
    let dt_hours = options.timestep_hours;

    let mut catchment_results = Vec::new();
    let mut tributary: HashMap<String, Vec<Vec<f64>>> = HashMap::new();

    for cm in catchments {
        let cn = resolve_curve_number(cm);
        let tc = if cm.tc_minutes > 0.0 {
            cm.tc_minutes
        } else {
            options.default_tc_minutes
        };
        let hydro = generate_tr20_hydrograph(
            cm.area_acres,
            cn,
            tc,
            options.storm_depth_in,
            dt_hours,
            options.unit_hydro_method,
        );
        let series = hydro_to_flow_series(&hydro, dt_hours);
        let struct_id = format!("headwater::{}", cm.name);
        tributary.entry(struct_id.clone()).or_default().push(series);
        catchment_results.push(CatchmentHydrographResult {
            catchment_name: cm.name.clone(),
            curve_number: cn,
            assigned_structure_id: Some(struct_id),
            peak_flow_cfs: hydro.peak_flow_cfs,
        });
    }

    let mut pipe_results = HashMap::new();
    if pipes.is_empty() {
        return HydrographRouterResult {
            catchment_hydrographs: catchment_results,
            pipe_hydrographs: pipe_results,
        };
    }

    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut by_upstream: HashMap<String, Vec<&RouterPipe>> = HashMap::new();
    for pipe in pipes {
        *in_degree.entry(pipe.downstream_node_id.clone()).or_insert(0) += 1;
        in_degree.entry(pipe.upstream_node_id.clone()).or_insert(0);
        by_upstream
            .entry(pipe.upstream_node_id.clone())
            .or_default()
            .push(pipe);
    }

    let mut inflow_series: HashMap<String, Vec<Vec<f64>>> = HashMap::new();
    for (k, v) in tributary {
        inflow_series.entry(k).or_default().extend(v);
    }

    let mut ready: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();
    ready.make_contiguous().sort();

    while let Some(struct_id) = ready.pop_front() {
        let branches = inflow_series.get(&struct_id).cloned().unwrap_or_default();
        let struct_inflow = combine_at_junction(&branches);

        if let Some(outgoing) = by_upstream.get(&struct_id) {
            for pipe in outgoing {
                let lag_hours = pipe_travel_time_minutes(pipe) / 60.0;
                let pipe_out = shift_lag_hydrograph(&struct_inflow, dt_hours, lag_hours);
                let ordinates = to_ordinates(&pipe_out, dt_hours);
                let peak = ordinates
                    .iter()
                    .max_by(|a, b| a.flow_cfs.partial_cmp(&b.flow_cfs).unwrap())
                    .cloned()
                    .unwrap_or(RoutedHydrographOrdinate {
                        time_minutes: 0.0,
                        flow_cfs: 0.0,
                    });
                pipe_results.insert(
                    pipe.pipe_key.clone(),
                    PipeHydrographResult {
                        pipe_key: pipe.pipe_key.clone(),
                        network_name: pipe.network_name.clone(),
                        pipe_name: pipe.pipe_name.clone(),
                        peak_flow_cfs: peak.flow_cfs,
                        time_to_peak_minutes: peak.time_minutes,
                        volume_acre_ft: crate::hydrograph_convolution::hydrograph_volume_acre_ft(
                            &pipe_out, dt_hours,
                        ),
                        travel_time_minutes: pipe_travel_time_minutes(pipe),
                        ordinates,
                    },
                );
                inflow_series
                    .entry(pipe.downstream_node_id.clone())
                    .or_default()
                    .push(pipe_out);
                if let Some(deg) = in_degree.get_mut(&pipe.downstream_node_id) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.push_back(pipe.downstream_node_id.clone());
                    }
                }
            }
        }
    }

    HydrographRouterResult {
        catchment_hydrographs: catchment_results,
        pipe_hydrographs: pipe_results,
    }
}