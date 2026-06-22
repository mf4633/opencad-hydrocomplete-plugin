//! Clark (1945) instantaneous unit hydrograph.

pub const DEFAULT_STORAGE_FACTOR: f64 = 0.4;

#[derive(Debug, Clone)]
pub struct HydrographOrdinate {
    pub time_minutes: f64,
    pub translated_flow_cfs: f64,
    pub flow_cfs: f64,
}

#[derive(Debug, Clone)]
pub struct UnitHydrographResult {
    pub area_acres: f64,
    pub tc_minutes: f64,
    pub storage_coefficient_minutes: f64,
    pub timestep_minutes: f64,
    pub peak_flow_cfs: f64,
    pub time_to_peak_minutes: f64,
    pub ordinates: Vec<HydrographOrdinate>,
}

pub fn storage_coefficient_minutes(tc_minutes: f64, storage_factor: f64) -> f64 {
    assert!(tc_minutes > 0.0 && storage_factor > 0.0);
    storage_factor * tc_minutes
}

pub fn time_area_histogram(num_steps: usize) -> Vec<f64> {
    assert!(num_steps > 0);
    let mut hist = Vec::with_capacity(num_steps);
    for i in 0..num_steps {
        let ratio = i as f64 / num_steps as f64;
        hist.push(if ratio <= 0.5 {
            2.0 * ratio
        } else {
            2.0 * (1.0 - ratio)
        });
    }
    let sum: f64 = hist.iter().sum();
    if sum > 0.0 {
        for v in &mut hist {
            *v /= sum;
        }
    }
    hist
}

pub fn generate(
    area_acres: f64,
    tc_minutes: f64,
    timestep_minutes: f64,
    storage_factor: f64,
    total_steps: Option<usize>,
) -> UnitHydrographResult {
    assert!(area_acres > 0.0 && tc_minutes > 0.0 && timestep_minutes > 0.0);
    let r_min = storage_coefficient_minutes(tc_minutes, storage_factor);
    let translation_steps = (tc_minutes / timestep_minutes).ceil() as usize;
    let n_steps = total_steps.unwrap_or_else(|| {
        (96_usize)
            .max(translation_steps + (5.0 * r_min / timestep_minutes).ceil() as usize)
    });

    let time_area = time_area_histogram(translation_steps);
    let c1 = timestep_minutes / (2.0 * r_min + timestep_minutes);
    let c2 = (2.0 * r_min - timestep_minutes) / (2.0 * r_min + timestep_minutes);

    let mut translated = vec![0.0; n_steps];
    for (i, &ta) in time_area.iter().enumerate() {
        if i < n_steps {
            translated[i] = ta * area_acres;
        }
    }

    let mut routed = vec![0.0; n_steps];
    for i in 1..n_steps {
        routed[i] = c1 * (translated[i] + translated[i - 1]) + c2 * routed[i - 1];
    }

    let mut ordinates = Vec::new();
    for i in 0..n_steps {
        ordinates.push(HydrographOrdinate {
            time_minutes: i as f64 * timestep_minutes,
            translated_flow_cfs: translated[i],
            flow_cfs: routed[i],
        });
    }

    let peak = ordinates
        .iter()
        .max_by(|a, b| a.flow_cfs.partial_cmp(&b.flow_cfs).unwrap())
        .cloned()
        .unwrap_or(HydrographOrdinate {
            time_minutes: 0.0,
            translated_flow_cfs: 0.0,
            flow_cfs: 0.0,
        });

    UnitHydrographResult {
        area_acres,
        tc_minutes,
        storage_coefficient_minutes: r_min,
        timestep_minutes,
        peak_flow_cfs: peak.flow_cfs,
        time_to_peak_minutes: peak.time_minutes,
        ordinates,
    }
}