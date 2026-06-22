//! NRCS/TR-55 synthetic unit hydrograph.

pub const PEAK_FACTOR: f64 = 484.0;
pub const LAG_FACTOR: f64 = 0.6;
pub const DEFAULT_DURATION_FACTOR: f64 = 0.133;
pub const TOTAL_DURATION_FACTOR: f64 = 5.0;
pub const DEFAULT_TIME_STEP_FACTOR: f64 = 0.2;

const DIMENSIONLESS_CURVE: &[(f64, f64)] = &[
    (0.0, 0.00),
    (0.1, 0.03),
    (0.2, 0.10),
    (0.3, 0.30),
    (0.4, 0.53),
    (0.5, 0.72),
    (0.6, 0.86),
    (0.7, 0.94),
    (0.8, 0.97),
    (0.9, 0.99),
    (1.0, 1.00),
    (1.1, 0.99),
    (1.2, 0.93),
    (1.3, 0.86),
    (1.4, 0.78),
    (1.5, 0.68),
    (1.6, 0.56),
    (1.7, 0.46),
    (1.8, 0.35),
    (1.9, 0.26),
    (2.0, 0.17),
    (2.2, 0.07),
    (2.4, 0.02),
    (2.6, 0.00),
];

#[derive(Debug, Clone)]
pub struct HydrographOrdinate {
    pub time_minutes: f64,
    pub flow_cfs: f64,
    pub t_ratio: f64,
    pub q_ratio: f64,
}

#[derive(Debug, Clone)]
pub struct UnitHydrographResult {
    pub area_acres: f64,
    pub tc_minutes: f64,
    pub duration_minutes: f64,
    pub lag_hours: f64,
    pub time_to_peak_hours: f64,
    pub time_to_peak_minutes: f64,
    pub peak_flow_cfs: f64,
    pub time_step_minutes: f64,
    pub ordinates: Vec<HydrographOrdinate>,
}

pub fn acres_to_sq_mi(area_acres: f64) -> f64 {
    area_acres / 640.0
}

pub fn lag_hours(tc_minutes: f64) -> f64 {
    assert!(tc_minutes > 0.0);
    LAG_FACTOR * tc_minutes / 60.0
}

pub fn duration_hours(tc_minutes: f64, duration_minutes: Option<f64>) -> f64 {
    assert!(tc_minutes > 0.0);
    match duration_minutes {
        Some(d) => {
            assert!(d > 0.0);
            d / 60.0
        }
        None => DEFAULT_DURATION_FACTOR * tc_minutes / 60.0,
    }
}

pub fn time_to_peak_hours(tc_minutes: f64, duration_minutes: Option<f64>) -> f64 {
    let d_hr = duration_hours(tc_minutes, duration_minutes);
    d_hr / 2.0 + lag_hours(tc_minutes)
}

pub fn peak_discharge_cfs(area_acres: f64, time_to_peak_hours: f64) -> f64 {
    assert!(area_acres > 0.0 && time_to_peak_hours > 0.0);
    PEAK_FACTOR * acres_to_sq_mi(area_acres) / time_to_peak_hours
}

pub fn dimensionless_flow(t_ratio: f64) -> f64 {
    if t_ratio < 0.0 {
        return 0.0;
    }
    if t_ratio >= DIMENSIONLESS_CURVE.last().unwrap().0 {
        return 0.0;
    }
    for w in DIMENSIONLESS_CURVE.windows(2) {
        let (t0, q0) = w[0];
        let (t1, q1) = w[1];
        if t_ratio <= t1 {
            if t1 <= t0 {
                return q1;
            }
            let f = (t_ratio - t0) / (t1 - t0);
            return q0 + f * (q1 - q0);
        }
    }
    0.0
}

pub fn generate(
    area_acres: f64,
    tc_minutes: f64,
    duration_minutes: Option<f64>,
    time_step_minutes: Option<f64>,
) -> UnitHydrographResult {
    assert!(area_acres > 0.0 && tc_minutes > 0.0);
    let d_min = duration_minutes.unwrap_or(DEFAULT_DURATION_FACTOR * tc_minutes);
    assert!(d_min > 0.0);
    let tp_hr = time_to_peak_hours(tc_minutes, Some(d_min));
    let tp_min = tp_hr * 60.0;
    let tl_hr = lag_hours(tc_minutes);
    let qp = peak_discharge_cfs(area_acres, tp_hr);
    let dt_min = time_step_minutes.unwrap_or(tp_min * DEFAULT_TIME_STEP_FACTOR);
    let total_min = tp_min * TOTAL_DURATION_FACTOR;

    let mut ordinates = Vec::new();
    let mut t_min = 0.0;
    while t_min <= total_min + 1e-9 {
        let t_ratio = if tp_min > 0.0 { t_min / tp_min } else { 0.0 };
        let q_ratio = dimensionless_flow(t_ratio);
        ordinates.push(HydrographOrdinate {
            time_minutes: t_min,
            flow_cfs: q_ratio * qp,
            t_ratio,
            q_ratio,
        });
        t_min += dt_min;
    }

    UnitHydrographResult {
        area_acres,
        tc_minutes,
        duration_minutes: d_min,
        lag_hours: tl_hr,
        time_to_peak_hours: tp_hr,
        time_to_peak_minutes: tp_min,
        peak_flow_cfs: qp,
        time_step_minutes: dt_min,
        ordinates,
    }
}