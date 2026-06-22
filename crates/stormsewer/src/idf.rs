// SPDX-License-Identifier: GPL-3.0-only

//! Rainfall intensity–duration–frequency (IDF) curves.
//!
//! Uses the common three-parameter form
//!
//! ```text
//! i = a / (t + b)^c          (i in in/hr, t in minutes)
//! ```
//!
//! which fits NOAA Atlas-14 partial-duration data well over the 5–180 min
//! range used in storm-sewer design.

/// A three-parameter IDF curve for a single return period.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdfCurve {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

impl IdfCurve {
    pub fn new(a: f64, b: f64, c: f64) -> Self {
        Self { a, b, c }
    }

    /// Rainfall intensity (in/hr) for a storm duration `t_min` (minutes).
    pub fn intensity(&self, t_min: f64) -> f64 {
        let t = t_min.max(0.0);
        self.a / (t + self.b).powf(self.c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intensity_decreases_with_duration() {
        let idf = IdfCurve::new(120.0, 10.0, 0.8);
        let short = idf.intensity(5.0);
        let long = idf.intensity(60.0);
        assert!(short > long, "short {short} long {long}");
        assert!(long > 0.0);
    }

    #[test]
    fn intensity_known_value() {
        // a=120,b=10,c=0.8 ; t=15 -> 120 / 25^0.8 = 9.14 in/hr (hand calc).
        let idf = IdfCurve::new(120.0, 10.0, 0.8);
        assert!((idf.intensity(15.0) - 9.14).abs() < 0.05, "{}", idf.intensity(15.0));
    }
}
