// Per-document analysis parameters.
//
// Kept in a plugin-owned map keyed by the host tab id. The host's
// `ensure_plugin_state` helper is in-process only and panics under the
// out-of-process runner every current OCS release uses, so state must live
// on this side of the IPC boundary.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use ocs_plugin_api::host::HostApi;
use stormsewer::params::StormAnalysisParams;

static TAB_STATES: Mutex<Option<HashMap<u64, HydroTabState>>> = Mutex::new(None);

fn states() -> MutexGuard<'static, Option<HashMap<u64, HydroTabState>>> {
    TAB_STATES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Run `f` against the active tab's mutable state, creating it on first use.
pub fn with_tab_state_mut<R>(host: &dyn HostApi, f: impl FnOnce(&mut HydroTabState) -> R) -> R {
    let id = host.tab_id();
    let mut guard = states();
    let map = guard.get_or_insert_with(HashMap::new);
    f(map.entry(id).or_default())
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