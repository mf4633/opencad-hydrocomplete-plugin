// SPDX-License-Identifier: GPL-3.0-only

//! Multi-return-period IDF curves for storm-sewer design.

use std::collections::BTreeMap;

use crate::idf::IdfCurve;

/// Rainfall IDF curves keyed by return period (years).
#[derive(Clone, Debug, PartialEq)]
pub struct IdfSet {
    /// Active design storm return period (years).
    pub design_rp: u32,
    curves: BTreeMap<u32, IdfCurve>,
}

impl Default for IdfSet {
    fn default() -> Self {
        let mut curves = BTreeMap::new();
        curves.insert(10, IdfCurve::new(60.0, 10.0, 0.8));
        Self { design_rp: 10, curves }
    }
}

impl IdfSet {
    pub fn new(design_rp: u32) -> Self {
        Self { design_rp, curves: BTreeMap::new() }
    }

    /// Municipal default: 10-year curve `i = 60/(t+10)^0.8`.
    pub fn municipal_default() -> Self {
        Self::default()
    }

    pub fn set_curve(&mut self, rp: u32, curve: IdfCurve) {
        self.curves.insert(rp, curve);
    }

    pub fn curve(&self, rp: u32) -> Option<&IdfCurve> {
        self.curves.get(&rp)
    }

    pub fn design_curve(&self) -> &IdfCurve {
        self.curves
            .get(&self.design_rp)
            .or_else(|| self.curves.values().next())
            .expect("IdfSet must contain at least one curve")
    }

    pub fn set_design_rp(&mut self, rp: u32) {
        self.design_rp = rp;
    }

    /// Intensity (in/hr) for duration `t_min` at the design return period.
    pub fn design_intensity(&self, t_min: f64) -> f64 {
        self.design_curve().intensity(t_min)
    }

    /// All configured return periods, ascending.
    pub fn return_periods(&self) -> Vec<u32> {
        self.curves.keys().copied().collect()
    }

    /// Analyze intensity at every configured return period for one duration.
    pub fn intensities_at(&self, t_min: f64) -> Vec<(u32, f64)> {
        self.return_periods()
            .into_iter()
            .map(|rp| (rp, self.curves[&rp].intensity(t_min)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_curve_defaults_to_10yr() {
        let set = IdfSet::default();
        assert_eq!(set.design_rp, 10);
        assert!(set.design_intensity(15.0) > 0.0);
    }

    #[test]
    fn multiple_return_periods() {
        let mut set = IdfSet::default();
        set.set_curve(25, IdfCurve::new(80.0, 15.0, 0.8));
        set.set_design_rp(25);
        let i10 = set.curve(10).unwrap().intensity(20.0);
        let i25 = set.design_intensity(20.0);
        assert!(i25 > i10);
    }
}