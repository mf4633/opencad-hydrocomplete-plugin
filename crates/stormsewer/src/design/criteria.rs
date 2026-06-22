// SPDX-License-Identifier: GPL-3.0-only

//! Design criteria for storm-sewer pipe sizing (velocity, capacity, catalogs).

/// Standard reinforced-concrete pipe diameters (inches) used in US storm design.
pub const STANDARD_RCP_INCHES: &[u32] =
    &[8, 10, 12, 15, 18, 21, 24, 27, 30, 33, 36, 42, 48, 54, 60, 66, 72];

/// Convert a catalog diameter from inches to feet.
pub fn inches_to_ft(d_in: u32) -> f64 {
    d_in as f64 / 12.0
}

/// Default ascending catalog in feet.
pub fn standard_diameters_ft() -> Vec<f64> {
    STANDARD_RCP_INCHES.iter().map(|&d| inches_to_ft(d)).collect()
}

/// Agency-style limits used by [`super::sizing::size_pipe_for_flow`].
#[derive(Clone, Debug, PartialEq)]
pub struct DesignCriteria {
    /// Minimum design velocity (ft/s). Pipes below this are rejected.
    pub min_velocity: f64,
    /// Maximum design velocity (ft/s). Pipes above this are rejected.
    pub max_velocity: f64,
    /// Maximum design flow as a fraction of just-full Manning capacity.
    pub max_pct_full: f64,
    /// Ascending catalog of trial diameters (ft).
    pub standard_diameters_ft: Vec<f64>,
    /// When true, reject diameters where design Q exceeds open-channel capacity.
    pub require_open_channel: bool,
}

impl Default for DesignCriteria {
    fn default() -> Self {
        Self {
            min_velocity: 2.0,
            max_velocity: 10.0,
            max_pct_full: 0.85,
            standard_diameters_ft: standard_diameters_ft(),
            require_open_channel: true,
        }
    }
}

impl DesignCriteria {
    /// Typical municipal / DOT storm trunk defaults.
    pub fn municipal() -> Self {
        Self::default()
    }

    /// Slightly relaxed criteria for laterals (allows higher % full).
    pub fn lateral() -> Self {
        Self { max_pct_full: 0.95, ..Self::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_ascending_in_feet() {
        let d = standard_diameters_ft();
        assert!(d.windows(2).all(|w| w[0] < w[1]));
        assert!((d[0] - inches_to_ft(8)).abs() < 1e-9);
    }
}