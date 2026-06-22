//! USDA NRCS Soil Data Access (SSURGO) fetcher with cache and regional fallback.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::soil_database::{self, SoilProperties};

pub const DEFAULT_SDA_ENDPOINT: &str =
    "https://sdmdataaccess.nrcs.usda.gov/Tabular/SDMTabularService/post.rest";

pub const DEFAULT_CACHE_TTL_DAYS: u64 = 30;

const COMPONENT_QUERY_TEMPLATE: &str = r#"
  SELECT mapunit.mukey, mapunit.muname,
         component.compname, component.comppct_r, component.hydgrp,
         chorizon.hzdept_r, chorizon.hzdepb_r,
         chorizon.sandtotal_r, chorizon.silttotal_r, chorizon.claytotal_r,
         chorizon.sandvc_r, chorizon.sandco_r, chorizon.sandmed_r,
         chorizon.sandfine_r, chorizon.sandvf_r,
         chorizon.kwfact, chorizon.kffact, chorizon.om_r, chorizon.dbthirdbar_r,
         chtexturegrp.texture
  FROM mapunit
  INNER JOIN component ON component.mukey = mapunit.mukey
  LEFT OUTER JOIN chorizon ON chorizon.cokey = component.cokey AND chorizon.hzdept_r = 0
  LEFT OUTER JOIN chtexturegrp ON chtexturegrp.chkey = chorizon.chkey AND chtexturegrp.rvindicator = 'Yes'
  WHERE component.majcompflag = 'Yes'
  AND mapunit.mukey IN (
    SELECT * FROM SDA_Get_Mukey_from_intersection_with_WktWgs84('__WKT__')
  )
  ORDER BY mapunit.mukey, component.comppct_r DESC"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsugroSource {
    Live,
    Cache,
    RegionalFallback,
    Embedded,
}

impl SsugroSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "SSURGO live",
            Self::Cache => "SSURGO cache",
            Self::RegionalFallback => "Regional fallback",
            Self::Embedded => "Embedded table",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SsugroSurfaceHorizon {
    pub pct_sand: Option<f64>,
    pub pct_silt: Option<f64>,
    pub pct_clay: Option<f64>,
    pub k_factor: Option<f64>,
    pub organic_matter: Option<f64>,
    pub bulk_density: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SsugroMapUnit {
    pub mukey: Option<String>,
    pub muname: String,
    pub dominant_component: Option<String>,
    pub dominant_pct: Option<f64>,
    pub hydrologic_soil_group: Option<char>,
    pub dominant_texture: Option<String>,
    pub surface_horizon: Option<SsugroSurfaceHorizon>,
    pub is_fallback: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SsugroResolution {
    pub source: SsugroSource,
    pub lat: f64,
    pub lon: f64,
    pub display_label: String,
    pub fetched_utc: Option<u64>,
    pub map_unit: SsugroMapUnit,
}

impl SsugroResolution {
    pub fn to_soil_properties(&self) -> SoilProperties {
        let hsg = self.map_unit.hydrologic_soil_group.unwrap_or('B');
        let k = self
            .map_unit
            .surface_horizon
            .as_ref()
            .and_then(|h| h.k_factor)
            .unwrap_or(0.30);
        let texture = self
            .map_unit
            .dominant_texture
            .clone()
            .unwrap_or_else(|| infer_texture(self.map_unit.surface_horizon.as_ref()));
        SoilProperties {
            key: normalize_soil_key(&self.map_unit.muname),
            name: self.map_unit.muname.clone(),
            series: self
                .map_unit
                .dominant_component
                .clone()
                .unwrap_or_else(|| self.map_unit.muname.clone()),
            region: match self.source {
                SsugroSource::Live | SsugroSource::Cache => {
                    format!("SSURGO @ {:.4}, {:.4}", self.lat, self.lon)
                }
                _ => "Regional estimate".into(),
            },
            texture,
            hydrologic_soil_group: hsg,
            k_factor: k,
            infiltration_rate_in_per_hr: infiltration_for_hsg(hsg),
            drainage: drainage_for_hsg(hsg).into(),
        }
    }

    pub fn regional_fallback(lat: f64, lon: f64) -> Self {
        let mut unit = regional_nearest(lat, lon);
        unit.is_fallback = true;
        Self {
            source: SsugroSource::RegionalFallback,
            lat,
            lon,
            display_label: unit.muname.clone(),
            fetched_utc: None,
            map_unit: unit,
        }
    }

    pub fn embedded(soil_name: &str, lat: f64, lon: f64) -> Result<Self, String> {
        let soil = soil_database::lookup(soil_name)?;
        Ok(Self {
            source: SsugroSource::Embedded,
            lat,
            lon,
            display_label: soil.name.clone(),
            fetched_utc: None,
            map_unit: SsugroMapUnit {
                muname: soil.name.clone(),
                dominant_component: Some(soil.series.clone()),
                hydrologic_soil_group: Some(soil.hydrologic_soil_group),
                dominant_texture: Some(soil.texture.clone()),
                surface_horizon: Some(SsugroSurfaceHorizon {
                    k_factor: Some(soil.k_factor),
                    ..Default::default()
                }),
                is_fallback: true,
                warning: Some("Embedded soil table — not live SSURGO.".into()),
                ..Default::default()
            },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SsugroCacheEntry {
    lat: f64,
    lon: f64,
    fetched_utc: u64,
    map_unit: SsugroMapUnit,
}

impl SsugroCacheEntry {
    fn is_expired(&self, ttl: Duration) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.fetched_utc) > ttl.as_secs()
    }

    fn to_resolution(&self, source: SsugroSource) -> SsugroResolution {
        SsugroResolution {
            source,
            lat: self.lat,
            lon: self.lon,
            display_label: self.map_unit.muname.clone(),
            fetched_utc: Some(self.fetched_utc),
            map_unit: self.map_unit.clone(),
        }
    }

    fn from_map_unit(lat: f64, lon: f64, unit: SsugroMapUnit) -> Self {
        let fetched_utc = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            lat,
            lon,
            fetched_utc,
            map_unit: unit,
        }
    }
}

pub struct SsugroFetcher {
    sda_endpoint: String,
    cache_directory: Option<PathBuf>,
    cache_ttl: Duration,
}

impl Default for SsugroFetcher {
    fn default() -> Self {
        Self::new(None)
    }
}

impl SsugroFetcher {
    pub fn new(cache_directory: Option<PathBuf>) -> Self {
        Self {
            sda_endpoint: DEFAULT_SDA_ENDPOINT.into(),
            cache_directory,
            cache_ttl: Duration::from_secs(DEFAULT_CACHE_TTL_DAYS * 86400),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.sda_endpoint = endpoint.into();
        self
    }

    pub fn resolve(&self, lat: f64, lon: f64) -> SsugroResolution {
        if let Err(e) = validate_coordinates(lat, lon) {
            let mut unit = regional_nearest(lat, lon);
            unit.warning = Some(e);
            return SsugroResolution {
                source: SsugroSource::RegionalFallback,
                lat,
                lon,
                display_label: unit.muname.clone(),
                fetched_utc: None,
                map_unit: unit,
            };
        }

        if let Some(cached) = self.try_read_cache(lat, lon) {
            if !cached.is_expired(self.cache_ttl) {
                return cached.to_resolution(SsugroSource::Cache);
            }
        }

        match self.download_sda(lat, lon) {
            Ok(json) => {
                let rows = parse_sda_table(&json);
                let units = aggregate_components(&rows);
                if units.is_empty() {
                    let mut res = SsugroResolution::regional_fallback(lat, lon);
                    res.map_unit.warning = Some(
                        "SSURGO has no coverage at this point. Using regional defaults — verify before use.".into(),
                    );
                    return res;
                }
                let entry = SsugroCacheEntry::from_map_unit(lat, lon, units[0].clone());
                self.write_cache(&entry);
                entry.to_resolution(SsugroSource::Live)
            }
            Err(_) => {
                if let Some(cached) = self.try_read_cache(lat, lon) {
                    return cached.to_resolution(SsugroSource::Cache);
                }
                SsugroResolution::regional_fallback(lat, lon)
            }
        }
    }

    fn download_sda(&self, lat: f64, lon: f64) -> Result<String, String> {
        let wkt = format!("point({lon} {lat})");
        let query = COMPONENT_QUERY_TEMPLATE.replace("__WKT__", &wkt);
        let body = serde_json::json!({ "format": "JSON+COLUMNNAME", "query": query });
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            .timeout_read(Duration::from_secs(8))
            .user_agent("HydroComplete-OpenCAD/0.3")
            .build();
        let resp = agent
            .post(&self.sda_endpoint)
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| e.to_string())?;
        if resp.status() >= 400 {
            return Err(format!("SDA HTTP {}", resp.status()));
        }
        resp.into_string().map_err(|e| e.to_string())
    }

    fn try_read_cache(&self, lat: f64, lon: f64) -> Option<SsugroCacheEntry> {
        let dir = self.cache_directory.as_ref()?;
        let path = cache_path(dir, lat, lon);
        let json = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&json).ok()
    }

    fn write_cache(&self, entry: &SsugroCacheEntry) {
        let Some(dir) = self.cache_directory.as_ref() else {
            return;
        };
        let _ = std::fs::create_dir_all(dir);
        if let Ok(json) = serde_json::to_string(entry) {
            let _ = std::fs::write(cache_path(dir, entry.lat, entry.lon), json);
        }
    }
}

pub fn validate_coordinates(lat: f64, lon: f64) -> Result<(), String> {
    if !(-90.0..=90.0).contains(&lat) {
        return Err("Latitude must be between -90 and 90.".into());
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err("Longitude must be between -180 and 180.".into());
    }
    Ok(())
}

pub fn parse_sda_table(json: &str) -> Vec<HashMap<String, Option<String>>> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(table) = v.get("Table").and_then(|t| t.as_array()) else {
        return Vec::new();
    };
    if table.len() < 2 {
        return Vec::new();
    }
    let columns: Vec<String> = table[0]
        .as_array()
        .map(|row| {
            row.iter()
                .map(|c| c.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();

    let mut rows = Vec::new();
    for row_el in table.iter().skip(1) {
        let Some(cells) = row_el.as_array() else {
            continue;
        };
        let mut row = HashMap::new();
        for (i, col) in columns.iter().enumerate() {
            let val = cells.get(i).map(|c| {
                if c.is_null() {
                    None
                } else {
                    Some(c.to_string().trim_matches('"').to_string())
                }
            });
            row.insert(col.to_lowercase(), val.flatten());
        }
        rows.push(row);
    }
    rows
}

struct ComponentRow {
    name: String,
    pct: f64,
    hsg: Option<char>,
    texture: Option<String>,
    surface: SsugroSurfaceHorizon,
}

pub fn aggregate_components(rows: &[HashMap<String, Option<String>>]) -> Vec<SsugroMapUnit> {
    let mut by_mukey: HashMap<String, (String, Vec<ComponentRow>)> = HashMap::new();
    for row in rows {
        let mukey = get(row, "mukey");
        let Some(mukey) = mukey.filter(|s| !s.is_empty()) else {
            continue;
        };
        let bucket = by_mukey
            .entry(mukey.to_string())
            .or_insert_with(|| {
                (
                    get(row, "muname")
                        .unwrap_or(mukey)
                        .to_string(),
                    Vec::new(),
                )
            });
        bucket.1.push(ComponentRow {
            name: get(row, "compname").unwrap_or_default().to_string(),
            pct: parse_double(get(row, "comppct_r")).unwrap_or(0.0),
            hsg: parse_hsg(get(row, "hydgrp")),
            texture: get(row, "texture").map(str::to_string),
            surface: SsugroSurfaceHorizon {
                pct_sand: parse_double(get(row, "sandtotal_r")),
                pct_silt: parse_double(get(row, "silttotal_r")),
                pct_clay: parse_double(get(row, "claytotal_r")),
                k_factor: parse_double(get(row, "kwfact")).or_else(|| parse_double(get(row, "kffact"))),
                organic_matter: parse_double(get(row, "om_r")),
                bulk_density: parse_double(get(row, "dbthirdbar_r")),
            },
        });
    }

    let mut results = Vec::new();
    for (mukey, (muname, mut comps)) in by_mukey {
        comps.sort_by(|a, b| b.pct.partial_cmp(&a.pct).unwrap_or(std::cmp::Ordering::Equal));
        let dominant = &comps[0];
        results.push(SsugroMapUnit {
            mukey: Some(mukey),
            muname,
            dominant_component: Some(dominant.name.clone()),
            dominant_pct: Some(dominant.pct),
            hydrologic_soil_group: dominant.hsg,
            dominant_texture: dominant.texture.clone(),
            surface_horizon: Some(dominant.surface.clone()),
            ..Default::default()
        });
    }
    results.sort_by(|a, b| {
        b.dominant_pct
            .unwrap_or(0.0)
            .partial_cmp(&a.dominant_pct.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

fn get<'a>(row: &'a HashMap<String, Option<String>>, key: &str) -> Option<&'a str> {
    row.get(&key.to_lowercase()).and_then(|v| v.as_deref())
}

fn parse_double(text: Option<&str>) -> Option<f64> {
    text?.trim().parse::<f64>().ok()
}

fn parse_hsg(text: Option<&str>) -> Option<char> {
    let c = text?.trim().chars().next()?.to_ascii_uppercase();
    matches!(c, 'A' | 'B' | 'C' | 'D').then_some(c)
}

fn normalize_soil_key(name: &str) -> String {
    if name.trim().is_empty() {
        return "unknown".into();
    }
    name.trim().to_lowercase().replace(' ', "-")
}

fn infer_texture(hz: Option<&SsugroSurfaceHorizon>) -> String {
    let Some(hz) = hz else {
        return "loam".into();
    };
    let (Some(sand), Some(silt), Some(clay)) = (hz.pct_sand, hz.pct_silt, hz.pct_clay) else {
        return "loam".into();
    };
    if clay >= 40.0 {
        "clay".into()
    } else if sand >= 70.0 {
        "sand".into()
    } else if silt >= 50.0 {
        "silt loam".into()
    } else {
        "loam".into()
    }
}

fn infiltration_for_hsg(hsg: char) -> f64 {
    match hsg {
        'A' => 0.60,
        'B' => 0.25,
        'C' => 0.10,
        'D' => 0.03,
        _ => 0.25,
    }
}

fn drainage_for_hsg(hsg: char) -> &'static str {
    match hsg {
        'A' => "well drained",
        'B' => "moderately well drained",
        'C' => "somewhat poorly drained",
        'D' => "poorly drained",
        _ => "unknown",
    }
}

fn cache_path(dir: &Path, lat: f64, lon: f64) -> PathBuf {
    dir.join(format!("{lat:.4}_{lon:.4}.json"))
}

struct RegionTemplate {
    lat_min: f64,
    lat_max: f64,
    lon_min: f64,
    lon_max: f64,
    name: &'static str,
    hsg: char,
    sand: f64,
    silt: f64,
    clay: f64,
    k: f64,
    om: f64,
    bd: f64,
}

const REGIONS: &[RegionTemplate] = &[
    RegionTemplate { lat_min: 24.0, lat_max: 31.0, lon_min: -90.0, lon_max: -78.0, name: "Florida/Gulf Coast", hsg: 'A', sand: 75.0, silt: 15.0, clay: 10.0, k: 0.15, om: 1.0, bd: 1.55 },
    RegionTemplate { lat_min: 31.0, lat_max: 36.0, lon_min: -82.0, lon_max: -75.0, name: "Southeast Coast", hsg: 'B', sand: 55.0, silt: 25.0, clay: 20.0, k: 0.22, om: 1.5, bd: 1.50 },
    RegionTemplate { lat_min: 31.0, lat_max: 36.0, lon_min: -90.0, lon_max: -82.0, name: "Southeast Inland", hsg: 'B', sand: 45.0, silt: 30.0, clay: 25.0, k: 0.28, om: 2.0, bd: 1.45 },
    RegionTemplate { lat_min: 35.0, lat_max: 41.0, lon_min: -85.0, lon_max: -75.0, name: "Piedmont/Mid-Atlantic", hsg: 'B', sand: 35.0, silt: 35.0, clay: 30.0, k: 0.32, om: 2.0, bd: 1.45 },
    RegionTemplate { lat_min: 41.0, lat_max: 47.0, lon_min: -80.0, lon_max: -67.0, name: "Northeast", hsg: 'C', sand: 40.0, silt: 40.0, clay: 20.0, k: 0.30, om: 3.0, bd: 1.40 },
    RegionTemplate { lat_min: 36.0, lat_max: 42.0, lon_min: -90.0, lon_max: -80.0, name: "Ohio Valley", hsg: 'C', sand: 25.0, silt: 50.0, clay: 25.0, k: 0.35, om: 2.5, bd: 1.40 },
    RegionTemplate { lat_min: 42.0, lat_max: 49.0, lon_min: -97.0, lon_max: -82.0, name: "Upper Midwest", hsg: 'B', sand: 30.0, silt: 45.0, clay: 25.0, k: 0.32, om: 3.5, bd: 1.40 },
    RegionTemplate { lat_min: 28.0, lat_max: 36.0, lon_min: -100.0, lon_max: -90.0, name: "South Central", hsg: 'C', sand: 30.0, silt: 35.0, clay: 35.0, k: 0.30, om: 1.5, bd: 1.45 },
    RegionTemplate { lat_min: 42.0, lat_max: 49.0, lon_min: -105.0, lon_max: -97.0, name: "Northern Plains", hsg: 'B', sand: 40.0, silt: 40.0, clay: 20.0, k: 0.28, om: 2.5, bd: 1.42 },
    RegionTemplate { lat_min: 32.0, lat_max: 42.0, lon_min: -105.0, lon_max: -97.0, name: "Southern Plains", hsg: 'C', sand: 35.0, silt: 35.0, clay: 30.0, k: 0.30, om: 1.5, bd: 1.45 },
    RegionTemplate { lat_min: 26.0, lat_max: 32.0, lon_min: -98.0, lon_max: -92.0, name: "Texas Coast", hsg: 'D', sand: 20.0, silt: 40.0, clay: 40.0, k: 0.32, om: 1.5, bd: 1.40 },
    RegionTemplate { lat_min: 32.0, lat_max: 49.0, lon_min: -114.0, lon_max: -105.0, name: "Rockies", hsg: 'B', sand: 50.0, silt: 30.0, clay: 20.0, k: 0.24, om: 2.0, bd: 1.45 },
    RegionTemplate { lat_min: 28.0, lat_max: 38.0, lon_min: -118.0, lon_max: -109.0, name: "Southwest", hsg: 'A', sand: 70.0, silt: 20.0, clay: 10.0, k: 0.18, om: 0.5, bd: 1.55 },
    RegionTemplate { lat_min: 36.0, lat_max: 44.0, lon_min: -120.0, lon_max: -113.0, name: "Great Basin", hsg: 'B', sand: 55.0, silt: 30.0, clay: 15.0, k: 0.22, om: 1.0, bd: 1.50 },
    RegionTemplate { lat_min: 42.0, lat_max: 49.0, lon_min: -125.0, lon_max: -118.0, name: "Pacific Northwest", hsg: 'B', sand: 45.0, silt: 35.0, clay: 20.0, k: 0.28, om: 3.0, bd: 1.40 },
    RegionTemplate { lat_min: 32.0, lat_max: 42.0, lon_min: -125.0, lon_max: -119.0, name: "California Coast", hsg: 'C', sand: 40.0, silt: 35.0, clay: 25.0, k: 0.30, om: 2.0, bd: 1.45 },
    RegionTemplate { lat_min: 32.0, lat_max: 42.0, lon_min: -119.0, lon_max: -114.0, name: "California Inland", hsg: 'B', sand: 50.0, silt: 30.0, clay: 20.0, k: 0.26, om: 1.5, bd: 1.48 },
];

fn regional_nearest(lat: f64, lon: f64) -> SsugroMapUnit {
    let mut matched = None;
    for r in REGIONS {
        if lat >= r.lat_min && lat <= r.lat_max && lon >= r.lon_min && lon <= r.lon_max {
            matched = Some(r);
            break;
        }
    }
    let r = matched.unwrap_or(&RegionTemplate {
        lat_min: 0.0,
        lat_max: 0.0,
        lon_min: 0.0,
        lon_max: 0.0,
        name: "Continental US (generic)",
        hsg: 'B',
        sand: 40.0,
        silt: 40.0,
        clay: 20.0,
        k: 0.30,
        om: 2.0,
        bd: 1.45,
    });
    SsugroMapUnit {
        muname: format!("{} regional default (not SSURGO-verified)", r.name),
        dominant_component: Some(format!("{} representative", r.name)),
        hydrologic_soil_group: Some(r.hsg),
        is_fallback: true,
        surface_horizon: Some(SsugroSurfaceHorizon {
            pct_sand: Some(r.sand),
            pct_silt: Some(r.silt),
            pct_clay: Some(r.clay),
            k_factor: Some(r.k),
            organic_matter: Some(r.om),
            bulk_density: Some(r.bd),
        }),
        ..Default::default()
    }
}

pub fn default_cache_directory() -> PathBuf {
    dirs_or_fallback()
}

fn dirs_or_fallback() -> PathBuf {
    if let Some(base) = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    {
        base.join("HydroComplete").join("ssurgo-cache")
    } else {
        PathBuf::from(".hydrocomplete-ssurgo-cache")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        fs::read_to_string(path).expect("fixture")
    }

    #[test]
    fn parse_cecil_fixture() {
        let rows = parse_sda_table(&fixture("ssurgo_cecil_sda.json"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("muname").and_then(|v| v.as_deref()), Some("Cecil sandy loam"));
    }

    #[test]
    fn aggregate_cecil_dominant() {
        let rows = parse_sda_table(&fixture("ssurgo_cecil_sda.json"));
        let units = aggregate_components(&rows);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].dominant_component.as_deref(), Some("Cecil"));
        assert_eq!(units[0].hydrologic_soil_group, Some('B'));
        assert!((units[0].surface_horizon.as_ref().unwrap().k_factor.unwrap() - 0.24).abs() < 0.01);
    }

    #[test]
    fn regional_piedmont_hsg_b() {
        let res = SsugroResolution::regional_fallback(35.5, -80.0);
        assert_eq!(res.source, SsugroSource::RegionalFallback);
        assert_eq!(res.map_unit.hydrologic_soil_group, Some('B'));
    }

    #[test]
    fn to_soil_properties_maps_hsg() {
        let res = SsugroResolution {
            source: SsugroSource::Live,
            lat: 35.5,
            lon: -82.5,
            display_label: "Cecil sandy loam".into(),
            fetched_utc: None,
            map_unit: SsugroMapUnit {
                muname: "Cecil sandy loam".into(),
                dominant_component: Some("Cecil".into()),
                hydrologic_soil_group: Some('B'),
                dominant_texture: Some("sandy loam".into()),
                surface_horizon: Some(SsugroSurfaceHorizon {
                    k_factor: Some(0.24),
                    ..Default::default()
                }),
                ..Default::default()
            },
        };
        let soil = res.to_soil_properties();
        assert_eq!(soil.hydrologic_soil_group, 'B');
        assert!((soil.k_factor - 0.24).abs() < 0.01);
    }
}