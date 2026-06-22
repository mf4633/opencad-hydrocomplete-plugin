//! Culvert headwater analysis (FHWA HDS-5 simplified).

pub const G: f64 = 32.2;
pub const KN: f64 = 1.486;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlType {
    None,
    Inlet,
    Outlet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DischargeMode {
    Orifice,
    Manning,
}

#[derive(Debug, Clone)]
pub struct CulvertParameters {
    pub diameter_in: f64,
    pub length_ft: f64,
    pub slope_ft_per_ft: f64,
    pub manning_n: f64,
    pub entrance_loss_ke: f64,
}

impl Default for CulvertParameters {
    fn default() -> Self {
        Self {
            diameter_in: 24.0,
            length_ft: 100.0,
            slope_ft_per_ft: 0.01,
            manning_n: 0.013,
            entrance_loss_ke: 0.5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HeadwaterResult {
    pub discharge_cfs: f64,
    pub headwater_ft: f64,
    pub headwater_inlet_ft: f64,
    pub headwater_outlet_ft: f64,
    pub control: ControlType,
    pub velocity_fps: f64,
    pub diameter_ft: f64,
}

pub fn orifice_flow_cfs(head_ft: f64, diameter_ft: f64, discharge_coeff: f64) -> f64 {
    assert!(head_ft >= 0.0 && diameter_ft > 0.0);
    let area = std::f64::consts::PI * diameter_ft * diameter_ft / 4.0;
    discharge_coeff * area * (2.0 * G * head_ft).sqrt()
}

pub fn manning_full_flow_cfs(diameter_ft: f64, slope_ft_per_ft: f64, manning_n: f64) -> f64 {
    assert!(diameter_ft > 0.0 && slope_ft_per_ft >= 0.0 && manning_n > 0.0);
    let area = std::f64::consts::PI * diameter_ft * diameter_ft / 4.0;
    let radius = diameter_ft / 4.0;
    (KN / manning_n) * area * radius.powf(2.0 / 3.0) * slope_ft_per_ft.sqrt()
}

pub fn headwater(
    discharge_cfs: f64,
    culvert: &CulvertParameters,
    tailwater_ft: f64,
) -> HeadwaterResult {
    assert!(discharge_cfs >= 0.0);
    let d_ft = culvert.diameter_in / 12.0;
    let l = culvert.length_ft;
    let s = culvert.slope_ft_per_ft;
    let n = culvert.manning_n;
    let ke = culvert.entrance_loss_ke;
    let area = std::f64::consts::PI * d_ft * d_ft / 4.0;

    let mut hw_inlet = 0.0;
    let mut hw_outlet = 0.0;
    let mut velocity = 0.0;

    if discharge_cfs > 0.0 && area > 0.0 {
        velocity = discharge_cfs / area;
        let q_over_ad05 = discharge_cfs / (area * d_ft.powf(0.5));
        const KU: f64 = 0.0098;
        const MU: f64 = 2.0;
        const KSU: f64 = -0.5;
        const CS: f64 = 0.0398;
        const YS: f64 = 0.67;
        let hw_unsub = d_ft * (1.0 + KU * q_over_ad05.powf(MU) + KSU * s);
        let hw_sub = d_ft * (CS * q_over_ad05.powf(2.0) + YS - 0.5 * s);
        hw_inlet = hw_unsub.max(hw_sub);

        let r = d_ft / 4.0;
        let friction_coeff = 19.63 * n * n * l / r.powf(4.0 / 3.0);
        let h_loss = (ke + 1.0 + friction_coeff) * velocity * velocity / (2.0 * G);
        let dc = 0.467 * (discharge_cfs * discharge_cfs / (G * d_ft.powi(5))).powf(0.1) * d_ft;
        let ho = tailwater_ft.max((dc.min(d_ft) + d_ft) / 2.0);
        hw_outlet = (h_loss + ho - l * s).max(0.0);
    }

    let control = if hw_inlet >= hw_outlet {
        ControlType::Inlet
    } else {
        ControlType::Outlet
    };
    let hw = hw_inlet.max(hw_outlet);

    HeadwaterResult {
        discharge_cfs,
        headwater_ft: hw,
        headwater_inlet_ft: hw_inlet,
        headwater_outlet_ft: hw_outlet,
        control,
        velocity_fps: velocity,
        diameter_ft: d_ft,
    }
}

pub fn discharge_from_headwater_ft(
    headwater_ft: f64,
    culvert: &CulvertParameters,
    mode: DischargeMode,
) -> f64 {
    let d_ft = culvert.diameter_in / 12.0;
    if headwater_ft <= 0.0 {
        return 0.0;
    }
    let q_orifice = orifice_flow_cfs(headwater_ft, d_ft, 0.6);
    let q_manning = manning_full_flow_cfs(d_ft, culvert.slope_ft_per_ft, culvert.manning_n);
    match mode {
        DischargeMode::Manning => q_manning,
        DischargeMode::Orifice => q_orifice.min(q_manning),
    }
}

#[derive(Debug, Clone)]
pub struct RatingPoint {
    pub discharge_cfs: f64,
    pub headwater_ft: f64,
    pub control: ControlType,
}

pub fn rating_curve(
    culvert: &CulvertParameters,
    tailwater_ft: f64,
    max_discharge_cfs: f64,
    point_count: usize,
) -> Vec<RatingPoint> {
    assert!(max_discharge_cfs >= 0.0 && point_count >= 2);
    let mut curve = vec![RatingPoint {
        discharge_cfs: 0.0,
        headwater_ft: 0.0,
        control: ControlType::None,
    }];
    let step = max_discharge_cfs / (point_count - 1) as f64;
    for i in 1..point_count {
        let q = i as f64 * step;
        let hw = headwater(q, culvert, tailwater_ft);
        curve.push(RatingPoint {
            discharge_cfs: q,
            headwater_ft: hw.headwater_ft,
            control: hw.control,
        });
    }
    curve
}