//! Pump station duty-point check.

#[derive(Debug, Clone)]
pub struct CurvePoint {
    pub flow_cfs: f64,
    pub head_ft: f64,
}

#[derive(Debug, Clone)]
pub struct DutyResult {
    pub design_flow_cfs: f64,
    pub static_head_ft: f64,
    pub friction_head_ft: f64,
    pub system_head_ft: f64,
    pub pump_head_ft: f64,
    pub head_margin_ft: f64,
    pub ok: bool,
}

pub fn interpolate_pump_head(curve: &[CurvePoint], flow_cfs: f64) -> f64 {
    if curve.is_empty() {
        return 0.0;
    }
    if curve.len() == 1 {
        return curve[0].head_ft;
    }
    let mut sorted = curve.to_vec();
    sorted.sort_by(|a, b| a.flow_cfs.partial_cmp(&b.flow_cfs).unwrap());
    if flow_cfs <= sorted[0].flow_cfs {
        return sorted[0].head_ft;
    }
    if flow_cfs >= sorted.last().unwrap().flow_cfs {
        return sorted.last().unwrap().head_ft;
    }
    for w in sorted.windows(2) {
        let lo = &w[0];
        let hi = &w[1];
        if flow_cfs >= lo.flow_cfs && flow_cfs <= hi.flow_cfs {
            let span = hi.flow_cfs - lo.flow_cfs;
            if span <= 0.0 {
                return lo.head_ft;
            }
            let t = (flow_cfs - lo.flow_cfs) / span;
            return lo.head_ft + t * (hi.head_ft - lo.head_ft);
        }
    }
    sorted.last().unwrap().head_ft
}

pub fn check_duty(
    design_flow_cfs: f64,
    suction_invert_ft: f64,
    discharge_invert_ft: f64,
    force_main_length_ft: f64,
    force_main_diameter_ft: f64,
    manning_n: f64,
    pump_curve: &[CurvePoint],
) -> DutyResult {
    let static_head = (discharge_invert_ft - suction_invert_ft).max(0.0);
    let friction = if force_main_length_ft > 0.0
        && force_main_diameter_ft > 0.0
        && design_flow_cfs > 0.0
    {
        let area = std::f64::consts::PI * force_main_diameter_ft * force_main_diameter_ft / 4.0;
        let velocity = design_flow_cfs / area;
        let hydraulic_radius = force_main_diameter_ft / 4.0;
        manning_n * manning_n * force_main_length_ft * velocity * velocity
            / (2.22 * hydraulic_radius.powf(4.0 / 3.0))
    } else {
        0.0
    };
    let system_head = static_head + friction;
    let pump_head = interpolate_pump_head(pump_curve, design_flow_cfs);
    let margin = pump_head - system_head;
    DutyResult {
        design_flow_cfs,
        static_head_ft: static_head,
        friction_head_ft: friction,
        system_head_ft: system_head,
        pump_head_ft: pump_head,
        head_margin_ft: margin,
        ok: pump_head >= system_head && design_flow_cfs > 0.0,
    }
}

pub fn default_curve() -> Vec<CurvePoint> {
    vec![
        CurvePoint {
            flow_cfs: 0.0,
            head_ft: 60.0,
        },
        CurvePoint {
            flow_cfs: 30.0,
            head_ft: 55.0,
        },
        CurvePoint {
            flow_cfs: 50.0,
            head_ft: 40.0,
        },
    ]
}