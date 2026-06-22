//! Embedded USDA-NRCS soil map unit lookup: HSG, K-factor, infiltration, BMP suitability.

use crate::bmp::bmp_type;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BmpSuitability {
    Excellent,
    Good,
    Marginal,
    Poor,
    NotRecommended,
}

impl BmpSuitability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Excellent => "Excellent",
            Self::Good => "Good",
            Self::Marginal => "Marginal",
            Self::Poor => "Poor",
            Self::NotRecommended => "NotRecommended",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SoilProperties {
    pub key: String,
    pub name: String,
    pub series: String,
    pub region: String,
    pub texture: String,
    pub hydrologic_soil_group: char,
    pub k_factor: f64,
    pub infiltration_rate_in_per_hr: f64,
    pub drainage: String,
}

#[derive(Debug, Clone)]
pub struct BmpSuggestionResult {
    pub soil: SoilProperties,
    pub bmp_type: String,
    pub suitability: BmpSuitability,
    pub rationale: String,
    pub alternatives: Vec<String>,
}

fn normalize_key(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .replace(' ', "-")
        .replace('_', "-")
}

fn normalize_bmp_type(bmp_type: &str) -> String {
    let lower = bmp_type.trim().to_lowercase().replace(' ', "-");
    match lower.as_str() {
        "wetpond" | "wet-pond" => bmp_type::WET_POND.to_string(),
        "rain-garden" | "rain_garden" => bmp_type::BIORETENTION.to_string(),
        "constructed_wetland" | "constructed-wetland" | "wetland" => "constructed-wetland".into(),
        "infiltration" => "infiltration-basin".into(),
        "" => bmp_type::BIORETENTION.to_string(),
        _ => lower,
    }
}

fn s(
    key: &str,
    name: &str,
    series: &str,
    region: &str,
    texture: &str,
    hsg: char,
    k: f64,
    fc: f64,
    drainage: &str,
) -> SoilProperties {
    SoilProperties {
        key: key.into(),
        name: name.into(),
        series: series.into(),
        region: region.into(),
        texture: texture.into(),
        hydrologic_soil_group: hsg,
        k_factor: k,
        infiltration_rate_in_per_hr: fc,
        drainage: drainage.into(),
    }
}

fn soil_table() -> &'static [SoilProperties] {
    static SOILS: std::sync::OnceLock<Vec<SoilProperties>> = std::sync::OnceLock::new();
    SOILS.get_or_init(|| {
        vec![
            s("cecil-sandy-loam", "Cecil sandy loam", "Cecil", "Piedmont (NC, SC, GA, VA)", "sandy-loam", 'B', 0.24, 0.60, "well drained"),
            s("cecil-clay-loam", "Cecil clay loam", "Cecil", "Piedmont (NC, SC, GA, VA)", "clay-loam", 'C', 0.28, 0.20, "well drained"),
            s("pacolet-sandy-loam", "Pacolet sandy loam", "Pacolet", "Piedmont (NC, SC, VA)", "sandy-loam", 'B', 0.28, 0.50, "well drained"),
            s("madison-sandy-loam", "Madison sandy loam", "Madison", "Piedmont (NC, VA)", "sandy-loam", 'B', 0.24, 0.55, "well drained"),
            s("appling-sandy-loam", "Appling sandy loam", "Appling", "Piedmont (NC, SC, GA, VA)", "sandy-loam", 'B', 0.24, 0.60, "well drained"),
            s("wedowee-sandy-loam", "Wedowee sandy loam", "Wedowee", "Piedmont (NC, SC, GA, AL)", "sandy-loam", 'B', 0.24, 0.55, "well drained"),
            s("georgeville-silt-loam", "Georgeville silt loam", "Georgeville", "Piedmont (NC, SC, VA)", "silt-loam", 'B', 0.37, 0.35, "well drained"),
            s("herndon-silt-loam", "Herndon silt loam", "Herndon", "Piedmont (NC, SC, VA)", "silt-loam", 'B', 0.37, 0.30, "well drained"),
            s("iredell-loam", "Iredell loam", "Iredell", "Piedmont (NC, SC)", "clay-loam", 'D', 0.32, 0.05, "moderately well drained"),
            s("mecklenburg-loam", "Mecklenburg loam", "Mecklenburg", "Piedmont (NC, SC)", "clay-loam", 'C', 0.32, 0.15, "well drained"),
            s("norfolk-sandy-loam", "Norfolk sandy loam", "Norfolk", "Coastal Plain (NC, SC, VA, GA)", "sandy-loam", 'A', 0.17, 0.80, "well drained"),
            s("goldsboro-sandy-loam", "Goldsboro sandy loam", "Goldsboro", "Coastal Plain (NC, SC, VA)", "sandy-loam", 'B', 0.20, 0.45, "moderately well drained"),
            s("lynchburg-sandy-loam", "Lynchburg sandy loam", "Lynchburg", "Coastal Plain (NC, SC, VA)", "sandy-loam", 'C', 0.20, 0.15, "somewhat poorly drained"),
            s("rains-sandy-loam", "Rains sandy loam", "Rains", "Coastal Plain (NC, SC)", "sandy-loam", 'D', 0.17, 0.03, "poorly drained"),
            s("wagram-sand", "Wagram sand", "Wagram", "Coastal Plain (NC, SC)", "sand", 'A', 0.10, 1.50, "well drained"),
            s("hayesville-loam", "Hayesville loam", "Hayesville", "Blue Ridge (NC, SC, GA, VA)", "loam", 'B', 0.28, 0.40, "well drained"),
            s("evard-sandy-loam", "Evard sandy loam", "Evard", "Blue Ridge (NC, SC, GA)", "sandy-loam", 'B', 0.24, 0.50, "well drained"),
            s("davidson-clay-loam", "Davidson clay loam", "Davidson", "Piedmont (NC, TN, VA)", "clay-loam", 'C', 0.30, 0.12, "well drained"),
            s("chewacla-loam", "Chewacla loam", "Chewacla", "Piedmont (NC, SC)", "loam", 'B', 0.32, 0.35, "well drained"),
            s("cataula-sandy-loam", "Cataula sandy loam", "Cataula", "Piedmont (NC, SC, GA)", "sandy-loam", 'B', 0.26, 0.55, "well drained"),
            s("duplin-sandy-loam", "Duplin sandy loam", "Duplin", "Coastal Plain (NC, SC)", "sandy-loam", 'B', 0.18, 0.40, "moderately well drained"),
            s("cape-fear-loam", "Cape Fear loam", "Cape Fear", "Coastal Plain (NC, SC)", "loam", 'D', 0.20, 0.04, "poorly drained"),
            s("johnston-loam", "Johnston loam", "Johnston", "Coastal Plain (NC, SC, VA)", "loam", 'C', 0.22, 0.12, "somewhat poorly drained"),
            s("roanoke-loam", "Roanoke loam", "Roanoke", "Coastal Plain (NC, SC, VA)", "loam", 'D', 0.24, 0.03, "poorly drained"),
            s("platte-silt-loam", "Platte silt loam", "Platte", "Midwest (IA, NE)", "silt-loam", 'B', 0.42, 0.25, "well drained"),
            s("miami-silt-loam", "Miami silt loam", "Miami", "Midwest (IN, OH)", "silt-loam", 'B', 0.35, 0.28, "well drained"),
            s("drummer-silty-clay-loam", "Drummer silty clay loam", "Drummer", "Midwest (IL)", "silty-clay-loam", 'D', 0.28, 0.04, "poorly drained"),
            s("hagerstown-silt-loam", "Hagerstown silt loam", "Hagerstown", "Appalachian (PA, MD)", "silt-loam", 'B', 0.32, 0.30, "well drained"),
            s("frederick-silt-loam", "Frederick silt loam", "Frederick", "Appalachian (VA, WV)", "silt-loam", 'B', 0.30, 0.32, "well drained"),
            s("myatt-sandy-loam", "Myatt sandy loam", "Myatt", "Coastal Plain (SC, GA)", "sandy-loam", 'A', 0.15, 0.90, "well drained"),
            s("barnwell-sandy-loam", "Barnwell sandy loam", "Barnwell", "Coastal Plain (SC, GA)", "sandy-loam", 'B', 0.19, 0.42, "moderately well drained"),
            s("marlboro-sandy-loam", "Marlboro sandy loam", "Marlboro", "Coastal Plain (SC, NC)", "sandy-loam", 'C', 0.21, 0.14, "somewhat poorly drained"),
            s("buncombe-silt-loam", "Buncombe silt loam", "Buncombe", "Piedmont (NC, SC)", "silt-loam", 'B', 0.34, 0.32, "well drained"),
            s("cleveland-silt-loam", "Cleveland silt loam", "Cleveland", "Piedmont (NC, SC)", "silt-loam", 'B', 0.33, 0.30, "well drained"),
            s("rhodhiss-sandy-loam", "Rhodhiss sandy loam", "Rhodhiss", "Piedmont (NC)", "sandy-loam", 'B', 0.25, 0.52, "well drained"),
            s("enon-sandy-loam", "Enon sandy loam", "Enon", "Piedmont (NC, SC, VA)", "sandy-loam", 'B', 0.27, 0.48, "well drained"),
            s("wake-sandy-loam", "Wake sandy loam", "Wake", "Piedmont (NC)", "sandy-loam", 'B', 0.26, 0.50, "well drained"),
            s("davidson-sandy-clay-loam", "Davidson sandy clay loam", "Davidson", "Piedmont (NC, TN)", "sandy-clay-loam", 'C', 0.31, 0.10, "well drained"),
            s("catawba-sandy-loam", "Catawba sandy loam", "Catawba", "Piedmont (NC, SC)", "sandy-loam", 'B', 0.25, 0.53, "well drained"),
            s("orangeburg-sandy-loam", "Orangeburg sandy loam", "Orangeburg", "Coastal Plain (SC, GA, FL)", "sandy-loam", 'A', 0.16, 0.85, "well drained"),
            s("lucy-sandy-loam", "Lucy sandy loam", "Lucy", "Coastal Plain (SC, GA)", "sandy-loam", 'A', 0.14, 0.95, "well drained"),
            s("kalmia-sand", "Kalmia sand", "Kalmia", "Coastal Plain (SC, NC)", "sand", 'A', 0.11, 1.30, "excessively drained"),
            s("leon-sand", "Leon sand", "Leon", "Coastal Plain (FL, GA)", "sand", 'A', 0.08, 1.20, "excessively drained"),
            s("brookman-loam", "Brookman loam", "Brookman", "Coastal Plain (SC, GA)", "loam", 'B', 0.23, 0.35, "well drained"),
            s("generic-sand", "Sand (generic)", "(generic)", "All", "sand", 'A', 0.05, 1.20, "excessively drained"),
            s("generic-loamy-sand", "Loamy sand (generic)", "(generic)", "All", "loamy-sand", 'A', 0.12, 0.80, "well drained"),
            s("generic-sandy-loam", "Sandy loam (generic)", "(generic)", "All", "sandy-loam", 'B', 0.27, 0.45, "well drained"),
            s("generic-loam", "Loam (generic)", "(generic)", "All", "loam", 'B', 0.38, 0.25, "well drained"),
            s("generic-silt-loam", "Silt loam (generic)", "(generic)", "All", "silt-loam", 'B', 0.48, 0.20, "moderately well drained"),
            s("generic-clay-loam", "Clay loam (generic)", "(generic)", "All", "clay-loam", 'C', 0.37, 0.10, "moderately well drained"),
            s("generic-clay", "Clay (generic)", "(generic)", "All", "clay", 'D', 0.13, 0.03, "poorly drained"),
        ]
    })
}

/// Lookup soil properties by map unit name or series key (case-insensitive, fuzzy).
pub fn lookup(soil_name: &str) -> Result<SoilProperties, String> {
    if soil_name.trim().is_empty() {
        return Err("Soil name is required.".into());
    }
    let normalized = normalize_key(soil_name);
    for soil in soil_table() {
        if soil.key.eq_ignore_ascii_case(&normalized) {
            return Ok(soil.clone());
        }
    }
    for soil in soil_table() {
        let name = normalize_key(&soil.name);
        let series = normalize_key(&soil.series);
        if name.contains(&normalized)
            || series.contains(&normalized)
            || normalized.contains(&series)
        {
            return Ok(soil.clone());
        }
    }
    Err(format!("Unknown soil map unit: {soil_name}"))
}

fn evaluate_infiltration_bmp(
    soil: &SoilProperties,
) -> (BmpSuitability, String, Vec<String>) {
    let mut alternatives = Vec::new();
    let (suitability, rationale) = match soil.hydrologic_soil_group {
        'A' => (
            BmpSuitability::Excellent,
            format!(
                "HSG A ({:.2} in/hr) — high infiltration supports bioretention.",
                soil.infiltration_rate_in_per_hr
            ),
        ),
        'B' => (
            BmpSuitability::Good,
            format!(
                "HSG B ({:.2} in/hr) — bioretention feasible with amended media.",
                soil.infiltration_rate_in_per_hr
            ),
        ),
        'C' => {
            alternatives.push(bmp_type::WET_POND.into());
            alternatives.push("constructed-wetland".into());
            (
                BmpSuitability::Marginal,
                format!(
                    "HSG C ({:.2} in/hr) — limited infiltration; underdrain required.",
                    soil.infiltration_rate_in_per_hr
                ),
            )
        }
        'D' => {
            alternatives.push(bmp_type::WET_POND.into());
            alternatives.push("constructed-wetland".into());
            (
                BmpSuitability::NotRecommended,
                format!(
                    "HSG D ({:.2} in/hr) — infiltration BMP not recommended on {} soils.",
                    soil.infiltration_rate_in_per_hr, soil.drainage
                ),
            )
        }
        _ => (
            BmpSuitability::Marginal,
            format!("HSG {} — verify infiltration BMP feasibility.", soil.hydrologic_soil_group),
        ),
    };
    (suitability, rationale, alternatives)
}

/// Evaluate BMP suitability from resolved soil properties.
pub fn suggest_bmp(soil: &SoilProperties, bmp_type: &str) -> BmpSuggestionResult {
    let bmp = normalize_bmp_type(bmp_type);
    let (suitability, rationale, alternatives) = match bmp.as_str() {
        bmp_type::BIORETENTION | "rain-garden" | "infiltration-basin" => {
            evaluate_infiltration_bmp(soil)
        }
        bmp_type::WET_POND => {
            let mut alts = Vec::new();
            if soil.hydrologic_soil_group == 'A' {
                alts.push(bmp_type::BIORETENTION.into());
            }
            (
                BmpSuitability::Excellent,
                format!(
                    "HSG {} soils are well suited to wet detention; infiltration rate ({:.2} in/hr) is not limiting.",
                    soil.hydrologic_soil_group, soil.infiltration_rate_in_per_hr
                ),
                alts,
            )
        }
        "constructed-wetland" => {
            let mut alts = Vec::new();
            let (suit, rat) = if matches!(soil.hydrologic_soil_group, 'C' | 'D') {
                (
                    BmpSuitability::Excellent,
                    format!(
                        "Poorly drained HSG {} ({}) — wetland treatment matches site hydrology.",
                        soil.hydrologic_soil_group, soil.drainage
                    ),
                )
            } else {
                if matches!(soil.hydrologic_soil_group, 'A' | 'B') {
                    alts.push(bmp_type::BIORETENTION.into());
                }
                (
                    BmpSuitability::Good,
                    format!(
                        "HSG {} supports wetland BMP; verify permanent pool hydraulics.",
                        soil.hydrologic_soil_group
                    ),
                )
            };
            (suit, rat, alts)
        }
        bmp_type::SAND_FILTER => {
            let suit = if matches!(soil.hydrologic_soil_group, 'A' | 'B') {
                BmpSuitability::Good
            } else {
                BmpSuitability::Marginal
            };
            (
                suit,
                format!(
                    "Sand filter tolerates HSG {}; verify underdrain on low-K soils.",
                    soil.hydrologic_soil_group
                ),
                Vec::new(),
            )
        }
        _ => (
            BmpSuitability::Marginal,
            format!(
                "No specific guidance for BMP '{bmp_type}'; defaulting to HSG {} screening.",
                soil.hydrologic_soil_group
            ),
            Vec::new(),
        ),
    };

    BmpSuggestionResult {
        soil: soil.clone(),
        bmp_type: bmp,
        suitability,
        rationale,
        alternatives,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_cecil_fuzzy() {
        let soil = lookup("Cecil sandy loam").unwrap();
        assert_eq!(soil.hydrologic_soil_group, 'B');
    }

    #[test]
    fn suggest_bioretention_hsg_d() {
        let soil = lookup("generic-clay").unwrap();
        let s = suggest_bmp(&soil, "bioretention");
        assert_eq!(s.suitability, BmpSuitability::NotRecommended);
    }
}