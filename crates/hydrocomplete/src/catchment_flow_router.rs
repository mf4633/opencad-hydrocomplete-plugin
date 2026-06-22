//! Routes per-catchment Rational peak flows through pipe topology.

use std::collections::{HashMap, HashSet, VecDeque};

use stormsewer::idf::IdfCurve;

use crate::models::{Catchment, NetworkPipeLink};
use crate::rational;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatchmentAssignmentMethod {
    OutletStructure,
    AreaWeightedHeadwater,
    UniformFallback,
}

#[derive(Debug, Clone)]
pub struct RoutedCatchmentFlow {
    pub catchment: Catchment,
    pub peak_flow_cfs: f64,
    pub assigned_structure_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CatchmentFlowRouterResult {
    pub assignment_method: CatchmentAssignmentMethod,
    pub pipe_flow_cfs: HashMap<String, f64>,
    pub structure_inflow_cfs: HashMap<String, f64>,
    pub catchment_flows: Vec<RoutedCatchmentFlow>,
    pub uniform_fallback_cfs: Option<f64>,
    pub total_peak_cfs: f64,
}

impl Default for CatchmentFlowRouterResult {
    fn default() -> Self {
        Self {
            assignment_method: CatchmentAssignmentMethod::UniformFallback,
            pipe_flow_cfs: HashMap::new(),
            structure_inflow_cfs: HashMap::new(),
            catchment_flows: Vec::new(),
            uniform_fallback_cfs: None,
            total_peak_cfs: 0.0,
        }
    }
}

fn ci_key(s: &str) -> String {
    s.to_ascii_uppercase()
}

fn map_get<'a>(map: &'a HashMap<String, f64>, key: &str) -> Option<f64> {
    map.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| *v)
}

fn map_add_inflow(map: &mut HashMap<String, f64>, key: &str, add: f64) {
    let upper = ci_key(key);
    if let Some(v) = map.get_mut(&upper) {
        *v += add;
    } else {
        map.insert(upper, add);
    }
}

fn map_insert_add(map: &mut HashMap<String, f64>, key: &str, flow: f64) {
    let upper = ci_key(key);
    if let Some(v) = map.get_mut(&upper) {
        *v += flow;
    } else {
        map.insert(upper, flow);
    }
}

/// Route catchments through pipe topology using per-catchment IDF intensity.
pub fn route(
    catchments: &[Catchment],
    pipes: &[NetworkPipeLink],
    idf: &IdfCurve,
    structure_id_to_name: Option<&HashMap<String, String>>,
    uniform_fallback_cfs: Option<f64>,
) -> CatchmentFlowRouterResult {
    assert!(!catchments.is_empty(), "at least one catchment is required");
    let links = normalize_pipes(pipes);
    let mut result = CatchmentFlowRouterResult::default();
    let mut tributary: HashMap<String, f64> = HashMap::new();
    let structure_names = structure_names_from_pipes(&links, structure_id_to_name);

    for cm in catchments {
        let peak = rational::peak(cm, idf);
        result.catchment_flows.push(RoutedCatchmentFlow {
            catchment: cm.clone(),
            peak_flow_cfs: peak.peak_flow_cfs,
            assigned_structure_id: None,
        });
        result.total_peak_cfs += peak.peak_flow_cfs;
    }

    let mut known_structures = HashSet::new();
    for link in &links {
        known_structures.insert(ci_key(&link.upstream_structure_id));
        known_structures.insert(ci_key(&link.downstream_structure_id));
    }

    let assigned_count = assign_catchments_to_structures(
        &mut result.catchment_flows,
        &mut tributary,
        &structure_names,
        &known_structures,
    );

    let unassigned: Vec<usize> = result
        .catchment_flows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.assigned_structure_id.is_none())
        .map(|(i, _)| i)
        .collect();

    if !unassigned.is_empty() && !links.is_empty() {
        area_weight_unassigned_to_headwaters(&mut result.catchment_flows, &unassigned, &mut tributary, &links);
    }

    if links.is_empty() || tributary.is_empty() {
        let uniform = uniform_fallback_cfs.unwrap_or(result.total_peak_cfs);
        assert!(uniform > 0.0, "uniform fallback flow must be positive");
        result.assignment_method = CatchmentAssignmentMethod::UniformFallback;
        result.uniform_fallback_cfs = Some(uniform);
        for link in &links {
            result
                .pipe_flow_cfs
                .insert(link.pipe_key.clone(), uniform);
        }
        return result;
    }

    result.assignment_method = if assigned_count > 0 {
        CatchmentAssignmentMethod::OutletStructure
    } else {
        CatchmentAssignmentMethod::AreaWeightedHeadwater
    };

    let mut groups: HashMap<String, Vec<NetworkPipeLink>> = HashMap::new();
    for link in links {
        groups
            .entry(link.network_name.clone())
            .or_default()
            .push(link);
    }
    for (_, group) in groups {
        route_network(&group, &tributary, &mut result);
    }

    result
}

fn normalize_pipes(pipes: &[NetworkPipeLink]) -> Vec<NetworkPipeLink> {
    pipes
        .iter()
        .map(|p| NetworkPipeLink {
            pipe_key: p.pipe_key.clone(),
            network_name: p.network_name.clone(),
            pipe_name: p.pipe_name.clone(),
            upstream_structure_id: if p.upstream_structure_id.trim().is_empty() {
                format!("__src::{}", p.pipe_key)
            } else {
                p.upstream_structure_id.clone()
            },
            downstream_structure_id: if p.downstream_structure_id.trim().is_empty() {
                format!("__out::{}", p.pipe_key)
            } else {
                p.downstream_structure_id.clone()
            },
        })
        .collect()
}

fn assign_catchments_to_structures(
    catchment_flows: &mut [RoutedCatchmentFlow],
    tributary: &mut HashMap<String, f64>,
    structure_names: &HashMap<String, String>,
    known_structures: &HashSet<String>,
) -> usize {
    let mut assigned = 0;
    for routed in catchment_flows.iter_mut() {
        if let Some(struct_id) =
            resolve_structure_id(&routed.catchment, structure_names, known_structures)
        {
            routed.assigned_structure_id = Some(struct_id.clone());
            map_insert_add(tributary, &struct_id, routed.peak_flow_cfs);
            assigned += 1;
        }
    }
    assigned
}

fn resolve_structure_id(
    catchment: &Catchment,
    structure_names: &HashMap<String, String>,
    known_structures: &HashSet<String>,
) -> Option<String> {
    if let Some(ref outfall_id) = catchment.outfall_structure_id {
        let id = outfall_id.trim();
        if !id.is_empty() && known_structures.contains(&ci_key(id)) {
            return Some(id.to_string());
        }
    }
    let outfall_name = catchment.outfall_structure_name.as_deref()?;
    let target = outfall_name.trim();
    if target.is_empty() {
        return None;
    }
    for (struct_id, name) in structure_names {
        if name.eq_ignore_ascii_case(target) && known_structures.contains(&ci_key(struct_id)) {
            return Some(struct_id.clone());
        }
    }
    None
}

fn area_weight_unassigned_to_headwaters(
    catchment_flows: &mut [RoutedCatchmentFlow],
    unassigned: &[usize],
    tributary: &mut HashMap<String, f64>,
    pipes: &[NetworkPipeLink],
) {
    let mut groups: HashMap<String, Vec<NetworkPipeLink>> = HashMap::new();
    for link in pipes {
        groups
            .entry(link.network_name.clone())
            .or_default()
            .push(link.clone());
    }
    for (_, group) in groups {
        let headwaters = find_headwater_structures(&group);
        if headwaters.is_empty() {
            continue;
        }
        if headwaters.len() == 1 {
            let hw = &headwaters[0];
            let mut total_q = 0.0;
            for &idx in unassigned {
                total_q += catchment_flows[idx].peak_flow_cfs;
                catchment_flows[idx]
                    .assigned_structure_id
                    .get_or_insert_with(|| hw.clone());
            }
            map_insert_add(tributary, hw, total_q);
            continue;
        }
        let total_area: f64 = unassigned
            .iter()
            .map(|&i| catchment_flows[i].catchment.area_acres)
            .sum();
        if total_area <= 0.0 {
            let per_hw: f64 = unassigned
                .iter()
                .map(|&i| catchment_flows[i].peak_flow_cfs)
                .sum::<f64>()
                / headwaters.len() as f64;
            for hw in &headwaters {
                map_insert_add(tributary, hw, per_hw);
                for &idx in unassigned {
                    catchment_flows[idx]
                        .assigned_structure_id
                        .get_or_insert_with(|| hw.clone());
                }
            }
            continue;
        }
        for &idx in unassigned {
            let share = catchment_flows[idx].catchment.area_acres / total_area;
            let per_headwater = catchment_flows[idx].peak_flow_cfs * share;
            for hw in &headwaters {
                map_insert_add(tributary, hw, per_headwater / headwaters.len() as f64);
                catchment_flows[idx]
                    .assigned_structure_id
                    .get_or_insert_with(|| hw.clone());
            }
        }
    }
}

fn route_network(
    pipes: &[NetworkPipeLink],
    global_tributary: &HashMap<String, f64>,
    result: &mut CatchmentFlowRouterResult,
) {
    let mut by_upstream: HashMap<String, Vec<&NetworkPipeLink>> = HashMap::new();
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut inflow: HashMap<String, f64> = HashMap::new();

    let ensure = |in_degree: &mut HashMap<String, usize>, inflow: &mut HashMap<String, f64>, s: &str| {
        in_degree.entry(ci_key(s)).or_insert(0);
        inflow.entry(ci_key(s)).or_insert(0.0);
    };

    for link in pipes {
        ensure(&mut in_degree, &mut inflow, &link.upstream_structure_id);
        ensure(&mut in_degree, &mut inflow, &link.downstream_structure_id);
        by_upstream
            .entry(ci_key(&link.upstream_structure_id))
            .or_default()
            .push(link);
        *in_degree.entry(ci_key(&link.downstream_structure_id)).or_insert(0) += 1;
    }

    for (struct_id, flow) in global_tributary {
        map_add_inflow(&mut inflow, struct_id, *flow);
    }

    let mut ready_list: Vec<String> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(s, _)| s.clone())
        .collect();
    ready_list.sort();
    let mut ready: VecDeque<String> = ready_list.into();

    let mut visited_pipes = HashSet::new();

    while let Some(struct_id) = ready.pop_front() {
        let Some(outgoing) = by_upstream.get(&struct_id) else {
            continue;
        };
        let struct_inflow = *inflow.get(&struct_id).unwrap_or(&0.0);
        let mut sorted = outgoing.clone();
        sorted.sort_by(|a, b| {
            a.pipe_name
                .to_ascii_lowercase()
                .cmp(&b.pipe_name.to_ascii_lowercase())
        });

        for link in sorted {
            if !visited_pipes.insert(link.pipe_key.clone()) {
                continue;
            }
            result
                .pipe_flow_cfs
                .insert(link.pipe_key.clone(), struct_inflow);
            let ds = ci_key(&link.downstream_structure_id);
            if let Some(v) = inflow.get_mut(&ds) {
                *v += struct_inflow;
            }
            if let Some(deg) = in_degree.get_mut(&ds) {
                *deg -= 1;
                if *deg == 0 {
                    ready.push_back(ds);
                }
            }
        }
    }

    for link in pipes {
        if !result.pipe_flow_cfs.contains_key(&link.pipe_key) {
            let q = map_get(&inflow, &link.upstream_structure_id).unwrap_or(0.0);
            result.pipe_flow_cfs.insert(link.pipe_key.clone(), q);
        }
    }

    for (struct_id, flow) in &inflow {
        result
            .structure_inflow_cfs
            .entry(struct_id.clone())
            .and_modify(|e| *e = e.max(*flow))
            .or_insert(*flow);
    }
}

fn find_headwater_structures(pipes: &[NetworkPipeLink]) -> Vec<String> {
    let mut upstream = HashSet::new();
    let mut downstream = HashSet::new();
    for link in pipes {
        upstream.insert(ci_key(&link.upstream_structure_id));
        downstream.insert(ci_key(&link.downstream_structure_id));
    }
    let mut headwaters: Vec<String> = upstream
        .iter()
        .filter(|id| !downstream.contains(*id))
        .cloned()
        .collect();
    headwaters.sort();
    headwaters
}

pub fn structure_names_from_pipes(
    pipes: &[NetworkPipeLink],
    structure_id_to_name: Option<&HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut names = HashMap::new();
    let Some(id_to_name) = structure_id_to_name else {
        return names;
    };
    for link in pipes {
        if let Some(us_name) = id_to_name.get(&link.upstream_structure_id) {
            if !us_name.trim().is_empty() {
                names.insert(link.upstream_structure_id.clone(), us_name.clone());
            }
        }
        if let Some(ds_name) = id_to_name.get(&link.downstream_structure_id) {
            if !ds_name.trim().is_empty() {
                names.insert(link.downstream_structure_id.clone(), ds_name.clone());
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Catchment;

    fn idf() -> IdfCurve {
        IdfCurve::new(100.0, 10.0, 0.8)
    }

    fn cm(name: &str, area: f64, c: f64, tc: f64, outlet: &str) -> Catchment {
        Catchment {
            name: name.into(),
            area_acres: area,
            runoff_c: c,
            curve_number: 70.0,
            tc_minutes: tc,
            outfall_structure_id: Some(outlet.into()),
            outfall_structure_name: None,
        }
    }

    fn link(key: &str, us: &str, ds: &str) -> NetworkPipeLink {
        NetworkPipeLink {
            pipe_key: key.into(),
            network_name: "NET".into(),
            pipe_name: key.into(),
            upstream_structure_id: us.into(),
            downstream_structure_id: ds.into(),
        }
    }

    #[test]
    fn route_y_branch_accumulates_at_junction() {
        let catchments = vec![cm("C1", 1.0, 0.5, 10.0, "S1"), cm("C2", 2.0, 0.6, 12.0, "S2")];
        let pipes = vec![
            link("P1", "S1", "S3"),
            link("P2", "S2", "S3"),
            link("P3", "S3", "S4"),
        ];
        let result = route(&catchments, &pipes, &idf(), None, None);
        let q1 = rational::peak(&catchments[0], &idf()).peak_flow_cfs;
        let q2 = rational::peak(&catchments[1], &idf()).peak_flow_cfs;
        assert_eq!(result.assignment_method, CatchmentAssignmentMethod::OutletStructure);
        assert!((result.pipe_flow_cfs["P1"] - q1).abs() < 0.01);
        assert!((result.pipe_flow_cfs["P2"] - q2).abs() < 0.01);
        assert!((result.pipe_flow_cfs["P3"] - (q1 + q2)).abs() < 0.01);
    }

    #[test]
    fn route_unequal_branch_lengths_trunk_carries_full_sum() {
        let catchments = vec![cm("C1", 1.0, 0.5, 10.0, "H1"), cm("C2", 2.0, 0.6, 12.0, "H2")];
        let pipes = vec![
            link("P1", "H1", "A"),
            link("P2", "A", "C"),
            link("P3", "H2", "C"),
            link("P4", "C", "OUT"),
        ];
        let result = route(&catchments, &pipes, &idf(), None, None);
        let q1 = rational::peak(&catchments[0], &idf()).peak_flow_cfs;
        let q2 = rational::peak(&catchments[1], &idf()).peak_flow_cfs;
        assert!((result.pipe_flow_cfs["P4"] - (q1 + q2)).abs() < 0.01);
    }

    #[test]
    fn route_unconnected_outfall_end_does_not_throw() {
        let catchments = vec![cm("C1", 1.0, 0.5, 10.0, "S1")];
        let pipes = vec![link("P1", "S1", "")];
        let result = route(&catchments, &pipes, &idf(), None, None);
        let q1 = rational::peak(&catchments[0], &idf()).peak_flow_cfs;
        assert!((result.pipe_flow_cfs["P1"] - q1).abs() < 0.01);
    }
}