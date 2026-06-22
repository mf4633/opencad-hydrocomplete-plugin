//! State regulatory compliance checking (mirrors `ComplianceChecker.cs`).

use crate::state_compliance::{self, StateComplianceConfig};
use crate::trace::{CalcStep, TracedResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplianceStatus {
    Pass,
    Fail,
    Review,
    Incomplete,
    Info,
}

impl std::fmt::Display for ComplianceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "Pass"),
            Self::Fail => write!(f, "Fail"),
            Self::Review => write!(f, "Review"),
            Self::Incomplete => write!(f, "Incomplete"),
            Self::Info => write!(f, "Info"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComplianceCriterion {
    pub name: String,
    pub category: String,
    pub required: String,
    pub actual: String,
    pub status: ComplianceStatus,
    pub authority: String,
    pub notes: String,
    pub steps: Vec<CalcStep>,
}

#[derive(Debug, Clone)]
pub struct ComplianceReport {
    pub state: String,
    pub regulatory_body: String,
    pub development_type: String,
    pub overall_pass: bool,
    pub criteria: Vec<ComplianceCriterion>,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
    pub error: Option<String>,
    pub trace: TracedResult,
}

#[derive(Debug, Clone, Default)]
pub struct BmpEfficiencyInput {
    pub bmp_type: String,
    pub tss_removal_percent: f64,
    pub tn_removal_percent: f64,
    pub tp_removal_percent: f64,
    pub drawdown_hours: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct WaterQualityComplianceInput {
    pub bmp_count: usize,
    pub bmp_efficiency: Vec<BmpEfficiencyInput>,
    pub wqv_provided_cf: Option<f64>,
    pub wqv_required_cf: Option<f64>,
    pub drawdown_hours: Option<f64>,
    pub has_infiltration_bmp: bool,
}

#[derive(Debug, Clone, Default)]
pub struct HydrologyComplianceInput {
    pub has_detention: bool,
    pub pre_peak_cfs: Option<f64>,
    pub post_peak_cfs: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct WatershedSedimentInput {
    pub name: String,
    pub risk_level: String,
}

#[derive(Debug, Clone, Default)]
pub struct SedimentComplianceInput {
    pub total_soil_loss_tons_per_ac_yr: f64,
    pub sediment_control_count: usize,
    pub watershed_results: Vec<WatershedSedimentInput>,
}

#[derive(Debug, Clone, Default)]
pub struct ComplianceAnalysisResults {
    pub water_quality: Option<WaterQualityComplianceInput>,
    pub hydrology: Option<HydrologyComplianceInput>,
    pub sediment: Option<SedimentComplianceInput>,
}

const INFILTRATION_TYPES: &[&str] = &[
    "bioretention", "permeable", "infiltration", "permeable-pavement", "dry-well",
];
const POND_TYPES: &[&str] = &["wet-pond", "pond", "dry-pond", "detention"];

fn contains_type(types: &[&str], value: &str) -> bool {
    types.iter().any(|t| t.eq_ignore_ascii_case(value))
}

/// Treatment-train sequential removal: 1 - product(1 - eff_i/100).
pub fn combined_removal_percent(efficiencies: &[f64]) -> Option<f64> {
    let mut product = 1.0;
    let mut any = false;
    for eff in efficiencies {
        if eff.is_nan() {
            continue;
        }
        any = true;
        product *= 1.0 - eff / 100.0;
    }
    if any {
        Some((1.0 - product) * 100.0)
    } else {
        None
    }
}

pub fn check_compliance(
    results: &ComplianceAnalysisResults,
    state_code: &str,
    development_type: &str,
) -> ComplianceReport {
    let config = state_compliance::get(state_code);
    let dev = if development_type.trim().is_empty() {
        "residential"
    } else {
        development_type
    };
    let mut report = ComplianceReport {
        state: config.name.to_string(),
        regulatory_body: config.regulatory_body.to_string(),
        development_type: dev.to_string(),
        overall_pass: true,
        criteria: Vec::new(),
        warnings: Vec::new(),
        recommendations: Vec::new(),
        error: None,
        trace: TracedResult {
            steps: vec![CalcStep::new(
                "state",
                0.0,
                "",
                format!("{} — {}", config.code, config.name),
            )],
        },
    };

    check_tss_removal(&mut report, results, &config, dev);
    check_nutrient_removal(&mut report, results, &config, "TN", config.tn_removal_percent);
    check_nutrient_removal(&mut report, results, &config, "TP", config.tp_removal_percent);
    check_volume_control(&mut report, results, &config);
    check_peak_flow_control(&mut report, results, &config);
    check_erosion_control(&mut report, results, &config);
    check_drawdown_times(&mut report, results, &config);

    report.overall_pass = report.criteria.iter().all(|c| {
        c.status == ComplianceStatus::Pass || c.status == ComplianceStatus::Info
    });
    if !report.overall_pass {
        generate_recommendations(&mut report);
    }
    report
}

fn check_tss_removal(
    report: &mut ComplianceReport,
    results: &ComplianceAnalysisResults,
    config: &StateComplianceConfig,
    development_type: &str,
) {
    let required = state_compliance::required_tss_percent(config, development_type);
    let actual = results.water_quality.as_ref().and_then(|wq| {
        if wq.bmp_efficiency.is_empty() {
            None
        } else {
            combined_removal_percent(
                &wq.bmp_efficiency
                    .iter()
                    .map(|b| b.tss_removal_percent)
                    .collect::<Vec<_>>(),
            )
        }
    });

    let mut criterion = ComplianceCriterion {
        name: "TSS Removal".into(),
        category: "Water Quality".into(),
        required: format!("{required:.1}%"),
        actual: actual
            .map(|v| format!("{v:.1}%"))
            .unwrap_or_else(|| "Not calculated".into()),
        status: match actual {
            None => ComplianceStatus::Incomplete,
            Some(v) if v >= required => ComplianceStatus::Pass,
            Some(_) => ComplianceStatus::Fail,
        },
        authority: config.regulatory_body.to_string(),
        notes: format!("{required:.1}% TSS removal required for {development_type} development"),
        steps: Vec::new(),
    };
    if let Some(v) = actual {
        criterion.steps.push(CalcStep::new("required_TSS", required, "%", "state threshold"));
        criterion
            .steps
            .push(CalcStep::new("actual_TSS", v, "%", "1 - product(1 - BMP efficiency)"));
    }
    report.criteria.push(criterion);
}

fn check_nutrient_removal(
    report: &mut ComplianceReport,
    results: &ComplianceAnalysisResults,
    config: &StateComplianceConfig,
    pollutant: &str,
    required: f64,
) {
    if required <= 0.0 {
        return;
    }
    let actual = results.water_quality.as_ref().and_then(|wq| {
        if wq.bmp_efficiency.is_empty() {
            None
        } else {
            let values: Vec<f64> = wq
                .bmp_efficiency
                .iter()
                .map(|b| {
                    if pollutant == "TN" {
                        b.tn_removal_percent
                    } else {
                        b.tp_removal_percent
                    }
                })
                .collect();
            combined_removal_percent(&values)
        }
    });
    report.criteria.push(ComplianceCriterion {
        name: format!("{pollutant} Removal"),
        category: "Water Quality".into(),
        required: format!("{required:.1}%"),
        actual: actual
            .map(|v| format!("{v:.1}%"))
            .unwrap_or_else(|| "Not calculated".into()),
        status: match actual {
            None => ComplianceStatus::Incomplete,
            Some(v) if v >= required => ComplianceStatus::Pass,
            Some(_) => ComplianceStatus::Fail,
        },
        authority: config.regulatory_body.to_string(),
        notes: format!("{pollutant} removal target for post-construction stormwater management"),
        steps: Vec::new(),
    });
}

fn check_volume_control(
    report: &mut ComplianceReport,
    results: &ComplianceAnalysisResults,
    config: &StateComplianceConfig,
) {
    if !config.volume_control_required && results.water_quality.as_ref().and_then(|w| w.wqv_required_cf).is_none() {
        return;
    }
    let Some(wq) = &results.water_quality else {
        return;
    };
    let has_bmps = wq.bmp_count > 0;
    let has_infiltration = wq.has_infiltration_bmp
        || wq.bmp_efficiency.iter().any(|b| contains_type(INFILTRATION_TYPES, &b.bmp_type));

    if let (Some(required), Some(provided)) = (wq.wqv_required_cf, wq.wqv_provided_cf) {
        if required > 0.0 {
            let pass = provided >= required - 1e-6;
            let mut criterion = ComplianceCriterion {
                name: "Volume Control (WQV)".into(),
                category: "Volume Management".into(),
                required: format!(
                    "{:.2}\" storm ({required:.0} cf)",
                    config.wq_volume_factor_inches
                ),
                actual: format!("{provided:.0} cf provided / {required:.0} cf required"),
                status: if pass {
                    ComplianceStatus::Pass
                } else {
                    ComplianceStatus::Fail
                },
                authority: config.regulatory_body.to_string(),
                notes: format!(
                    "First {:.2} inch runoff capture",
                    config.wq_volume_factor_inches
                ),
                steps: vec![
                    CalcStep::new("WQV_required", required, "cf", "Rv * storm * area"),
                    CalcStep::new("WQV_provided", provided, "cf", "BMP storage"),
                ],
            };
            report.criteria.push(criterion);
            return;
        }
    }

    let (status, actual) = if has_infiltration {
        (ComplianceStatus::Pass, "Infiltration BMPs present")
    } else if has_bmps {
        (ComplianceStatus::Review, "BMPs present (verify infiltration capacity)")
    } else {
        (ComplianceStatus::Fail, "No volume reduction BMPs")
    };
    report.criteria.push(ComplianceCriterion {
        name: "Volume Control (WQ Storm)".into(),
        category: "Volume Management".into(),
        required: format!("First {:.2}\" of rainfall", config.wq_volume_factor_inches),
        actual: actual.into(),
        status,
        authority: config.regulatory_body.to_string(),
        notes: if config.volume_control_required {
            "Volume reduction required statewide".into()
        } else {
            "Verify local volume requirements".into()
        },
        steps: Vec::new(),
    });
}

fn check_peak_flow_control(
    report: &mut ComplianceReport,
    results: &ComplianceAnalysisResults,
    config: &StateComplianceConfig,
) {
    let hydro = results.hydrology.as_ref();
    let has_detention = hydro.map(|h| h.has_detention).unwrap_or(false);
    let pre = hydro.and_then(|h| h.pre_peak_cfs);
    let post = hydro.and_then(|h| h.post_peak_cfs);

    if let (Some(pre), Some(post)) = (pre, post) {
        if pre > 0.0 {
            let attenuation = ((1.0 - post / pre) * 100.0).max(0.0);
            let pass = post <= pre + 1e-6;
            let mut criterion = ComplianceCriterion {
                name: "Peak Flow Attenuation".into(),
                category: "Flow Attenuation".into(),
                required: format!(
                    "<= pre-dev peak ({:.1}% match)",
                    config.peak_attenuation_percent
                ),
                actual: format!(
                    "Post {post:.2} cfs vs pre {pre:.2} cfs ({attenuation:.1}% reduction)"
                ),
                status: if pass {
                    ComplianceStatus::Pass
                } else {
                    ComplianceStatus::Fail
                },
                authority: config.regulatory_body.to_string(),
                notes: "Post-development peak shall not exceed pre-development".into(),
                steps: vec![
                    CalcStep::new("Q_pre", pre, "cfs", "pre-development peak"),
                    CalcStep::new("Q_post", post, "cfs", "post-development peak"),
                    CalcStep::new("attenuation", attenuation, "%", "(1 - Qpost/Qpre)*100"),
                ],
            };
            report.criteria.push(criterion);
            return;
        }
    }

    report.criteria.push(ComplianceCriterion {
        name: "Peak Flow Control".into(),
        category: "Flow Attenuation".into(),
        required: "Match pre-development peak".into(),
        actual: if has_detention {
            "Detention provided (verify peaks)".into()
        } else {
            "No detention facility".into()
        },
        status: if has_detention {
            ComplianceStatus::Review
        } else {
            ComplianceStatus::Fail
        },
        authority: config.regulatory_body.to_string(),
        notes: "Detention required for required design storms".into(),
        steps: Vec::new(),
    });
}

fn check_erosion_control(
    report: &mut ComplianceReport,
    results: &ComplianceAnalysisResults,
    config: &StateComplianceConfig,
) {
    let Some(sediment) = &results.sediment else {
        return;
    };
    let tolerable = config.tolerable_soil_loss_tons_per_ac_yr;
    let loss = sediment.total_soil_loss_tons_per_ac_yr;
    let pass = loss <= tolerable;
    let mut soil = ComplianceCriterion {
        name: "Soil Loss (Tolerable T)".into(),
        category: "Erosion Control".into(),
        required: format!("< {tolerable:.1} tons/ac/yr"),
        actual: format!("{loss:.2} tons/ac/yr"),
        status: if pass {
            ComplianceStatus::Pass
        } else {
            ComplianceStatus::Fail
        },
        authority: "USDA NRCS Soil Conservation Standards".into(),
        notes: if pass {
            "Soil loss within tolerable limits".into()
        } else {
            "Soil loss exceeds tolerable limit. Additional erosion controls required.".into()
        },
        steps: vec![
            CalcStep::new("soil_loss", loss, "tons/ac/yr", "RUSLE/MUSLE"),
            CalcStep::new("tolerable_T", tolerable, "tons/ac/yr", "state/USDA default"),
        ],
    };
    report.criteria.push(soil);

    let high_risk = sediment
        .watershed_results
        .iter()
        .any(|w| w.risk_level.eq_ignore_ascii_case("High"));
    if sediment.sediment_control_count == 0 && high_risk {
        report.criteria.push(ComplianceCriterion {
            name: "Construction Sediment Controls".into(),
            category: "Erosion Control".into(),
            required: "Sediment basins for high-risk areas".into(),
            actual: "No sediment control structures in model".into(),
            status: ComplianceStatus::Fail,
            authority: config.regulatory_body.to_string(),
            notes: "High-risk erosion areas require sediment basins or equivalent controls".into(),
            steps: Vec::new(),
        });
    }
}

fn check_drawdown_times(
    report: &mut ComplianceReport,
    results: &ComplianceAnalysisResults,
    config: &StateComplianceConfig,
) {
    let Some(wq) = &results.water_quality else {
        return;
    };
    if let Some(drawdown) = wq.drawdown_hours {
        let pass = drawdown >= config.drawdown_min_hours && drawdown <= config.drawdown_max_hours;
        report.criteria.push(ComplianceCriterion {
            name: "BMP Drawdown Time".into(),
            category: "Water Quality".into(),
            required: format!(
                "{:.1}-{:.1} hours",
                config.drawdown_min_hours, config.drawdown_max_hours
            ),
            actual: format!("{drawdown:.1} hours"),
            status: if pass {
                ComplianceStatus::Pass
            } else {
                ComplianceStatus::Fail
            },
            authority: config.regulatory_body.to_string(),
            notes: "Water quality volume drawdown within regulatory window".into(),
            steps: vec![CalcStep::new("drawdown", drawdown, "hr", "BMP drain time")],
        });
        return;
    }

    let has_ponds = wq.bmp_efficiency.iter().any(|b| contains_type(POND_TYPES, &b.bmp_type));
    if has_ponds {
        report.criteria.push(ComplianceCriterion {
            name: "BMP Drawdown Time".into(),
            category: "Water Quality".into(),
            required: format!(
                "{:.1}-{:.1} hours",
                config.drawdown_min_hours, config.drawdown_max_hours
            ),
            actual: "Detention facilities present (verify drawdown)".into(),
            status: ComplianceStatus::Review,
            authority: config.regulatory_body.to_string(),
            notes: "Verify orifice sizing for WQV drawdown".into(),
            steps: Vec::new(),
        });
    }
}

fn generate_recommendations(report: &mut ComplianceReport) {
    for criterion in &report.criteria {
        if criterion.status != ComplianceStatus::Fail {
            continue;
        }
        match criterion.category.as_str() {
            "Water Quality" => {
                if criterion.name.to_ascii_uppercase().contains("TSS") {
                    report.recommendations.push(
                        "Consider adding or upsizing bioretention cells for improved TSS removal."
                            .into(),
                    );
                    report.recommendations.push(
                        "A treatment train (e.g., swale + bioretention) can achieve higher combined removal."
                            .into(),
                    );
                }
                if criterion.name.contains("TN") || criterion.name.contains("TP") {
                    report.recommendations.push(
                        "Constructed wetlands and bioretention with IWS provide the highest nutrient removal."
                            .into(),
                    );
                }
                if criterion.name.to_ascii_uppercase().contains("DRAWDOWN") {
                    report.recommendations.push(
                        "Resize outlet orifice to meet drawdown time for the water quality volume."
                            .into(),
                    );
                }
            }
            "Volume Management" => report.recommendations.push(
                "Add infiltration-based BMPs (bioretention, permeable pavement) to meet volume reduction."
                    .into(),
            ),
            "Flow Attenuation" => report.recommendations.push(
                "Add detention facilities to attenuate peak flows to pre-development levels.".into(),
            ),
            "Erosion Control" => {
                report
                    .recommendations
                    .push("Implement temporary sediment basins during construction phase.".into());
                report.recommendations.push(
                    "Consider phased grading to minimize exposed area at any given time.".into(),
                );
            }
            _ => {}
        }
    }
    report.recommendations.sort();
    report.recommendations.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passing_wq(tss: f64, tn: f64, tp: f64) -> ComplianceAnalysisResults {
        ComplianceAnalysisResults {
            water_quality: Some(WaterQualityComplianceInput {
                bmp_count: 1,
                has_infiltration_bmp: true,
                wqv_required_cf: Some(5000.0),
                wqv_provided_cf: Some(5500.0),
                drawdown_hours: Some(72.0),
                bmp_efficiency: vec![BmpEfficiencyInput {
                    bmp_type: "bioretention".into(),
                    tss_removal_percent: tss,
                    tn_removal_percent: tn,
                    tp_removal_percent: tp,
                    drawdown_hours: None,
                }],
            }),
            hydrology: Some(HydrologyComplianceInput {
                has_detention: true,
                pre_peak_cfs: Some(10.0),
                post_peak_cfs: Some(9.5),
            }),
            sediment: Some(SedimentComplianceInput {
                total_soil_loss_tons_per_ac_yr: 3.0,
                sediment_control_count: 1,
                watershed_results: vec![],
            }),
        }
    }

    fn find<'a>(report: &'a ComplianceReport, name: &str) -> Option<&'a ComplianceCriterion> {
        report.criteria.iter().find(|c| c.name == name)
    }

    #[test]
    fn nc_requires_85_percent_tss_fails_at_80() {
        let report = check_compliance(&passing_wq(80.0, 35.0, 35.0), "NC", "residential");
        assert_eq!(find(&report, "TSS Removal").unwrap().status, ComplianceStatus::Fail);
        assert!(!report.overall_pass);
    }

    #[test]
    fn nc_passes_at_85_percent_tss() {
        let report = check_compliance(&passing_wq(85.0, 35.0, 35.0), "NC", "residential");
        assert_eq!(find(&report, "TSS Removal").unwrap().status, ComplianceStatus::Pass);
    }

    #[test]
    fn combined_removal_uses_treatment_train_formula() {
        let combined = combined_removal_percent(&[50.0, 50.0]).unwrap();
        assert!((combined - 75.0).abs() < 1.0);
    }

    #[test]
    fn peak_flow_fails_when_post_exceeds_pre() {
        let mut r = passing_wq(85.0, 35.0, 35.0);
        r.hydrology.as_mut().unwrap().post_peak_cfs = Some(12.0);
        r.hydrology.as_mut().unwrap().pre_peak_cfs = Some(10.0);
        let report = check_compliance(&r, "NC", "residential");
        assert_eq!(
            find(&report, "Peak Flow Attenuation").unwrap().status,
            ComplianceStatus::Fail
        );
    }

    #[test]
    fn va_requires_50_percent_tp() {
        let report = check_compliance(&passing_wq(90.0, 35.0, 45.0), "VA", "residential");
        assert_eq!(find(&report, "TP Removal").unwrap().status, ComplianceStatus::Fail);
    }
}