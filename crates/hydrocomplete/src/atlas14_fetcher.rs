//! NOAA Atlas 14 PFDS live fetch + IDF curve fitting — mirrors `Atlas14Fetcher.cs`.

use std::io::{BufRead, BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use stormsewer::idf::IdfCurve;

use crate::atlas14_presets::{self, Atlas14Preset};

pub const DEFAULT_PFDS_INTENSITY_URL: &str =
    "https://hdsc.nws.noaa.gov/cgi-bin/new/fe_text.csv";

pub const DEFAULT_CACHE_TTL_DAYS: u64 = 30;

pub const STANDARD_RETURN_PERIODS: [i32; 4] = [2, 10, 25, 100];

pub const SUPPORTED_RETURN_PERIODS: [i32; 10] = [1, 2, 5, 10, 25, 50, 100, 200, 500, 1000];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atlas14Source {
    Live,
    Cache,
    Embedded,
}

impl Atlas14Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Cache => "cached live",
            Self::Embedded => "embedded",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Atlas14Resolution {
    pub source: Atlas14Source,
    pub lat: f64,
    pub lon: f64,
    pub return_period_years: i32,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub display_label: String,
    pub preset_key: Option<String>,
    pub project_area: Option<String>,
    pub fetched_utc: Option<u64>,
}

impl Atlas14Resolution {
    pub fn to_curve(&self) -> IdfCurve {
        IdfCurve::new(self.a, self.b, self.c)
    }

    pub fn from_preset(preset: &Atlas14Preset, return_period_years: i32) -> Result<Self, String> {
        let curve = preset.to_curve(return_period_years)?;
        Ok(Self {
            source: Atlas14Source::Embedded,
            lat: preset.lat,
            lon: preset.lon,
            return_period_years,
            a: curve.a,
            b: curve.b,
            c: curve.c,
            display_label: preset.display_name.to_string(),
            preset_key: Some(preset.key.to_string()),
            project_area: None,
            fetched_utc: None,
        })
    }

    pub fn embedded_nearest(lat: f64, lon: f64, return_period_years: i32) -> Result<Self, String> {
        Self::from_preset(atlas14_presets::nearest(lat, lon), return_period_years)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Atlas14CacheEntry {
    pub lat: f64,
    pub lon: f64,
    pub return_period_years: i32,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub project_area: Option<String>,
    pub fetched_utc: u64,
    pub expires_utc: u64,
}

impl Atlas14CacheEntry {
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.expires_utc
    }

    pub fn to_resolution(&self, source: Atlas14Source) -> Atlas14Resolution {
        Atlas14Resolution {
            source,
            lat: self.lat,
            lon: self.lon,
            return_period_years: self.return_period_years,
            a: self.a,
            b: self.b,
            c: self.c,
            display_label: format!("NOAA Atlas 14 @ {:.4}, {:.4}", self.lat, self.lon),
            preset_key: None,
            project_area: self.project_area.clone(),
            fetched_utc: Some(self.fetched_utc),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DurationIntensityPoint {
    pub duration_min: f64,
    pub intensity_in_hr: f64,
}

pub struct Atlas14Fetcher {
    pfds_url: String,
    cache_directory: Option<PathBuf>,
    cache_ttl: Duration,
}

impl Default for Atlas14Fetcher {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Atlas14Fetcher {
    pub fn new(cache_directory: Option<PathBuf>) -> Self {
        Self {
            pfds_url: DEFAULT_PFDS_INTENSITY_URL.into(),
            cache_directory,
            cache_ttl: Duration::from_secs(DEFAULT_CACHE_TTL_DAYS * 86400),
        }
    }

    pub fn resolve(&self, lat: f64, lon: f64, return_period_years: i32) -> Result<Atlas14Resolution, String> {
        validate_coordinates(lat, lon)?;
        if let Some(cached) = self.try_read_cache(lat, lon, return_period_years) {
            if !cached.is_expired() {
                return Ok(cached.to_resolution(Atlas14Source::Cache));
            }
        }
        match self.download_csv(lat, lon) {
            Ok(csv) => {
                let entry = parse_and_fit(&csv, lat, lon, return_period_years)?;
                self.write_cache(&entry);
                Ok(entry.to_resolution(Atlas14Source::Live))
            }
            Err(e) => {
                if let Some(cached) = self.try_read_cache(lat, lon, return_period_years) {
                    return Ok(cached.to_resolution(Atlas14Source::Cache));
                }
                Err(e)
            }
        }
    }

    pub fn resolve_with_fallback(
        &self,
        lat: f64,
        lon: f64,
        return_period_years: i32,
    ) -> Atlas14Resolution {
        self.resolve(lat, lon, return_period_years)
            .unwrap_or_else(|_| {
                Atlas14Resolution::embedded_nearest(lat, lon, return_period_years)
                    .expect("embedded preset")
            })
    }

    fn download_csv(&self, lat: f64, lon: f64) -> Result<String, String> {
        let url = format!(
            "{}?lat={lat:.4}&lon={lon:.4}&data=intensity&units=english&series=pds",
            self.pfds_url.trim_end_matches('?')
        );
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            .timeout_read(Duration::from_secs(8))
            .user_agent("HydroComplete-OpenCAD/0.4 (NOAA Atlas 14 PFDS)")
            .build();
        let resp = agent.get(&url).call().map_err(|e| e.to_string())?;
        if resp.status() >= 400 {
            return Err(format!("NOAA PFDS HTTP {}", resp.status()));
        }
        resp.into_string().map_err(|e| e.to_string())
    }

    fn try_read_cache(&self, lat: f64, lon: f64, rp: i32) -> Option<Atlas14CacheEntry> {
        let dir = self.cache_directory.as_ref()?;
        let path = cache_file_path(dir, lat, lon, rp);
        let json = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&json).ok()
    }

    fn write_cache(&self, entry: &Atlas14CacheEntry) {
        let Some(dir) = self.cache_directory.as_ref() else {
            return;
        };
        let _ = std::fs::create_dir_all(dir);
        if let Ok(json) = serde_json::to_string(entry) {
            let _ = std::fs::write(cache_file_path(dir, entry.lat, entry.lon, entry.return_period_years), json);
        }
    }
}

pub fn default_cache_directory() -> PathBuf {
    if let Some(base) = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    {
        base.join("HydroComplete").join("idf-cache")
    } else {
        PathBuf::from(".hydrocomplete-idf-cache")
    }
}

pub fn validate_coordinates(lat: f64, lon: f64) -> Result<(), String> {
    if lat.is_nan() || lon.is_nan() || lat.is_infinite() || lon.is_infinite() {
        return Err("Latitude and longitude must be finite.".into());
    }
    if !(-90.0..=90.0).contains(&lat) {
        return Err("Latitude must be between -90 and 90.".into());
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err("Longitude must be between -180 and 180.".into());
    }
    Ok(())
}

pub fn parse_and_fit(
    csv: &str,
    lat: f64,
    lon: f64,
    return_period_years: i32,
) -> Result<Atlas14CacheEntry, String> {
    if csv.trim().is_empty() {
        return Err("NOAA PFDS response was empty.".into());
    }
    if csv.to_ascii_lowercase().contains("errormsg") {
        return Err(extract_error_message(csv));
    }
    let project_area = extract_metadata(csv, "Project area:");
    let table = parse_intensity_table(csv, return_period_years)?;
    if table.len() < 3 {
        return Err("NOAA PFDS table did not contain enough duration rows.".into());
    }
    let (a, b, c) = idf_curve_fitter::fit(&table)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(Atlas14CacheEntry {
        lat,
        lon,
        return_period_years,
        a,
        b,
        c,
        project_area,
        fetched_utc: now,
        expires_utc: now + DEFAULT_CACHE_TTL_DAYS * 86400,
    })
}

pub fn parse_intensity_table(csv: &str, return_period_years: i32) -> Result<Vec<DurationIntensityPoint>, String> {
    let ari_col = return_period_column_index(return_period_years)?;
    let mut points = Vec::new();
    let mut in_main = false;
    for line in split_lines(csv) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if try_enter_intensity_table(line, &mut in_main) {
            continue;
        }
        if !in_main {
            continue;
        }
        if line.starts_with("PRECIPITATION FREQUENCY ESTIMATES AT") {
            break;
        }
        if let Some((dur, inten)) = parse_duration_row(line, ari_col) {
            if dur > 0.0 && inten > 0.0 {
                points.push(DurationIntensityPoint {
                    duration_min: dur,
                    intensity_in_hr: inten,
                });
            }
        }
    }
    Ok(points)
}

pub fn parse_intensities_at_duration(
    csv: &str,
    duration_min: f64,
    return_periods: Option<&[i32]>,
) -> Result<std::collections::HashMap<i32, f64>, String> {
    let periods: Vec<i32> = return_periods
        .map(|s| s.to_vec())
        .unwrap_or_else(|| STANDARD_RETURN_PERIODS.to_vec());
    let mut in_main = false;
    for line in split_lines(csv) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if try_enter_intensity_table(line, &mut in_main) {
            continue;
        }
        if !in_main {
            continue;
        }
        if line.starts_with("PRECIPITATION FREQUENCY ESTIMATES AT") {
            break;
        }
        let Some((row_dur, _)) = parse_duration_row(line, 0) else {
            continue;
        };
        if (row_dur - duration_min).abs() > 0.001 {
            continue;
        }
        let mut out = std::collections::HashMap::new();
        for rp in &periods {
            let col = return_period_column_index(*rp)?;
            if let Some((_, i)) = parse_duration_row(line, col) {
                out.insert(*rp, i);
            }
        }
        if out.is_empty() {
            return Err(format!(
                "NOAA PFDS table did not contain intensities at {duration_min:.1} min."
            ));
        }
        return Ok(out);
    }
    Err(format!(
        "NOAA PFDS table did not contain a row for {duration_min:.1} min duration."
    ))
}

pub fn format_multi_return_period_intensities(
    intensities: &std::collections::HashMap<i32, f64>,
) -> String {
    let mut parts = Vec::new();
    for rp in STANDARD_RETURN_PERIODS {
        if let Some(i) = intensities.get(&rp) {
            parts.push(format!("{rp}y={i:.2}"));
        }
    }
    parts.join(" ")
}

fn cache_file_path(dir: &Path, lat: f64, lon: f64, rp: i32) -> PathBuf {
    dir.join(format!("{lat:.4}_{lon:.4}_{rp}yr.json"))
}

fn extract_error_message(csv: &str) -> String {
    for line in split_lines(csv) {
        let lower = line.to_ascii_lowercase();
        if lower.contains("errormsg") {
            if let Some(idx) = lower.find('=') {
                let tail = line[idx + 1..].trim().trim_matches(|c| c == ';' || c == '\'' || c == '"');
                return tail.to_string();
            }
        }
    }
    "NOAA PFDS returned an error.".into()
}

fn extract_metadata(csv: &str, label: &str) -> Option<String> {
    for line in split_lines(csv) {
        if line.starts_with(label) {
            return Some(line[label.len()..].trim().to_string());
        }
    }
    None
}

fn try_enter_intensity_table(line: &str, in_main: &mut bool) -> bool {
    if line.starts_with("PRECIPITATION FREQUENCY ESTIMATES AT") {
        return false;
    }
    if line.to_ascii_lowercase().contains("by duration for ari") {
        *in_main = true;
        return true;
    }
    if line.starts_with("PRECIPITATION FREQUENCY ESTIMATES") {
        *in_main = line.to_ascii_lowercase().contains("by duration for ari");
        return true;
    }
    false
}

fn parse_duration_row(line: &str, ari_col: usize) -> Option<(f64, f64)> {
    let colon = line.find(':')?;
    let dur_token = line[..colon].trim();
    let duration_min = parse_duration_minutes(dur_token)?;
    let parts: Vec<&str> = line[colon + 1..].split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if parts.len() <= ari_col {
        return None;
    }
    let intensity: f64 = parts[ari_col].parse().ok()?;
    Some((duration_min, intensity))
}

fn parse_duration_minutes(token: &str) -> Option<f64> {
    let token = token.trim();
    if let Some(num) = token.strip_suffix("-min") {
        return num.parse().ok();
    }
    if let Some(num) = token.strip_suffix("-hr") {
        return num.parse::<f64>().ok().map(|h| h * 60.0);
    }
    if let Some(num) = token.strip_suffix("-day") {
        return num.parse::<f64>().ok().map(|d| d * 24.0 * 60.0);
    }
    None
}

fn return_period_column_index(rp: i32) -> Result<usize, String> {
    SUPPORTED_RETURN_PERIODS
        .iter()
        .position(|&p| p == rp)
        .ok_or_else(|| format!("Return period must be one of {:?}", SUPPORTED_RETURN_PERIODS))
}

fn split_lines(text: &str) -> Vec<String> {
    BufReader::new(Cursor::new(text))
        .lines()
        .filter_map(|l| l.ok())
        .collect()
}

mod idf_curve_fitter {
    use super::DurationIntensityPoint;

    struct IdfFit {
        a: f64,
        b: f64,
        c: f64,
    }

    const FIT_DURATIONS: [f64; 6] = [5.0, 10.0, 15.0, 30.0, 60.0, 120.0];

    pub fn fit(table: &[DurationIntensityPoint]) -> Result<(f64, f64, f64), String> {
        let fit_points = select_fit_points(table);
        if fit_points.len() < 3 {
            return Err("Not enough NOAA duration points to fit an IDF curve.".into());
        }
        let mut best = IdfFit { a: 0.0, b: 0.0, c: 0.0 };
        let mut best_err = f64::MAX;
        let mut b = 4.0;
        while b <= 20.0 {
            if let Some(candidate) = try_fit_for_b(&fit_points, b) {
                let err = sum_squared_relative_error(&fit_points, &candidate);
                if err < best_err {
                    best_err = err;
                    best = candidate;
                }
            }
            b += 0.25;
        }
        if best_err == f64::MAX {
            return Err("IDF curve fit did not converge.".into());
        }
        Ok((best.a, best.b, best.c))
    }

    fn select_fit_points(table: &[DurationIntensityPoint]) -> Vec<DurationIntensityPoint> {
        let mut selected = Vec::new();
        for &d in &FIT_DURATIONS {
            if let Some(p) = table
                .iter()
                .find(|p| (p.duration_min - d).abs() < 0.05)
            {
                selected.push(DurationIntensityPoint {
                    duration_min: d,
                    intensity_in_hr: p.intensity_in_hr,
                });
            }
        }
        if selected.len() >= 3 {
            return selected;
        }
        let mut sorted: Vec<_> = table
            .iter()
            .filter(|p| p.duration_min <= 180.0)
            .cloned()
            .collect();
        sorted.sort_by(|a, b| a.duration_min.partial_cmp(&b.duration_min).unwrap());
        sorted.into_iter().take(6).collect()
    }

    fn try_fit_for_b(points: &[DurationIntensityPoint], b: f64) -> Option<IdfFit> {
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xx = 0.0;
        let mut sum_xy = 0.0;
        let n = points.len() as f64;
        for p in points {
            let x = (p.duration_min + b).ln();
            let y = p.intensity_in_hr.ln();
            sum_x += x;
            sum_y += y;
            sum_xx += x * x;
            sum_xy += x * y;
        }
        let denom = n * sum_xx - sum_x * sum_x;
        if denom.abs() < 1e-12 {
            return None;
        }
        let c = -(n * sum_xy - sum_x * sum_y) / denom;
        let log_a = (sum_y + c * sum_x) / n;
        let a = log_a.exp();
        if a <= 0.0 || c <= 0.0 || !a.is_finite() || !c.is_finite() {
            return None;
        }
        Some(IdfFit { a, b, c })
    }

    fn sum_squared_relative_error(points: &[DurationIntensityPoint], fit: &IdfFit) -> f64 {
        points
            .iter()
            .map(|p| {
                let model = fit.a / (p.duration_min + fit.b).powf(fit.c);
                let rel = (model - p.intensity_in_hr) / p.intensity_in_hr;
                rel * rel
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(name: &str) -> String {
        fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .expect("fixture")
    }

    #[test]
    fn parse_charlotte_10yr() {
        let csv = fixture("charlotte_nc_intensity.csv");
        let table = parse_intensity_table(&csv, 10).unwrap();
        assert!(table.len() >= 6);
        assert!(table.iter().any(|p| (p.duration_min - 10.0).abs() < 0.01 && (p.intensity_in_hr - 5.81).abs() < 0.01));
    }

    #[test]
    fn intensities_at_10min() {
        let csv = fixture("charlotte_nc_intensity.csv");
        let m = parse_intensities_at_duration(&csv, 10.0, None).unwrap();
        assert!((m[&10] - 5.81).abs() < 0.01);
        assert!((m[&100] - 7.19).abs() < 0.01);
    }

    #[test]
    fn parse_and_fit_charlotte() {
        let csv = fixture("charlotte_nc_intensity.csv");
        let entry = parse_and_fit(&csv, 35.23, -80.84, 10).unwrap();
        assert_eq!(entry.project_area.as_deref(), Some("Ohio River Basin"));
        let i10 = entry.a / (10.0 + entry.b).powf(entry.c);
        assert!(i10 > 5.0 && i10 < 6.5);
    }
}