//! CAD-agnostic pipe and catchment models (mirrors `HydroComplete.Engine.Models`).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeShape {
    Circular,
    Box,
    Arch,
}

#[derive(Debug, Clone)]
pub struct PipeSegment {
    pub name: String,
    pub shape: PipeShape,
    /// Inside diameter, ft (circular).
    pub diameter_ft: f64,
    /// Inside width, ft (box / arch span).
    pub width_ft: f64,
    /// Inside height, ft (box / arch rise).
    pub height_ft: f64,
    pub span_ft: f64,
    pub rise_ft: f64,
    pub slope: f64,
    pub manning_n: f64,
    pub design_flow_cfs: f64,
    pub length_ft: f64,
    pub start_invert_ft: f64,
    pub end_invert_ft: f64,
}

impl PipeSegment {
    pub fn circular(name: impl Into<String>, diameter_ft: f64, slope: f64, n: f64) -> Self {
        Self {
            name: name.into(),
            shape: PipeShape::Circular,
            diameter_ft,
            width_ft: 0.0,
            height_ft: 0.0,
            span_ft: 0.0,
            rise_ft: 0.0,
            slope,
            manning_n: n,
            design_flow_cfs: 0.0,
            length_ft: 0.0,
            start_invert_ft: 0.0,
            end_invert_ft: 0.0,
        }
    }

    pub fn effective_span_ft(&self) -> f64 {
        if self.span_ft > 0.0 {
            self.span_ft
        } else {
            self.width_ft
        }
    }

    pub fn effective_rise_ft(&self) -> f64 {
        if self.rise_ft > 0.0 {
            self.rise_ft
        } else {
            self.height_ft
        }
    }
}

#[derive(Debug, Clone)]
pub struct Catchment {
    pub name: String,
    pub area_acres: f64,
    pub runoff_c: f64,
    pub curve_number: f64,
    pub tc_minutes: f64,
    /// Structure handle/id where this catchment drains (optional).
    pub outfall_structure_id: Option<String>,
    /// Structure name match when id is unknown (optional).
    pub outfall_structure_name: Option<String>,
}

/// Pipe link for catchment routing (topology only).
#[derive(Debug, Clone)]
pub struct NetworkPipeLink {
    pub pipe_key: String,
    pub network_name: String,
    pub pipe_name: String,
    pub upstream_structure_id: String,
    pub downstream_structure_id: String,
}

/// Pipe with Manning geometry for network analysis.
#[derive(Debug, Clone)]
pub struct NetworkAnalysisPipe {
    pub pipe_key: String,
    pub network_name: String,
    pub pipe_name: String,
    pub link: NetworkPipeLink,
    pub segment: PipeSegment,
    pub upstream_node_id: String,
    pub downstream_node_id: String,
    pub length_ft: f64,
    pub upstream_invert_ft: f64,
    pub downstream_invert_ft: f64,
}