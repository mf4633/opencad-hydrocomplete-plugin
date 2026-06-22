//! Storm pipe unit costs ($/LF).

#[derive(Debug, Clone)]
pub struct CostLine {
    pub material: &'static str,
    pub diameter_in: f64,
    pub cost_per_lf: f64,
}

#[derive(Debug, Clone)]
pub struct PipeCostItem {
    pub pipe_name: String,
    pub network_name: String,
    pub length_ft: f64,
    pub diameter_ft: f64,
    pub material: String,
    pub cost_per_lf: f64,
    pub total_cost: f64,
}

#[derive(Debug, Clone)]
pub struct NetworkCostRollup {
    pub network_name: String,
    pub total_length_ft: f64,
    pub total_cost: f64,
    pub pipes: Vec<PipeCostItem>,
}

const CATALOG: &[CostLine] = &[
    CostLine { material: "RCP", diameter_in: 12.0, cost_per_lf: 45.0 },
    CostLine { material: "RCP", diameter_in: 15.0, cost_per_lf: 52.0 },
    CostLine { material: "RCP", diameter_in: 18.0, cost_per_lf: 62.0 },
    CostLine { material: "RCP", diameter_in: 24.0, cost_per_lf: 78.0 },
    CostLine { material: "RCP", diameter_in: 30.0, cost_per_lf: 95.0 },
    CostLine { material: "RCP", diameter_in: 36.0, cost_per_lf: 115.0 },
    CostLine { material: "RCP", diameter_in: 42.0, cost_per_lf: 138.0 },
    CostLine { material: "RCP", diameter_in: 48.0, cost_per_lf: 165.0 },
    CostLine { material: "PVC", diameter_in: 12.0, cost_per_lf: 38.0 },
    CostLine { material: "PVC", diameter_in: 15.0, cost_per_lf: 44.0 },
    CostLine { material: "PVC", diameter_in: 18.0, cost_per_lf: 52.0 },
    CostLine { material: "PVC", diameter_in: 24.0, cost_per_lf: 68.0 },
    CostLine { material: "HDPE", diameter_in: 12.0, cost_per_lf: 42.0 },
    CostLine { material: "HDPE", diameter_in: 18.0, cost_per_lf: 58.0 },
    CostLine { material: "HDPE", diameter_in: 24.0, cost_per_lf: 72.0 },
    CostLine { material: "BOX", diameter_in: 24.0, cost_per_lf: 120.0 },
    CostLine { material: "BOX", diameter_in: 36.0, cost_per_lf: 165.0 },
    CostLine { material: "BOX", diameter_in: 48.0, cost_per_lf: 210.0 },
];

pub fn lookup_cost_per_lf(diameter_ft: f64, material: &str) -> f64 {
    if diameter_ft <= 0.0 {
        return 0.0;
    }
    let diameter_in = diameter_ft * 12.0;
    let mat = if material.trim().is_empty() {
        "RCP"
    } else {
        material.trim()
    };

    let mut best: Option<&CostLine> = None;
    let mut best_diff = f64::MAX;
    for line in CATALOG {
        if !line.material.eq_ignore_ascii_case(mat) {
            continue;
        }
        let diff = (line.diameter_in - diameter_in).abs();
        if diff < best_diff {
            best_diff = diff;
            best = Some(line);
        }
    }
    if best.is_none() {
        for line in CATALOG {
            if !line.material.eq_ignore_ascii_case("RCP") {
                continue;
            }
            let diff = (line.diameter_in - diameter_in).abs();
            if diff < best_diff {
                best_diff = diff;
                best = Some(line);
            }
        }
    }
    best.map(|l| l.cost_per_lf).unwrap_or(0.0)
}

pub fn rollup_by_network(
    pipes: &[(String, String, f64, f64, String)],
) -> Vec<NetworkCostRollup> {
    let mut by_network: std::collections::HashMap<String, NetworkCostRollup> =
        std::collections::HashMap::new();
    for (network, pipe, length_ft, diameter_ft, material) in pipes {
        let net_name = if network.trim().is_empty() {
            "Network".to_string()
        } else {
            network.trim().to_string()
        };
        let rollup = by_network
            .entry(net_name.clone())
            .or_insert_with(|| NetworkCostRollup {
                network_name: net_name,
                total_length_ft: 0.0,
                total_cost: 0.0,
                pipes: Vec::new(),
            });
        let rate = lookup_cost_per_lf(*diameter_ft, material);
        let total = rate * length_ft;
        rollup.pipes.push(PipeCostItem {
            pipe_name: pipe.clone(),
            network_name: rollup.network_name.clone(),
            length_ft: *length_ft,
            diameter_ft: *diameter_ft,
            material: material.clone(),
            cost_per_lf: rate,
            total_cost: total,
        });
        rollup.total_length_ft += length_ft;
        rollup.total_cost += total;
    }
    let mut list: Vec<_> = by_network.into_values().collect();
    list.sort_by(|a, b| a.network_name.cmp(&b.network_name));
    list
}