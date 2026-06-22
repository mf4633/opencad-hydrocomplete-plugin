//! Detention pond routing via Modified Puls (storage indication method).

use std::collections::HashMap;

use crate::scs_unit_hydrograph::UnitHydrographResult;

pub const DEFAULT_TIMESTEP_HOURS: f64 = 0.1;
pub const DRAIN_STORAGE_TOLERANCE_FT3: f64 = 1.0;
pub const GRAVITY_FT_PER_SEC2: f64 = 32.2;

#[derive(Debug, Clone)]
pub struct HydrographPoint {
    pub time_hours: f64,
    pub flow_cfs: f64,
}

#[derive(Debug, Clone)]
pub struct ElevationAreaPoint {
    pub elevation_ft: f64,
    pub area_ft2: f64,
}

#[derive(Debug, Clone)]
pub struct StageStoragePoint {
    pub elevation_ft: f64,
    pub area_ft2: f64,
    pub storage_ft3: f64,
}

#[derive(Debug, Clone)]
pub struct StageStorageResult {
    pub points: Vec<StageStoragePoint>,
    pub total_storage_ft3: f64,
}

#[derive(Debug, Clone)]
pub struct OrificeOutlet {
    pub name: String,
    pub diameter_inches: f64,
    pub cd: f64,
    pub invert_elev_ft: f64,
}

#[derive(Debug, Clone)]
pub struct WeirOutlet {
    pub name: String,
    pub length_ft: f64,
    pub cw: f64,
    pub crest_elev_ft: f64,
}

#[derive(Debug, Clone)]
pub struct RiserOutlet {
    pub name: String,
    pub diameter_inches: f64,
    pub cd: f64,
    pub cw: f64,
    pub crest_elev_ft: f64,
}

#[derive(Debug, Clone)]
pub enum OutletDefinition {
    Orifice(OrificeOutlet),
    Weir(WeirOutlet),
    Riser(RiserOutlet),
}

#[derive(Debug, Clone)]
pub struct StorageIndicationPoint {
    pub elevation_ft: f64,
    pub storage_ft3: f64,
    pub outflow_cfs: f64,
    pub outlet_flows_cfs: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
pub struct RoutingOrdinate {
    pub time_hours: f64,
    pub inflow_cfs: f64,
    pub outflow_cfs: f64,
    pub storage_ft3: f64,
    pub elevation_ft: f64,
    pub outlet_flows_cfs: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
pub struct RoutingResult {
    pub peak_inflow_cfs: f64,
    pub peak_outflow_cfs: f64,
    pub peak_storage_ft3: f64,
    pub peak_elevation_ft: f64,
    pub reduction_percent: f64,
    pub timestep_hours: f64,
    pub ordinates: Vec<RoutingOrdinate>,
    pub outlet_hydrographs: HashMap<String, Vec<HydrographPoint>>,
}

pub fn orifice_discharge_cfs(cd: f64, diameter_inches: f64, head_ft: f64) -> f64 {
    if head_ft <= 0.0 {
        return 0.0;
    }
    let diameter_ft = diameter_inches / 12.0;
    let area_ft2 = std::f64::consts::PI * (diameter_ft / 2.0).powi(2);
    cd * area_ft2 * (2.0 * GRAVITY_FT_PER_SEC2 * head_ft).sqrt()
}

pub fn sharp_crested_weir_discharge_cfs(cw: f64, length_ft: f64, head_ft: f64) -> f64 {
    if head_ft <= 0.0 {
        return 0.0;
    }
    cw * length_ft * head_ft.powf(1.5)
}

pub fn riser_discharge_cfs(cd: f64, cw: f64, diameter_inches: f64, head_ft: f64) -> f64 {
    if head_ft <= 0.0 {
        return 0.0;
    }
    let diameter_ft = diameter_inches / 12.0;
    let perimeter_ft = std::f64::consts::PI * diameter_ft;
    let q_weir = sharp_crested_weir_discharge_cfs(cw, perimeter_ft, head_ft);
    let area_ft2 = std::f64::consts::PI * (diameter_ft / 2.0).powi(2);
    let q_orifice = cd * area_ft2 * (2.0 * GRAVITY_FT_PER_SEC2 * head_ft).sqrt();
    q_weir.min(q_orifice)
}

pub fn discharge_at_elevation(outlet: &OutletDefinition, elevation_ft: f64) -> f64 {
    match outlet {
        OutletDefinition::Orifice(o) => {
            orifice_discharge_cfs(o.cd, o.diameter_inches, elevation_ft - o.invert_elev_ft)
        }
        OutletDefinition::Weir(w) => {
            sharp_crested_weir_discharge_cfs(w.cw, w.length_ft, elevation_ft - w.crest_elev_ft)
        }
        OutletDefinition::Riser(r) => {
            riser_discharge_cfs(r.cd, r.cw, r.diameter_inches, elevation_ft - r.crest_elev_ft)
        }
    }
}

fn outlet_name(outlet: &OutletDefinition) -> String {
    match outlet {
        OutletDefinition::Orifice(o) if !o.name.is_empty() => o.name.clone(),
        OutletDefinition::Weir(w) if !w.name.is_empty() => w.name.clone(),
        OutletDefinition::Riser(r) if !r.name.is_empty() => r.name.clone(),
        OutletDefinition::Orifice(_) => "Orifice".into(),
        OutletDefinition::Weir(_) => "SharpCrestedWeir".into(),
        OutletDefinition::Riser(_) => "Riser".into(),
    }
}

pub fn average_end_area_volume(area1_ft2: f64, area2_ft2: f64, depth_ft: f64) -> f64 {
    ((area1_ft2 + area2_ft2) / 2.0) * depth_ft
}

pub fn build_from_elevation_area(elev_area_table: &[ElevationAreaPoint]) -> StageStorageResult {
    assert!(elev_area_table.len() >= 2, "need at least two elevation-area points");

    let mut sorted: Vec<_> = elev_area_table.to_vec();
    sorted.sort_by(|a, b| a.elevation_ft.partial_cmp(&b.elevation_ft).unwrap());

    let mut points = Vec::new();
    let mut cum_storage = 0.0;

    points.push(StageStoragePoint {
        elevation_ft: sorted[0].elevation_ft,
        area_ft2: sorted[0].area_ft2,
        storage_ft3: 0.0,
    });

    for i in 1..sorted.len() {
        let dh = sorted[i].elevation_ft - sorted[i - 1].elevation_ft;
        let increment =
            average_end_area_volume(sorted[i - 1].area_ft2, sorted[i].area_ft2, dh);
        cum_storage += increment;
        points.push(StageStoragePoint {
            elevation_ft: sorted[i].elevation_ft,
            area_ft2: sorted[i].area_ft2,
            storage_ft3: cum_storage,
        });
    }

    StageStorageResult {
        total_storage_ft3: cum_storage,
        points,
    }
}

pub fn interpolate_storage(elevation_ft: f64, table: &[StageStoragePoint]) -> f64 {
    if table.is_empty() {
        return 0.0;
    }
    if table.len() == 1 {
        return if elevation_ft <= table[0].elevation_ft {
            0.0
        } else {
            table[0].storage_ft3
        };
    }
    if elevation_ft <= table[0].elevation_ft {
        return 0.0;
    }
    let last = table[table.len() - 1].clone();
    if elevation_ft >= last.elevation_ft {
        return last.storage_ft3 + last.area_ft2 * (elevation_ft - last.elevation_ft);
    }
    for i in 1..table.len() {
        if table[i].elevation_ft >= elevation_ft {
            let e0 = table[i - 1].elevation_ft;
            let e1 = table[i].elevation_ft;
            let s0 = table[i - 1].storage_ft3;
            let s1 = table[i].storage_ft3;
            if e1 > e0 {
                let f = (elevation_ft - e0) / (e1 - e0);
                return s0 + f * (s1 - s0);
            }
            return s0;
        }
    }
    0.0
}

pub fn build_storage_indication_curve(
    stage_storage: &[StageStoragePoint],
    outlets: &[OutletDefinition],
    max_elev_ft: Option<f64>,
    elev_step_ft: Option<f64>,
) -> Vec<StorageIndicationPoint> {
    assert!(stage_storage.len() >= 2);

    let min_elev = stage_storage[0].elevation_ft;
    let max_elev = max_elev_ft.unwrap_or(stage_storage[stage_storage.len() - 1].elevation_ft * 1.2);
    let step = elev_step_ft.unwrap_or((max_elev - min_elev).max(0.1) / 64.0);

    let mut curve = Vec::new();
    let mut elev = min_elev;
    while elev <= max_elev + 1e-9 {
        let storage = interpolate_storage(elev, stage_storage);
        let mut outlet_flows = HashMap::new();
        let mut total = 0.0;
        for outlet in outlets {
            let name = outlet_name(outlet);
            let q = discharge_at_elevation(outlet, elev);
            outlet_flows.insert(name, q);
            total += q;
        }
        curve.push(StorageIndicationPoint {
            elevation_ft: elev,
            storage_ft3: storage,
            outflow_cfs: total,
            outlet_flows_cfs: outlet_flows,
        });
        elev += step;
    }
    curve
}

pub fn build_prismatic_storage_indication_curve(
    max_storage_ft3: f64,
    outlets: &[OutletDefinition],
    avg_depth_ft: f64,
) -> Vec<StorageIndicationPoint> {
    assert!(max_storage_ft3 > 0.0 && avg_depth_ft > 0.0);
    let surface_area = max_storage_ft3 / avg_depth_ft;
    let max_elev = avg_depth_ft * 1.5;
    let table = vec![
        StageStoragePoint {
            elevation_ft: 0.0,
            area_ft2: surface_area,
            storage_ft3: 0.0,
        },
        StageStoragePoint {
            elevation_ft: max_elev,
            area_ft2: surface_area,
            storage_ft3: surface_area * max_elev,
        },
    ];
    build_storage_indication_curve(
        &table,
        outlets,
        Some(max_elev),
        Some((avg_depth_ft / 32.0).max(0.25)),
    )
}

pub fn default_nc_detention_outlets() -> Vec<OutletDefinition> {
    vec![OutletDefinition::Orifice(OrificeOutlet {
        name: "primary".into(),
        diameter_inches: 4.0,
        cd: 0.6,
        invert_elev_ft: 0.0,
    })]
}

fn interpolate_flow(hydrograph: &[HydrographPoint], time_hours: f64) -> f64 {
    if hydrograph.is_empty() {
        return 0.0;
    }
    for j in 1..hydrograph.len() {
        if hydrograph[j].time_hours >= time_hours {
            let t0 = hydrograph[j - 1].time_hours;
            let t1 = hydrograph[j].time_hours;
            let f0 = hydrograph[j - 1].flow_cfs;
            let f1 = hydrograph[j].flow_cfs;
            if t1 > t0 {
                return f0 + (f1 - f0) * (time_hours - t0) / (t1 - t0);
            }
            return f0;
        }
    }
    0.0
}

fn resample_inflow(inflow: &[HydrographPoint], timestep_hours: f64) -> Vec<HydrographPoint> {
    let max_time = inflow[inflow.len() - 1].time_hours;
    let mut uniform = Vec::new();
    let mut t = 0.0;
    while t <= max_time + timestep_hours {
        let flow = if t > max_time {
            0.0
        } else {
            interpolate_flow(inflow, t)
        };
        uniform.push(HydrographPoint {
            time_hours: t,
            flow_cfs: flow.max(0.0),
        });
        t += timestep_hours;
    }
    uniform
}

fn interpolate_outlet_flows(
    prev: &StorageIndicationPoint,
    curr: &StorageIndicationPoint,
    fraction: f64,
    has_outlet_flows: bool,
) -> HashMap<String, f64> {
    let mut result = HashMap::new();
    if !has_outlet_flows {
        return result;
    }
    for (name, cv) in &curr.outlet_flows_cfs {
        let pv = prev.outlet_flows_cfs.get(name).copied().unwrap_or(0.0);
        result.insert(name.clone(), pv + fraction * (cv - pv));
    }
    result
}

fn solve_storage_indication(
    left_side: f64,
    storage_curve: &[StorageIndicationPoint],
    dt_seconds: f64,
) -> StorageIndicationPoint {
    let has_outlet_flows = !storage_curve[0].outlet_flows_cfs.is_empty();

    for i in 1..storage_curve.len() {
        let point = &storage_curve[i];
        let prev = &storage_curve[i - 1];
        let indicator = 2.0 * point.storage_ft3 / dt_seconds + point.outflow_cfs;
        let prev_indicator = 2.0 * prev.storage_ft3 / dt_seconds + prev.outflow_cfs;
        if indicator >= left_side {
            let denom = indicator - prev_indicator;
            let fraction = if denom.abs() < 1e-12 {
                0.0
            } else {
                (left_side - prev_indicator) / denom
            };
            let mut solved = StorageIndicationPoint {
                storage_ft3: prev.storage_ft3 + fraction * (point.storage_ft3 - prev.storage_ft3),
                outflow_cfs: prev.outflow_cfs + fraction * (point.outflow_cfs - prev.outflow_cfs),
                elevation_ft: prev.elevation_ft + fraction * (point.elevation_ft - prev.elevation_ft),
                outlet_flows_cfs: interpolate_outlet_flows(prev, point, fraction, has_outlet_flows),
            };
            return solved;
        }
    }

    let n = storage_curve.len();
    let last = &storage_curve[n - 1];
    let prev_last = &storage_curve[n - 2];
    let last_indicator = 2.0 * last.storage_ft3 / dt_seconds + last.outflow_cfs;
    let prev_last_indicator = 2.0 * prev_last.storage_ft3 / dt_seconds + prev_last.outflow_cfs;
    let slope = last_indicator - prev_last_indicator;

    if slope <= 0.0 {
        return last.clone();
    }

    let frac = (left_side - last_indicator) / slope;
    StorageIndicationPoint {
        storage_ft3: last.storage_ft3 + frac * (last.storage_ft3 - prev_last.storage_ft3),
        outflow_cfs: last.outflow_cfs + frac * (last.outflow_cfs - prev_last.outflow_cfs),
        elevation_ft: last.elevation_ft + frac * (last.elevation_ft - prev_last.elevation_ft),
        outlet_flows_cfs: interpolate_outlet_flows(prev_last, last, frac, has_outlet_flows),
    }
}

pub fn route(
    inflow_hydrograph: &[HydrographPoint],
    storage_curve: &[StorageIndicationPoint],
    timestep_hours: f64,
) -> RoutingResult {
    assert!(storage_curve.len() >= 2);
    assert!(timestep_hours > 0.0);

    if inflow_hydrograph.is_empty() {
        return RoutingResult {
            peak_inflow_cfs: 0.0,
            peak_outflow_cfs: 0.0,
            peak_storage_ft3: 0.0,
            peak_elevation_ft: 0.0,
            reduction_percent: 0.0,
            timestep_hours,
            ordinates: Vec::new(),
            outlet_hydrographs: HashMap::new(),
        };
    }

    let dt_seconds = timestep_hours * 3600.0;
    let uniform_inflow = resample_inflow(inflow_hydrograph, timestep_hours);

    let mut s1 = 0.0;
    let mut o1 = 0.0;
    let mut routing = Vec::new();

    for (i, pt) in uniform_inflow.iter().enumerate() {
        let i1 = if i > 0 {
            uniform_inflow[i - 1].flow_cfs
        } else {
            0.0
        };
        let i2 = pt.flow_cfs;
        let left_side = 2.0 * s1 / dt_seconds - o1 + (i1 + i2);
        let solved = solve_storage_indication(left_side, storage_curve, dt_seconds);
        let entry = RoutingOrdinate {
            time_hours: pt.time_hours,
            inflow_cfs: i2,
            outflow_cfs: solved.outflow_cfs.max(0.0),
            storage_ft3: solved.storage_ft3.max(0.0),
            elevation_ft: solved.elevation_ft.max(0.0),
            outlet_flows_cfs: solved
                .outlet_flows_cfs
                .iter()
                .map(|(k, v)| (k.clone(), v.max(0.0)))
                .collect(),
        };
        s1 = entry.storage_ft3;
        o1 = entry.outflow_cfs;
        routing.push(entry);
    }

    let mut max_drain = 5000;
    while s1 > DRAIN_STORAGE_TOLERANCE_FT3 && max_drain > 0 {
        max_drain -= 1;
        let last_time = routing[routing.len() - 1].time_hours + timestep_hours;
        let i1 = routing[routing.len() - 1].inflow_cfs;
        let left_side = 2.0 * s1 / dt_seconds - o1 + i1;
        let solved = solve_storage_indication(left_side, storage_curve, dt_seconds);
        let entry = RoutingOrdinate {
            time_hours: last_time,
            inflow_cfs: 0.0,
            outflow_cfs: solved.outflow_cfs.max(0.0),
            storage_ft3: solved.storage_ft3.max(0.0),
            elevation_ft: solved.elevation_ft.max(0.0),
            outlet_flows_cfs: solved
                .outlet_flows_cfs
                .iter()
                .map(|(k, v)| (k.clone(), v.max(0.0)))
                .collect(),
        };
        s1 = entry.storage_ft3;
        o1 = entry.outflow_cfs;
        routing.push(entry);
    }

    let peak_inflow = inflow_hydrograph
        .iter()
        .map(|p| p.flow_cfs)
        .fold(0.0_f64, f64::max);
    let peak_outflow = routing.iter().map(|p| p.outflow_cfs).fold(0.0_f64, f64::max);
    let peak_storage = routing.iter().map(|p| p.storage_ft3).fold(0.0_f64, f64::max);
    let peak_elevation = routing.iter().map(|p| p.elevation_ft).fold(0.0_f64, f64::max);
    let reduction = if peak_inflow > 0.0 {
        (1.0 - peak_outflow / peak_inflow) * 100.0
    } else {
        0.0
    };

    let mut outlet_hydrographs = HashMap::new();
    if let Some(first) = routing.first() {
        for name in first.outlet_flows_cfs.keys() {
            let hydro: Vec<_> = routing
                .iter()
                .map(|ord| HydrographPoint {
                    time_hours: ord.time_hours,
                    flow_cfs: ord.outlet_flows_cfs.get(name).copied().unwrap_or(0.0).max(0.0),
                })
                .collect();
            outlet_hydrographs.insert(name.clone(), hydro);
        }
    }

    RoutingResult {
        peak_inflow_cfs: peak_inflow,
        peak_outflow_cfs: peak_outflow,
        peak_storage_ft3: peak_storage,
        peak_elevation_ft: peak_elevation,
        reduction_percent: reduction,
        timestep_hours,
        ordinates: routing,
        outlet_hydrographs,
    }
}

pub fn inflow_from_unit_hydrograph(
    unit_hydrograph: &UnitHydrographResult,
    runoff_depth_inches: f64,
) -> Vec<HydrographPoint> {
    unit_hydrograph
        .ordinates
        .iter()
        .map(|o| HydrographPoint {
            time_hours: o.time_minutes / 60.0,
            flow_cfs: o.flow_cfs * runoff_depth_inches,
        })
        .collect()
}

pub fn continuity_error_percent(routing: &RoutingResult) -> f64 {
    if routing.ordinates.len() < 2 {
        return 0.0;
    }
    let mut in_vol = 0.0;
    let mut out_vol = 0.0;
    for i in 1..routing.ordinates.len() {
        let dt = (routing.ordinates[i].time_hours - routing.ordinates[i - 1].time_hours) * 3600.0;
        in_vol += (routing.ordinates[i].inflow_cfs + routing.ordinates[i - 1].inflow_cfs) / 2.0 * dt;
        out_vol += (routing.ordinates[i].outflow_cfs + routing.ordinates[i - 1].outflow_cfs) / 2.0
            * dt;
    }
    let final_storage = routing.ordinates[routing.ordinates.len() - 1].storage_ft3;
    if in_vol <= 0.0 {
        return 0.0;
    }
    ((in_vol - (out_vol + final_storage)) / in_vol * 100.0).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orifice_matches_hand_calc() {
        let q = orifice_discharge_cfs(0.6, 12.0, 4.0);
        assert!((q - 7.56).abs() < 0.1);
    }

    #[test]
    fn weir_matches_hand_calc() {
        let q = sharp_crested_weir_discharge_cfs(3.0, 8.0, 2.0);
        assert!((q - 67.88).abs() < 0.1);
    }

    #[test]
    fn stage_storage_average_end_area() {
        let table = build_from_elevation_area(&[
            ElevationAreaPoint {
                elevation_ft: 100.0,
                area_ft2: 0.0,
            },
            ElevationAreaPoint {
                elevation_ft: 101.0,
                area_ft2: 1000.0,
            },
            ElevationAreaPoint {
                elevation_ft: 102.0,
                area_ft2: 3000.0,
            },
        ]);
        assert!((table.points[1].storage_ft3 - 500.0).abs() < 1.0);
        assert!((table.points[2].storage_ft3 - 2500.0).abs() < 1.0);
    }

    #[test]
    fn routing_attenuates_peak() {
        let outlets = default_nc_detention_outlets();
        let curve = build_prismatic_storage_indication_curve(50_000.0, &outlets, 8.0);
        let inflow = vec![
            HydrographPoint {
                time_hours: 0.0,
                flow_cfs: 0.0,
            },
            HydrographPoint {
                time_hours: 0.5,
                flow_cfs: 100.0,
            },
            HydrographPoint {
                time_hours: 1.0,
                flow_cfs: 0.0,
            },
        ];
        let result = route(&inflow, &curve, 0.1);
        assert!(result.peak_outflow_cfs <= result.peak_inflow_cfs);
    }
}