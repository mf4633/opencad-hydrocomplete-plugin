// SPDX-License-Identifier: GPL-3.0-only

//! Time-of-concentration estimators (minutes).

/// Kirpich (1940) — overland flow over unpaved channel.
/// `l` = flow path length (ft), `s` = average slope (ft/ft, positive).
pub fn kirpich_minutes(l: f64, s: f64) -> f64 {
    if l <= 0.0 || s <= 0.0 {
        return 0.0;
    }
    0.0078 * l.powf(0.77) * s.powf(-0.385)
}

/// FAA / TR-55 style sheet flow on paved surfaces.
/// `l` = flow path (ft), `s` = slope (ft/ft).
pub fn faa_sheet_flow_minutes(l: f64, s: f64) -> f64 {
    if l <= 0.0 || s <= 0.0 {
        return 0.0;
    }
    // n=0.02, k=0.007 (US customary) → t = 0.007 * (n*L)^0.8 / (S^0.5 * k^0.2) with n fixed
    let n = 0.02_f64;
    let k = 0.007_f64;
    0.007_f64 * (n * l).powf(0.8) / (s.sqrt() * k.powf(0.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kirpich_increases_with_length() {
        let short = kirpich_minutes(200.0, 0.02);
        let long = kirpich_minutes(800.0, 0.02);
        assert!(long > short);
    }

    #[test]
    fn faa_reasonable_range() {
        let t = faa_sheet_flow_minutes(300.0, 0.01);
        assert!(t > 0.5 && t < 30.0, "t={t}");
    }
}