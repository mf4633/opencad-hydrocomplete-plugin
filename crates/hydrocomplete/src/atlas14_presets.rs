//! NOAA Atlas 14 embedded IDF presets — mirrors `Atlas14Presets.cs`.

use std::collections::HashMap;

use stormsewer::idf::IdfCurve;

#[derive(Debug, Clone)]
pub struct Atlas14Preset {
    pub key: &'static str,
    pub display_name: &'static str,
    pub state: &'static str,
    pub lat: f64,
    pub lon: f64,
    curves: HashMap<i32, (f64, f64, f64)>,
    i10: HashMap<i32, f64>,
}

impl Atlas14Preset {
    fn new(
        key: &'static str,
        display_name: &'static str,
        state: &'static str,
        lat: f64,
        lon: f64,
        a2: f64,
        b2: f64,
        c2: f64,
        a10: f64,
        b10: f64,
        c10: f64,
        a25: f64,
        b25: f64,
        c25: f64,
        a100: f64,
        b100: f64,
        c100: f64,
        i2: f64,
        i10v: f64,
        i25: f64,
        i100: f64,
    ) -> Self {
        let mut curves = HashMap::new();
        curves.insert(2, (a2, b2, c2));
        curves.insert(10, (a10, b10, c10));
        curves.insert(25, (a25, b25, c25));
        curves.insert(100, (a100, b100, c100));
        let mut i10 = HashMap::new();
        i10.insert(2, i2);
        i10.insert(10, i10v);
        i10.insert(25, i25);
        i10.insert(100, i100);
        Self {
            key,
            display_name,
            state,
            lat,
            lon,
            curves,
            i10,
        }
    }

    pub fn a(&self) -> f64 {
        self.curves.get(&10).map(|c| c.0).unwrap_or(0.0)
    }
    pub fn b(&self) -> f64 {
        self.curves.get(&10).map(|c| c.1).unwrap_or(0.0)
    }
    pub fn c(&self) -> f64 {
        self.curves.get(&10).map(|c| c.2).unwrap_or(0.0)
    }

    pub fn to_curve(&self, return_period_years: i32) -> Result<IdfCurve, String> {
        let (a, b, c) = self
            .curves
            .get(&return_period_years)
            .copied()
            .ok_or_else(|| format!("Return period {return_period_years} not in embedded presets (2,10,25,100)."))?;
        Ok(IdfCurve::new(a, b, c))
    }

    pub fn multi_return_period_10min_label(&self) -> String {
        let mut parts = Vec::new();
        for rp in [2, 10, 25, 100] {
            if let Some(i) = self.i10.get(&rp) {
                parts.push(format!("{rp}y={i:.2}"));
            }
        }
        parts.join(" ")
    }

    pub fn intensity_10min(&self, return_period_years: i32) -> Option<f64> {
        self.i10.get(&return_period_years).copied()
    }
}

fn all_presets() -> &'static [Atlas14Preset] {
    static PRESETS: std::sync::OnceLock<Vec<Atlas14Preset>> = std::sync::OnceLock::new();
    PRESETS.get_or_init(|| {
        vec![
            p("charlotte-nc", "Charlotte, NC", "NC", 35.23, -80.84, 75.09, 13.25, 0.891, 81.21, 13.50, 0.832, 81.57, 13.50, 0.802, 69.60, 12.00, 0.729, 4.54, 5.81, 6.40, 7.19),
            p("raleigh-nc", "Raleigh, NC", "NC", 35.78, -78.64, 70.57, 12.75, 0.877, 72.49, 12.50, 0.807, 71.11, 12.25, 0.772, 59.68, 10.50, 0.693, 4.55, 5.82, 6.41, 7.25),
            p("asheville-nc", "Asheville, NC", "NC", 35.60, -82.55, 73.41, 14.50, 0.925, 99.76, 16.25, 0.904, 105.49, 16.50, 0.875, 104.65, 16.00, 0.823, 3.79, 5.14, 5.89, 7.03),
            p("atlanta-ga", "Atlanta, GA", "GA", 33.75, -84.39, 28.19, 5.50, 0.703, 40.11, 5.50, 0.706, 45.92, 5.25, 0.695, 54.21, 4.75, 0.680, 4.02, 5.69, 6.78, 8.55),
            p("washington-dc", "Washington, DC", "DC", 38.91, -77.04, 69.47, 13.50, 0.897, 82.66, 14.25, 0.852, 78.40, 13.50, 0.804, 67.53, 11.75, 0.724, 4.07, 5.41, 6.11, 7.15),
            p("philadelphia-pa", "Philadelphia, PA", "PA", 39.95, -75.17, 52.50, 11.50, 0.841, 60.49, 12.00, 0.793, 58.81, 11.50, 0.754, 52.52, 10.25, 0.687, 3.97, 5.17, 5.76, 6.56),
            p("new-york-ny", "New York, NY", "NY", 40.71, -74.01, 24.05, 4.00, 0.700, 36.45, 4.00, 0.708, 44.03, 4.00, 0.710, 56.08, 4.00, 0.714, 3.77, 5.59, 6.73, 8.47),
            p("boston-ma", "Boston, MA", "MA", 42.36, -71.06, 20.36, 4.00, 0.708, 31.35, 4.00, 0.701, 38.36, 4.00, 0.700, 48.72, 4.00, 0.697, 3.14, 4.92, 6.03, 7.74),
            p("chicago-il", "Chicago, IL", "IL", 41.88, -87.63, 52.44, 9.25, 0.848, 55.29, 8.25, 0.779, 56.21, 7.75, 0.746, 55.70, 6.75, 0.696, 4.29, 5.73, 6.51, 7.70),
            p("detroit-mi", "Detroit, MI", "MI", 42.33, -83.05, 22.24, 5.25, 0.715, 32.83, 5.25, 0.714, 39.59, 5.25, 0.713, 48.87, 5.00, 0.704, 3.13, 4.63, 5.60, 7.16),
            p("minneapolis-mn", "Minneapolis, MN", "MN", 44.98, -93.27, 24.31, 5.00, 0.691, 31.88, 4.25, 0.656, 36.43, 4.00, 0.636, 44.37, 4.00, 0.613, 3.68, 5.49, 6.70, 8.69),
            p("denver-co", "Denver, CO", "CO", 39.74, -104.99, 19.87, 6.75, 0.764, 35.05, 7.00, 0.786, 43.66, 6.75, 0.782, 59.99, 6.75, 0.783, 2.26, 3.71, 4.73, 6.48),
            p("dallas-tx", "Dallas, TX", "TX", 32.78, -96.80, 45.96, 9.50, 0.765, 57.31, 8.50, 0.737, 63.29, 8.00, 0.722, 69.90, 7.25, 0.697, 4.75, 6.72, 7.93, 9.73),
            p("houston-tx", "Houston, TX", "TX", 29.76, -95.37, 48.02, 9.25, 0.726, 51.13, 6.75, 0.657, 51.17, 5.25, 0.616, 54.41, 4.00, 0.574, 5.57, 8.07, 9.67, 12.20),
            p("miami-fl", "Miami, FL", "FL", 25.76, -80.19, 31.41, 4.50, 0.628, 39.39, 4.00, 0.599, 44.51, 4.00, 0.584, 51.63, 4.00, 0.561, 5.73, 7.93, 9.33, 11.50),
            p("phoenix-az", "Phoenix, AZ", "AZ", 33.45, -112.07, 30.81, 10.00, 0.875, 62.26, 11.50, 0.918, 76.75, 11.50, 0.920, 95.72, 11.25, 0.913, 2.21, 3.67, 4.49, 5.79),
            p("los-angeles-ca", "Los Angeles, CA", "CA", 34.05, -118.24, 8.37, 4.00, 0.611, 12.99, 4.00, 0.609, 16.13, 4.00, 0.611, 21.24, 4.00, 0.612, 1.67, 2.60, 3.22, 4.22),
            p("boise-id", "Boise, ID", "ID", 43.61, -116.20, 10.88, 4.00, 0.802, 23.29, 4.00, 0.829, 30.49, 4.00, 0.835, 40.59, 4.00, 0.837, 1.32, 2.62, 3.37, 4.47),
            p("coeur-dalene-id", "Coeur d'Alene, ID", "ID", 47.68, -116.78, 11.21, 4.00, 0.790, 21.65, 4.00, 0.818, 27.49, 4.00, 0.825, 35.19, 4.00, 0.829, 1.41, 2.51, 3.13, 3.97),
            p("idaho-falls-id", "Idaho Falls, ID", "ID", 43.49, -112.03, 13.60, 4.00, 0.823, 26.93, 4.25, 0.850, 35.32, 4.50, 0.862, 47.20, 4.75, 0.872, 1.56, 2.81, 3.52, 4.51),
            p("billings-mt", "Billings, MT", "MT", 45.78, -108.50, 20.43, 4.00, 0.823, 38.62, 4.00, 0.837, 52.18, 4.25, 0.848, 71.61, 4.25, 0.850, 2.34, 4.25, 5.49, 7.48),
            p("helena-mt", "Helena, MT", "MT", 46.59, -112.04, 15.32, 4.00, 0.811, 29.55, 4.00, 0.830, 38.27, 4.00, 0.835, 53.15, 4.25, 0.846, 1.81, 3.31, 4.23, 5.62),
            p("missoula-mt", "Missoula, MT", "MT", 46.87, -113.99, 11.58, 4.00, 0.799, 23.39, 4.00, 0.827, 30.14, 4.00, 0.833, 39.27, 4.00, 0.836, 1.42, 2.65, 3.35, 4.34),
            p("great-falls-mt", "Great Falls, MT", "MT", 47.50, -111.30, 16.68, 4.00, 0.809, 32.45, 4.00, 0.833, 42.02, 4.00, 0.837, 59.08, 4.25, 0.848, 1.99, 3.61, 4.63, 6.21),
            p("bozeman-mt", "Bozeman, MT", "MT", 45.68, -111.04, 13.38, 4.00, 0.802, 28.15, 4.00, 0.831, 36.86, 4.00, 0.836, 51.28, 4.25, 0.847, 1.62, 3.15, 4.06, 5.41),
        ]
    })
}

#[allow(clippy::too_many_arguments)]
fn p(
    key: &'static str,
    name: &'static str,
    state: &'static str,
    lat: f64,
    lon: f64,
    a2: f64,
    b2: f64,
    c2: f64,
    a10: f64,
    b10: f64,
    c10: f64,
    a25: f64,
    b25: f64,
    c25: f64,
    a100: f64,
    b100: f64,
    c100: f64,
    i2: f64,
    i10: f64,
    i25: f64,
    i100: f64,
) -> Atlas14Preset {
    Atlas14Preset::new(
        key, name, state, lat, lon, a2, b2, c2, a10, b10, c10, a25, b25, c25, a100, b100, c100, i2, i10, i25,
        i100,
    )
}

pub fn list() -> &'static [Atlas14Preset] {
    all_presets()
}

pub fn find(key: &str) -> Option<&'static Atlas14Preset> {
    let k = key.trim().to_lowercase();
    all_presets().iter().find(|p| p.key.eq_ignore_ascii_case(&k))
}

pub fn nearest(lat: f64, lon: f64) -> &'static Atlas14Preset {
    let mut best = &all_presets()[0];
    let mut best_dist = f64::MAX;
    for p in all_presets() {
        let dlat = p.lat - lat;
        let dlon = p.lon - lon;
        let dist = dlat * dlat + dlon * dlon;
        if dist < best_dist {
            best_dist = dist;
            best = p;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charlotte_multi_rp_label() {
        let p = find("charlotte-nc").unwrap();
        assert_eq!(p.multi_return_period_10min_label(), "2y=4.54 10y=5.81 25y=6.40 100y=7.19");
    }

    #[test]
    fn charlotte_curve_10yr() {
        let p = find("charlotte-nc").unwrap();
        let c = p.to_curve(10).unwrap();
        assert!((c.a - 81.21).abs() < 0.1);
    }
}