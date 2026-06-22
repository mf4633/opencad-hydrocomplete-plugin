// Per-document storm-sewer parameters (stored in host tab plugin_state).

use stormsewer::params::StormAnalysisParams;

#[derive(Clone, Debug, PartialEq)]
pub struct HydroTabState {
    pub params: StormAnalysisParams,
}

impl Default for HydroTabState {
    fn default() -> Self {
        Self {
            params: StormAnalysisParams::municipal(),
        }
    }
}

impl HydroTabState {
    pub fn params(&self) -> &StormAnalysisParams {
        &self.params
    }

    pub fn params_mut(&mut self) -> &mut StormAnalysisParams {
        &mut self.params
    }
}