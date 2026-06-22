//! Full-network analysis orchestrator (mirrors `NetworkAnalysisPipeline.cs`).

use std::collections::HashMap;

use stormsewer::design::{design_review, DesignFinding, ReviewCriteria};
use stormsewer::idf::IdfCurve;
use stormsewer::network::{Analysis, Network};

use crate::catchment_flow_router::{self, CatchmentAssignmentMethod, CatchmentFlowRouterResult};
use crate::compliance::{
    self, BmpEfficiencyInput, ComplianceAnalysisResults, ComplianceReport,
    HydrologyComplianceInput, SedimentComplianceInput, WaterQualityComplianceInput,
    WatershedSedimentInput,
};
use crate::manning::{self, CapacityResult, NormalDepthResult};
use crate::models::{Catchment, NetworkAnalysisPipe, NetworkPipeLink};
use crate::rational::{self, PeakFlowResult};
use crate::scs_runoff::{self, CatchmentRunoffResult};
use crate::sediment::{self, RusleResult};
use crate::state_compliance::{self, StateComplianceConfig};
use crate::trace::{CalcStep, TracedResult};
use crate::water_quality::{
    self, TreatmentTrainResult, WqvResult, POLLUTANT_TN, POLLUTANT_TP, POLLUTANT_TSS,
};

#[derive(Debug, Clone)]
pub struct CatchmentHydrologyResult {
    pub catchment: Catchment,
    pub rational: PeakFlowResult,
    pub scs: CatchmentRunoffResult,
}

#[derive(Debug, Clone)]
pub struct PipeCapacityAnalysisResult {
    pub pipe: NetworkAnalysisPipe,
    pub design_flow_cfs: f64,
    pub capacity: CapacityResult,
    pub normal_depth: NormalDepthResult,
}

impl PipeCapacityAnalysisResult {
    pub fn flow_ratio(&self) -> f64 {
        if self.capacity.full_flow_cfs > 0.0 {
            self.design_flow_cfs / self.capacity.full_flow_cfs
        } else {
            0.0
        }
    }

    pub fn surcharged(&self) -> bool {
        self.normal_depth.surcharged
    }
}

#[derive(Debug, Clone)]
pub struct NetworkAnalysisInput {
    pub catchments: Vec<Catchment>,
    pub pipes: Vec<NetworkAnalysisPipe>,
    pub state_code: String,
    pub development_type: String,
    pub idf: IdfCurve,
    pub scs_design_rainfall_inches: f64,
    pub structure_id_to_name: Option<HashMap<String, String>>,
    pub storm_network: Option<Network>,
    pub storm_analysis: Option<Analysis>,
    pub review_criteria: Option<ReviewCriteria>,
    pub placeholder_bmp_chain: Option<Vec<String>>,
    pub land_use: String,
    pub rusle_slope_percent: f64,
    pub rusle_slope_length_ft: f64,
}

impl Default for NetworkAnalysisInput {
    fn default() -> Self {
        Self {
            catchments: Vec::new(),
            pipes: Vec::new(),
            state_code: "NC".into(),
            development_type: "residential".into(),
            idf: IdfCurve::new(100.0, 10.0, 0.8),
            scs_design_rainfall_inches: 0.0,
            structure_id_to_name: None,
            storm_network: None,
            storm_analysis: None,
            review_criteria: None,
            placeholder_bmp_chain: None,
            land_use: "commercial".into(),
            rusle_slope_percent: 5.0,
            rusle_slope_length_ft: 300.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkAnalysisResult {
    pub state_code: String,
    pub overall_pass: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub hydrology: Vec<CatchmentHydrologyResult>,
    pub routing: Option<CatchmentFlowRouterResult>,
    pub capacity: Vec<PipeCapacityAnalysisResult>,
    pub sediment: Vec<RusleResult>,
    pub wqv: Option<WqvResult>,
    pub treatment_train: Option<TreatmentTrainResult>,
    pub compliance: Option<ComplianceReport>,
    pub design_review: Vec<DesignFinding>,
    pub trace: TracedResult,
}

const DEFAULT_BMP_CHAIN: &[&str] = &["bioretention"];

pub fn run(input: &NetworkAnalysisInput) -> Result<NetworkAnalysisResult, String> {
    if input.catchments.is_empty() {
        return Err("At least one catchment is required.".into());
    }

    let state = state_compliance::get(&input.state_code);
    let mut result = NetworkAnalysisResult {
        state_code: state.code.to_string(),
        overall_pass: false,
        warnings: Vec::new(),
        errors: Vec::new(),
        hydrology: Vec::new(),
        routing: None,
        capacity: Vec::new(),
        sediment: Vec::new(),
        wqv: None,
        treatment_train: None,
        compliance: None,
        design_review: Vec::new(),
        trace: TracedResult {
            steps: vec![
                CalcStep::new("state", 0.0, "", format!("{} — {}", state.code, state.name)),
                CalcStep::new("catchments", input.catchments.len() as f64, "", "drainage areas"),
                CalcStep::new("pipes", input.pipes.len() as f64, "", "network links"),
            ],
        },
    };

    let scs_rainfall = if input.scs_design_rainfall_inches > 0.0 {
        input.scs_design_rainfall_inches
    } else {
        state.design_storm_inches
    };

    for cm in &input.catchments {
        let rat = rational::peak(cm, &input.idf);
        let scs = scs_runoff::catchment_runoff(cm, scs_rainfall);
        result.hydrology.push(CatchmentHydrologyResult {
            catchment: cm.clone(),
            rational: rat,
            scs,
        });
    }

    let total_peak: f64 = result.hydrology.iter().map(|h| h.rational.peak_flow_cfs).sum();
    result.trace.steps.push(CalcStep::new(
        "Q_total",
        total_peak,
        "cfs",
        "sum of Rational peaks",
    ));

    let links: Vec<NetworkPipeLink> = input.pipes.iter().map(|p| p.link.clone()).collect();
    result.routing = Some(catchment_flow_router::route(
        &input.catchments,
        &links,
        &input.idf,
        input.structure_id_to_name.as_ref(),
        None,
    ));

    if let Some(routing) = &result.routing {
        for pipe in &input.pipes {
            let design_q = routing
                .pipe_flow_cfs
                .get(&pipe.pipe_key)
                .copied()
                .unwrap_or(0.0);
            if design_q <= 0.0 {
                continue;
            }
            match (manning::capacity(&pipe.segment), manning::normal_depth(&pipe.segment, design_q))
            {
                (cap, nd) => {
                    result.capacity.push(PipeCapacityAnalysisResult {
                        pipe: pipe.clone(),
                        design_flow_cfs: design_q,
                        capacity: cap,
                        normal_depth: nd,
                    });
                }
            }
        }
    }

    let surcharged_count = result.capacity.iter().filter(|c| c.surcharged()).count();
    if surcharged_count > 0 {
        result.warnings.push(format!(
            "{surcharged_count} pipe(s) surcharged at routed design Q."
        ));
    }

    for hydro in &result.hydrology {
        let rusle = sediment::rusle(
            hydro.catchment.area_acres,
            input.rusle_slope_percent,
            input.rusle_slope_length_ft,
            hydro.catchment.runoff_c,
            state.default_r_factor,
            0.32,
            1.0,
            &hydro.catchment.name,
        );
        result.sediment.push(rusle);
    }

    result.wqv = Some(water_quality::compute_wqv_from_catchments(
        &input.catchments,
        state.wq_volume_factor_inches,
    ));

    let wq_runoff_depth = if state.wq_volume_factor_inches > 0.0 {
        state.wq_volume_factor_inches * result.wqv.as_ref().unwrap().runoff_coefficient_rv
    } else {
        0.0
    };

    let bmp_chain: Vec<String> = input
        .placeholder_bmp_chain
        .clone()
        .unwrap_or_else(|| DEFAULT_BMP_CHAIN.iter().map(|s| s.to_string()).collect());
    let initial_loads = build_placeholder_loads(&input.catchments, &input.land_use, wq_runoff_depth);
    if initial_loads.values().any(|&v| v > 0.0) {
        result.treatment_train = Some(water_quality::apply_treatment_train(
            &initial_loads,
            &bmp_chain,
        ));
    }

    result.compliance = Some(compliance::check_compliance(
        &build_compliance_input(&result, &state, &input.development_type),
        &state.code,
        &input.development_type,
    ));

    if let Some(ref comp) = result.compliance {
        if !comp.overall_pass {
            result
                .warnings
                .push(format!("Regulatory compliance check FAILED for {}.", state.code));
        }
    }

    if let (Some(net), Some(analysis)) = (&input.storm_network, &input.storm_analysis) {
        let criteria = input.review_criteria.clone().unwrap_or_default();
        result.design_review = design_review(net, analysis, &criteria);
    }

    let design_errors = result
        .design_review
        .iter()
        .filter(|f| f.severity == stormsewer::design::Severity::Error)
        .count();
    if design_errors > 0 {
        result
            .warnings
            .push(format!("{design_errors} design-criteria error(s) found."));
    }

    result.overall_pass = result.compliance.as_ref().map(|c| c.overall_pass).unwrap_or(false)
        && design_errors == 0
        && surcharged_count == 0;

    result.trace.steps.push(CalcStep::new(
        "overall_pass",
        if result.overall_pass { 1.0 } else { 0.0 },
        "-",
        "compliance && no design errors && no surcharge",
    ));

    Ok(result)
}

fn build_compliance_input(
    result: &NetworkAnalysisResult,
    _state: &StateComplianceConfig,
    _development_type: &str,
) -> ComplianceAnalysisResults {
    let mut input = ComplianceAnalysisResults::default();

    if let Some(wqv) = &result.wqv {
        let mut wq = WaterQualityComplianceInput {
            bmp_count: result.treatment_train.as_ref().map(|t| t.chain_length).unwrap_or(0),
            wqv_required_cf: Some(wqv.wqv_cf),
            wqv_provided_cf: Some(0.0),
            ..Default::default()
        };
        if let Some(train) = &result.treatment_train {
            let tss = train.overall_removal_efficiency.get(POLLUTANT_TSS).copied();
            let tn = train.overall_removal_efficiency.get(POLLUTANT_TN).copied();
            let tp = train.overall_removal_efficiency.get(POLLUTANT_TP).copied();
            wq.bmp_efficiency.push(BmpEfficiencyInput {
                bmp_type: "bioretention".into(),
                tss_removal_percent: tss.unwrap_or(0.0) * 100.0,
                tn_removal_percent: tn.unwrap_or(0.0) * 100.0,
                tp_removal_percent: tp.unwrap_or(0.0) * 100.0,
                drawdown_hours: None,
            });
        }
        input.water_quality = Some(wq);
    }

    if !result.sediment.is_empty() {
        input.sediment = Some(SedimentComplianceInput {
            total_soil_loss_tons_per_ac_yr: sediment::weighted_average_soil_loss(&result.sediment),
            sediment_control_count: 0,
            watershed_results: result
                .sediment
                .iter()
                .map(|r| WatershedSedimentInput {
                    name: r.name.clone(),
                    risk_level: r.risk_level.clone(),
                })
                .collect(),
        });
    }

    let post_peak = result.routing.as_ref().map(|r| r.total_peak_cfs).unwrap_or(0.0);
    if post_peak > 0.0 {
        input.hydrology = Some(HydrologyComplianceInput {
            has_detention: false,
            pre_peak_cfs: Some(post_peak * 0.8),
            post_peak_cfs: Some(post_peak),
        });
    }

    input
}

fn build_placeholder_loads(
    catchments: &[Catchment],
    land_use: &str,
    runoff_depth_in: f64,
) -> HashMap<String, f64> {
    let mut totals = HashMap::new();
    for p in [POLLUTANT_TSS, POLLUTANT_TN, POLLUTANT_TP] {
        totals.insert(p.to_string(), 0.0);
    }
    if runoff_depth_in <= 0.0 {
        return totals;
    }
    for cm in catchments {
        for pollutant in [POLLUTANT_TSS, POLLUTANT_TN, POLLUTANT_TP] {
            let load = water_quality::calculate_emc_load(pollutant, land_use, runoff_depth_in, cm.area_acres);
            *totals.get_mut(pollutant).unwrap() += load.emc_load_lbs;
        }
    }
    totals
}

pub fn describe_assignment(method: CatchmentAssignmentMethod) -> &'static str {
    match method {
        CatchmentAssignmentMethod::OutletStructure => "outlet structures",
        CatchmentAssignmentMethod::AreaWeightedHeadwater => "area-weighted headwaters",
        CatchmentAssignmentMethod::UniformFallback => "uniform fallback",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NetworkPipeLink, PipeSegment, PipeShape};

    fn sample_catchment() -> Catchment {
        Catchment {
            name: "C1".into(),
            area_acres: 1.0,
            runoff_c: 0.7,
            curve_number: 70.0,
            tc_minutes: 10.0,
            outfall_structure_id: Some("S1".into()),
            outfall_structure_name: None,
        }
    }

    fn sample_pipe() -> NetworkAnalysisPipe {
        let link = NetworkPipeLink {
            pipe_key: "P1".into(),
            network_name: "NET".into(),
            pipe_name: "P1".into(),
            upstream_structure_id: "S1".into(),
            downstream_structure_id: "OUT".into(),
        };
        NetworkAnalysisPipe {
            pipe_key: "P1".into(),
            network_name: "NET".into(),
            pipe_name: "P1".into(),
            link: link.clone(),
            segment: PipeSegment::circular("P1", 1.5, 0.005, 0.013),
            upstream_node_id: "S1".into(),
            downstream_node_id: "OUT".into(),
            length_ft: 200.0,
            upstream_invert_ft: 100.0,
            downstream_invert_ft: 99.0,
        }
    }

    #[test]
    fn pipeline_requires_catchment() {
        let input = NetworkAnalysisInput {
            catchments: vec![],
            pipes: vec![],
            state_code: "NC".into(),
            development_type: "residential".into(),
            idf: IdfCurve::new(100.0, 10.0, 0.8),
            ..Default::default()
        };
        assert!(run(&input).is_err());
    }

    #[test]
    fn pipeline_runs_with_catchment_and_pipe() {
        let input = NetworkAnalysisInput {
            catchments: vec![sample_catchment()],
            pipes: vec![sample_pipe()],
            state_code: "NC".into(),
            development_type: "residential".into(),
            idf: IdfCurve::new(100.0, 10.0, 0.8),
            ..Default::default()
        };
        let result = run(&input).expect("pipeline runs");
        assert_eq!(result.hydrology.len(), 1);
        assert!(result.routing.is_some());
        assert!(result.wqv.is_some());
        assert!(result.compliance.is_some());
        assert_eq!(result.sediment.len(), 1);
    }
}