// Process-local storm-sewer parameters. Since OCS 0.7.x the plugin runs in
// its own runner process, and the host's `ensure_plugin_state` PANICS
// out-of-process by design — `dyn Any` can't cross the IPC boundary. A
// process global is the prescribed replacement; the runner process is
// per-plugin, so this is exactly plugin-scoped state.
//
// Scope change vs the old host-side store: params are shared across tabs (one
// runner serves every tab) instead of per-tab. Acceptable here because
// `tab_params` re-reads drawing-persisted params from the active document's
// entities on every command, overwriting the global before use.

use std::sync::{Mutex, MutexGuard, OnceLock};

use stormsewer::params::StormAnalysisParams;

static STATE: OnceLock<Mutex<HydroTabState>> = OnceLock::new();

/// Lock the process-local HydroComplete state. Dispatch is single-threaded per
/// command, but NEVER call this while an earlier guard from the same call
/// chain is still alive — `Mutex` is not reentrant.
pub fn state() -> MutexGuard<'static, HydroTabState> {
    STATE
        .get_or_init(|| Mutex::new(HydroTabState::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone, Debug, PartialEq)]
pub struct HydroTabState {
    pub params: StormAnalysisParams,
    /// Last Atlas 14 preset key applied via `HC_PARAMS PRESET` (for drawing persistence).
    pub preset_key: Option<String>,
}

impl Default for HydroTabState {
    fn default() -> Self {
        Self {
            params: StormAnalysisParams::municipal(),
            preset_key: None,
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