// SPDX-License-Identifier: GPL-3.0-only

//! Inlet interception capacity (HEC-22 Chapter 4, US customary).

/// Inlet configuration per FHWA HEC-22 (simplified US customary forms).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InletKind {
    /// Grate inlet on grade (composite gutter flow).
    #[default]
    GrateOnGrade,
    /// Curb-opening inlet on grade.
    CurbOpening,
    /// Grate + curb opening at the same structure (capacities summed).
    Combination,
    /// Sag (low-point) grate — weir-controlled capture.
    SagGrate,
}

impl InletKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::GrateOnGrade => "grate on grade",
            Self::CurbOpening => "curb opening",
            Self::Combination => "combination (grate + curb)",
            Self::SagGrate => "sag grate",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "grate" | "grateongrade" | "grate_on_grade" | "g" => Some(Self::GrateOnGrade),
            "curb" | "curbopening" | "curb_opening" | "c" => Some(Self::CurbOpening),
            "combo" | "combination" | "comb" => Some(Self::Combination),
            "sag" | "saggrate" | "sag_grate" | "s" => Some(Self::SagGrate),
            _ => None,
        }
    }
}

/// Gutter / inlet geometry for capacity checks (ft, ft/ft).
#[derive(Clone, Debug, PartialEq)]
pub struct InletGeometry {
    pub kind: InletKind,
    pub grate_length_ft: f64,
    pub curb_opening_length_ft: f64,
    pub flow_depth_ft: f64,
    pub gutter_slope: f64,
}

impl Default for InletGeometry {
    fn default() -> Self {
        Self {
            kind: InletKind::GrateOnGrade,
            grate_length_ft: 2.0,
            curb_opening_length_ft: 4.0,
            flow_depth_ft: 0.15,
            gutter_slope: 0.005,
        }
    }
}

/// Grate-on-grade capacity (cfs). HEC-22 composite gutter approximation.
///
/// `Q = C_w L d^{1.5} S^{0.5}` with `C_w ≈ 3.0` (US customary).
pub fn grate_capacity_cfs(grate_length_ft: f64, flow_depth_ft: f64, gutter_slope: f64) -> f64 {
    if grate_length_ft <= 0.0 || flow_depth_ft <= 0.0 || gutter_slope <= 0.0 {
        return 0.0;
    }
    const CW: f64 = 3.0;
    CW * grate_length_ft * flow_depth_ft.powf(1.5) * gutter_slope.sqrt()
}

/// Curb-opening on grade (cfs). HEC-22 Eq. 4-4 style.
///
/// `Q = 4.13 L d^{2.67} S^{0.5}`.
pub fn curb_opening_capacity_cfs(length_ft: f64, flow_depth_ft: f64, gutter_slope: f64) -> f64 {
    if length_ft <= 0.0 || flow_depth_ft <= 0.0 || gutter_slope <= 0.0 {
        return 0.0;
    }
    4.13 * length_ft * flow_depth_ft.powf(2.67) * gutter_slope.sqrt()
}

/// Sag (low-point) grate weir capacity (cfs).
///
/// `Q = 3.3 L d^{1.5}` (US customary weir form).
pub fn sag_grate_capacity_cfs(grate_length_ft: f64, flow_depth_ft: f64) -> f64 {
    if grate_length_ft <= 0.0 || flow_depth_ft <= 0.0 {
        return 0.0;
    }
    3.3 * grate_length_ft * flow_depth_ft.powf(1.5)
}

/// Total inlet capacity for the configured [`InletKind`].
pub fn inlet_capacity_cfs(geom: &InletGeometry) -> f64 {
    match geom.kind {
        InletKind::GrateOnGrade => {
            grate_capacity_cfs(geom.grate_length_ft, geom.flow_depth_ft, geom.gutter_slope)
        }
        InletKind::CurbOpening => curb_opening_capacity_cfs(
            geom.curb_opening_length_ft,
            geom.flow_depth_ft,
            geom.gutter_slope,
        ),
        InletKind::Combination => {
            grate_capacity_cfs(geom.grate_length_ft, geom.flow_depth_ft, geom.gutter_slope)
                + curb_opening_capacity_cfs(
                    geom.curb_opening_length_ft,
                    geom.flow_depth_ft,
                    geom.gutter_slope,
                )
        }
        InletKind::SagGrate => sag_grate_capacity_cfs(geom.grate_length_ft, geom.flow_depth_ft),
    }
}

/// Check whether an inlet can capture the approach design flow.
#[derive(Clone, Debug, PartialEq)]
pub struct InletCheck {
    pub kind: InletKind,
    pub design_q_cfs: f64,
    pub capacity_cfs: f64,
    pub ok: bool,
}

pub fn check_inlet_geom(design_q_cfs: f64, geom: &InletGeometry) -> InletCheck {
    let cap = inlet_capacity_cfs(geom);
    InletCheck {
        kind: geom.kind,
        design_q_cfs,
        capacity_cfs: cap,
        ok: cap >= design_q_cfs,
    }
}

/// Legacy grate-only check (backward compatible).
pub fn check_inlet(
    design_q_cfs: f64,
    grate_length_ft: f64,
    flow_depth_ft: f64,
    gutter_slope: f64,
) -> InletCheck {
    let geom = InletGeometry {
        kind: InletKind::GrateOnGrade,
        grate_length_ft,
        curb_opening_length_ft: 4.0,
        flow_depth_ft,
        gutter_slope,
    };
    check_inlet_geom(design_q_cfs, &geom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_grate_carries_more() {
        let a = grate_capacity_cfs(2.0, 0.15, 0.005);
        let b = grate_capacity_cfs(5.0, 0.15, 0.005);
        assert!(b > a);
    }

    #[test]
    fn curb_opening_exceeds_grate_at_same_length() {
        let g = grate_capacity_cfs(4.0, 0.15, 0.005);
        let c = curb_opening_capacity_cfs(4.0, 0.15, 0.005);
        assert!(c > g);
    }

    #[test]
    fn combination_sums_components() {
        let geom = InletGeometry {
            kind: InletKind::Combination,
            grate_length_ft: 2.0,
            curb_opening_length_ft: 4.0,
            flow_depth_ft: 0.15,
            gutter_slope: 0.005,
        };
        let total = inlet_capacity_cfs(&geom);
        let g = grate_capacity_cfs(2.0, 0.15, 0.005);
        let c = curb_opening_capacity_cfs(4.0, 0.15, 0.005);
        assert!((total - (g + c)).abs() < 1e-9);
    }

    #[test]
    fn sag_grate_no_slope_dependency() {
        let a = sag_grate_capacity_cfs(3.0, 0.2);
        let b = sag_grate_capacity_cfs(3.0, 0.2);
        assert!((a - b).abs() < 1e-12);
        assert!(a > 0.0);
    }

    #[test]
    fn kind_from_str() {
        assert_eq!(InletKind::from_str_loose("combo"), Some(InletKind::Combination));
        assert_eq!(InletKind::from_str_loose("SAG"), Some(InletKind::SagGrate));
    }
}