//! Snyder (1938) synthetic unit hydrograph.

use crate::scs_unit_hydrograph::acres_to_sq_mi;

pub const DEFAULT_CT: f64 = 1.8;
pub const DEFAULT_CP: f64 = 0.6;
pub const CHANNEL_LENGTH_FACTOR: f64 = 1.5;
pub const CENTROID_DISTANCE_FACTOR: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct HydrographOrdinate {
    pub time_hours: f64,
    pub flow_cfs: f64,
    pub relative_time: f64,
    pub relative_flow: f64,
}

#[derive(Debug, Clone)]
pub struct UnitHydrographResult {
    pub area_acres: f64,
    pub channel_length_mi: f64,
    pub centroid_distance_mi: f64,
    pub ct: f64,
    pub cp: f64,
    pub lag_hours: f64,
    pub time_to_peak_hours: f64,
    pub base_time_hours: f64,
    pub width_50_hours: f64,
    pub width_75_hours: f64,
    pub peak_flow_cfs: f64,
    pub time_step_hours: f64,
    pub ordinates: Vec<HydrographOrdinate>,
}

pub fn estimate_channel_length_mi(area_acres: f64) -> f64 {
    CHANNEL_LENGTH_FACTOR * acres_to_sq_mi(area_acres).powf(0.6)
}

pub fn lag_hours(channel_length_mi: f64, centroid_distance_mi: f64, ct: f64) -> f64 {
    assert!(channel_length_mi > 0.0 && centroid_distance_mi > 0.0 && ct > 0.0);
    ct * (channel_length_mi * centroid_distance_mi).powf(0.3)
}

pub fn peak_discharge_cfs(area_acres: f64, lag_hours: f64, cp: f64) -> f64 {
    assert!(area_acres > 0.0 && lag_hours > 0.0 && cp > 0.0);
    cp * 640.0 * acres_to_sq_mi(area_acres) / lag_hours
}

pub fn width_50_hours(lag_hours: f64) -> f64 {
    2.14 * lag_hours
}

pub fn width_75_hours(lag_hours: f64) -> f64 {
    1.37 * lag_hours
}

pub fn base_time_hours(lag_hours: f64) -> f64 {
    (5.0 * lag_hours).max(3.0)
}

pub fn generate(
    area_acres: f64,
    channel_length_mi: Option<f64>,
    centroid_distance_mi: Option<f64>,
    ct: f64,
    cp: f64,
    time_step_hours: Option<f64>,
) -> UnitHydrographResult {
    assert!(area_acres > 0.0);
    let l_mi = channel_length_mi.unwrap_or_else(|| estimate_channel_length_mi(area_acres));
    let lc_mi = centroid_distance_mi.unwrap_or(l_mi * CENTROID_DISTANCE_FACTOR);
    let tp = lag_hours(l_mi, lc_mi, ct);
    let tb = base_time_hours(tp);
    let qp = peak_discharge_cfs(area_acres, tp, cp);
    let dt = time_step_hours.unwrap_or(tp / 10.0).max(0.05);
    let recession_hours = (tb - tp).max(dt);

    let mut ordinates = Vec::new();
    let mut t = 0.0;
    while t <= tb + 1e-9 {
        let (q, rel) = if t <= tp {
            let rel = if tp > 0.0 { t / tp } else { 0.0 };
            (qp * rel, rel)
        } else {
            let rel = if recession_hours > 0.0 {
                1.0 - (t - tp) / recession_hours
            } else {
                0.0
            };
            (qp * rel.max(0.0), rel.max(0.0))
        };
        ordinates.push(HydrographOrdinate {
            time_hours: t,
            flow_cfs: q,
            relative_time: if tp > 0.0 { t / tp } else { 0.0 },
            relative_flow: if qp > 0.0 { q / qp } else { 0.0 },
        });
        t += dt;
    }

    UnitHydrographResult {
        area_acres,
        channel_length_mi: l_mi,
        centroid_distance_mi: lc_mi,
        ct,
        cp,
        lag_hours: tp,
        time_to_peak_hours: tp,
        base_time_hours: tb,
        width_50_hours: width_50_hours(tp),
        width_75_hours: width_75_hours(tp),
        peak_flow_cfs: qp,
        time_step_hours: dt,
        ordinates,
    }
}