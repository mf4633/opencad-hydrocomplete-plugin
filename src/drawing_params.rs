//! Persist storm analysis parameters in the drawing via `HYDROCOMPLETE_PARAMS` XDATA.
//!
//! Tab plugin state resets when a DWG is opened in a new session; this marker keeps
//! IDF presets and hydraulics/sizing options with the file.

use acadrust::types::Vector3;
use acadrust::xdata::{ExtendedDataRecord, XDataValue};
use acadrust::{EntityType, Handle, MText};
use ocs_plugin_api::host::HostApi;
use serde::{Deserialize, Serialize};
use stormsewer::idf::IdfCurve;
use stormsewer::params::StormAnalysisParams;

pub const APP_PARAMS: &str = "HYDROCOMPLETE_PARAMS";
const META_LAYER: &str = "HC-META";
const BLOB_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct CurveRow {
    rp: u32,
    a: f64,
    b: f64,
    c: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct ParamsBlob {
    v: u32,
    design_rp: u32,
    curves: Vec<CurveRow>,
    preset_key: Option<String>,
    min_tc: f64,
    tailwater: Option<f64>,
    junction_k: f64,
    intensity_override: Option<f64>,
    min_slope: f64,
    min_velocity: f64,
    max_velocity: f64,
    max_pct_full: f64,
    inlet_grate_length_ft: f64,
    inlet_curb_length_ft: f64,
    inlet_flow_depth_ft: f64,
    inlet_gutter_slope: f64,
}

impl ParamsBlob {
    pub(crate) fn from_params(params: &StormAnalysisParams, preset_key: Option<&str>) -> Self {
        let curves = params
            .idf
            .return_periods()
            .into_iter()
            .filter_map(|rp| {
                params.idf.curve(rp).map(|c| CurveRow {
                    rp,
                    a: c.a,
                    b: c.b,
                    c: c.c,
                })
            })
            .collect();
        Self {
            v: BLOB_VERSION,
            design_rp: params.idf.design_rp,
            curves,
            preset_key: preset_key.map(str::to_string),
            min_tc: params.hydraulics.min_tc,
            tailwater: params.hydraulics.tailwater,
            junction_k: params.hydraulics.junction_k,
            intensity_override: params.hydraulics.intensity_override,
            min_slope: params.hydraulics.min_slope,
            min_velocity: params.sizing.min_velocity,
            max_velocity: params.sizing.max_velocity,
            max_pct_full: params.sizing.max_pct_full,
            inlet_grate_length_ft: params.inlet_grate_length_ft,
            inlet_curb_length_ft: params.inlet_curb_length_ft,
            inlet_flow_depth_ft: params.inlet_flow_depth_ft,
            inlet_gutter_slope: params.inlet_gutter_slope,
        }
    }

    fn into_params(self) -> Result<StormAnalysisParams, String> {
        if self.v != BLOB_VERSION {
            return Err(format!("unsupported params blob version {}", self.v));
        }
        if self.curves.is_empty() {
            return Err("params blob has no IDF curves".into());
        }
        let mut params = StormAnalysisParams::municipal();
        for row in self.curves {
            params
                .idf
                .set_curve(row.rp, IdfCurve::new(row.a, row.b, row.c));
        }
        params.idf.set_design_rp(self.design_rp);
        params.hydraulics.min_tc = self.min_tc;
        params.hydraulics.tailwater = self.tailwater;
        params.hydraulics.junction_k = self.junction_k;
        params.hydraulics.intensity_override = self.intensity_override;
        params.hydraulics.min_slope = self.min_slope;
        params.sizing.min_velocity = self.min_velocity;
        params.sizing.max_velocity = self.max_velocity;
        params.sizing.max_pct_full = self.max_pct_full;
        params.inlet_grate_length_ft = self.inlet_grate_length_ft;
        params.inlet_curb_length_ft = self.inlet_curb_length_ft;
        params.inlet_flow_depth_ft = self.inlet_flow_depth_ft;
        params.inlet_gutter_slope = self.inlet_gutter_slope;
        Ok(params)
    }
}

fn params_record(json: &str) -> ExtendedDataRecord {
    let mut r = ExtendedDataRecord::new(APP_PARAMS);
    r.add_value(XDataValue::String(json.to_string()));
    r
}

fn read_blob_from_entity(e: &EntityType) -> Option<ParamsBlob> {
    let rec = e.common().extended_data.get_record(APP_PARAMS)?;
    let json = match rec.values.first()? {
        XDataValue::String(s) => s.as_str(),
        _ => return None,
    };
    serde_json::from_str(json).ok()
}

/// Read persisted params from any entity carrying `HYDROCOMPLETE_PARAMS`.
pub fn read_params_from_entities<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
) -> Option<StormAnalysisParams> {
    for e in entities {
        if let Some(blob) = read_blob_from_entity(e) {
            return blob.into_params().ok();
        }
    }
    None
}

fn find_params_handle<'a>(entities: impl IntoIterator<Item = &'a EntityType>) -> Option<Handle> {
    for e in entities {
        if e.common().extended_data.get_record(APP_PARAMS).is_some() {
            return Some(e.common().handle);
        }
    }
    None
}

/// Write params to the drawing (update marker or create hidden MText).
pub fn write_params_to_drawing(
    host: &mut dyn HostApi,
    params: &StormAnalysisParams,
    preset_key: Option<&str>,
) -> Result<(), String> {
    let blob = ParamsBlob::from_params(params, preset_key);
    let json = serde_json::to_string(&blob).map_err(|e| format!("params serialize: {e}"))?;

    if let Some(handle) = find_params_handle(host.document().entities()) {
        let record = params_record(&json);
        if !host.write_record(handle, record) {
            return Err("failed to update params marker entity".into());
        }
        host.set_dirty();
        return Ok(());
    }

    host.push_undo("HC_PARAMS");
    let mut ent = EntityType::MText(MText {
        value: "HydroComplete parameters".into(),
        insertion_point: Vector3::new(-1.0e9, -1.0e9, 0.0),
        height: 0.01,
        ..Default::default()
    });
    ent.common_mut().layer = META_LAYER.to_string();
    ent.common_mut()
        .extended_data
        .add_record(params_record(&json));
    host.add_entity(ent);
    host.set_dirty();
    Ok(())
}

/// Prefer drawing-stored params when a marker exists; otherwise tab defaults.
pub fn resolve_params<'a>(
    entities: impl Iterator<Item = &'a EntityType>,
    tab: &StormAnalysisParams,
) -> StormAnalysisParams {
    read_params_from_entities(entities).unwrap_or_else(|| tab.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_charlotte_preset_blob() {
        let mut params = StormAnalysisParams::municipal();
        let preset = hydrocomplete::atlas14_presets::find("charlotte-nc").unwrap();
        let curve = preset.to_curve(10).unwrap();
        params.idf.set_curve(10, curve);
        params.idf.set_design_rp(10);

        let blob = ParamsBlob::from_params(&params, Some("charlotte-nc"));
        let json = serde_json::to_string(&blob).unwrap();
        let back: ParamsBlob = serde_json::from_str(&json).unwrap();
        let restored = back.into_params().unwrap();
        assert!((restored.idf.design_curve().a - 81.2).abs() < 0.1);
        assert_eq!(restored.idf.design_rp, 10);
    }
}