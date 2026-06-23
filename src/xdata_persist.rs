//! Keep HydroComplete XDATA round-tripping through OCS DWG save/reopen.
//!
//! `acadrust` stores plugin XDATA in [`ExtendedData::records`] during editing, but
//! the DWG writer only serializes [`ExtendedData::raw_dwg_eed`]. On DWG read the
//! inverse happens: only `raw_dwg_eed` is populated. This module registers APPIDs,
//! encodes records before save, and hydrates records after open.

use acadrust::tables::{AppId, TableEntry};
use acadrust::xdata::{ExtendedDataRecord, XDataValue};
use acadrust::{CadDocument, DxfVersion, EntityType, Handle};
use ocs_plugin_api::host::HostApi;

use crate::data::{APP_CATCHMENT, APP_PIPE, APP_STRUCT};
use crate::drawing_params::APP_PARAMS;

/// All HydroComplete application names declared in `plugin.toml` / manifest.
pub const HYDRO_XDATA_APPS: &[&str] = &[APP_STRUCT, APP_PIPE, APP_CATCHMENT, APP_PARAMS];

fn is_hydro_app(name: &str) -> bool {
    HYDRO_XDATA_APPS.contains(&name)
}

fn dwg_strings_wide(doc: &CadDocument) -> bool {
    doc.version >= DxfVersion::AC1021
}

/// Register HC APPIDs so DWG/DXF writers can resolve application handles.
pub fn ensure_xdata_app_ids(doc: &mut CadDocument) {
    for &name in HYDRO_XDATA_APPS {
        if !doc.app_ids.contains(name) {
            let mut app = AppId::new(name);
            app.set_handle(doc.allocate_handle());
            let _ = doc.app_ids.add(app);
        } else if doc.app_ids.get(name).is_some_and(|a| a.handle.is_null()) {
            let h = doc.allocate_handle();
            if let Some(app) = doc.app_ids.get_mut(name) {
                app.set_handle(h);
            }
        }
    }
}

fn app_name_for_handle(doc: &CadDocument, app_handle: u64) -> Option<String> {
    doc.app_ids
        .iter()
        .find(|a| a.handle.value() == app_handle)
        .map(|a| a.name.clone())
}

fn app_handle_for_name(doc: &CadDocument, name: &str) -> Option<u64> {
    doc.app_ids.get(name).map(|a| a.handle.value())
}

fn encode_string(s: &str, wide: bool) -> Vec<u8> {
    let mut b = Vec::new();
    b.push(0);
    if wide {
        let units: Vec<u16> = s.encode_utf16().collect();
        b.extend_from_slice(&(units.len() as u16).to_le_bytes());
        for u in units {
            b.extend_from_slice(&u.to_le_bytes());
        }
    } else {
        b.push(s.len().min(255) as u8);
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(s.as_bytes());
    }
    b
}

fn encode_value(value: &XDataValue, wide: bool) -> Vec<u8> {
    match value {
        XDataValue::String(s) => encode_string(s, wide),
        XDataValue::ControlString(s) => {
            let mut b = vec![2];
            b.push(if s == "}" { 1 } else { 0 });
            b
        }
        XDataValue::LayerName(s) => encode_string(s, wide), // code 3 uses same layout as 0 in practice
        XDataValue::BinaryData(data) => {
            let mut b = vec![4, data.len().min(255) as u8];
            b.extend_from_slice(data);
            b
        }
        XDataValue::Handle(h) => {
            let mut b = vec![5];
            b.extend_from_slice(&h.value().to_le_bytes());
            b
        }
        XDataValue::Point3D(p)
        | XDataValue::Position3D(p)
        | XDataValue::Displacement3D(p)
        | XDataValue::Direction3D(p) => {
            let code = match value {
                XDataValue::Point3D(_) => 10,
                XDataValue::Position3D(_) => 11,
                XDataValue::Displacement3D(_) => 12,
                XDataValue::Direction3D(_) => 13,
                _ => 10,
            };
            let mut b = vec![code];
            b.extend_from_slice(&p.x.to_le_bytes());
            b.extend_from_slice(&p.y.to_le_bytes());
            b.extend_from_slice(&p.z.to_le_bytes());
            b
        }
        XDataValue::Real(v) | XDataValue::Distance(v) | XDataValue::ScaleFactor(v) => {
            let code = match value {
                XDataValue::Distance(_) => 41,
                XDataValue::ScaleFactor(_) => 42,
                _ => 40,
            };
            let mut b = vec![code];
            b.extend_from_slice(&v.to_le_bytes());
            b
        }
        XDataValue::Integer16(v) => {
            let mut b = vec![70];
            b.extend_from_slice(&v.to_le_bytes());
            b
        }
        XDataValue::Integer32(v) => {
            let mut b = vec![71];
            b.extend_from_slice(&v.to_le_bytes());
            b
        }
    }
}

fn encode_record(record: &ExtendedDataRecord, wide: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in &record.values {
        bytes.extend_from_slice(&encode_value(value, wide));
    }
    bytes
}

fn decode_string(bytes: &[u8], i: &mut usize, wide: bool) -> Option<String> {
    if wide {
        let n = u16::from_le_bytes([*bytes.get(*i)?, *bytes.get(*i + 1)?]) as usize;
        *i += 2;
        let mut units = Vec::with_capacity(n);
        for _ in 0..n {
            let u = u16::from_le_bytes([*bytes.get(*i)?, *bytes.get(*i + 1)?]);
            *i += 2;
            units.push(u);
        }
        String::from_utf16(&units).ok()
    } else {
        let n = *bytes.get(*i)? as usize;
        *i += 1 + 2 + n;
        // Narrow strings are best-effort for legacy DWGs; HC uses UTF-8 names/values.
        Some(String::new())
    }
}

fn decode_value(bytes: &[u8], i: &mut usize, wide: bool) -> Option<XDataValue> {
    let code = *bytes.get(*i)?;
    *i += 1;
    match code {
        0 => decode_string(bytes, i, wide).map(XDataValue::String),
        2 => {
            let ctrl = *bytes.get(*i)?;
            *i += 1;
            Some(XDataValue::ControlString(if ctrl == 1 {
                "}".into()
            } else {
                "{".into()
            }))
        }
        3 => decode_string(bytes, i, wide).map(XDataValue::LayerName),
        4 => {
            let n = *bytes.get(*i)? as usize;
            *i += 1;
            let data = bytes.get(*i..*i + n)?.to_vec();
            *i += n;
            Some(XDataValue::BinaryData(data))
        }
        5 => {
            let h = u64::from_le_bytes(bytes.get(*i..*i + 8)?.try_into().ok()?);
            *i += 8;
            Some(XDataValue::Handle(Handle::new(h)))
        }
        10 | 11 | 12 | 13 => {
            let x = f64::from_le_bytes(bytes.get(*i..*i + 8)?.try_into().ok()?);
            *i += 8;
            let y = f64::from_le_bytes(bytes.get(*i..*i + 8)?.try_into().ok()?);
            *i += 8;
            let z = f64::from_le_bytes(bytes.get(*i..*i + 8)?.try_into().ok()?);
            *i += 8;
            let p = acadrust::types::Vector3::new(x, y, z);
            Some(match code {
                11 => XDataValue::Position3D(p),
                12 => XDataValue::Displacement3D(p),
                13 => XDataValue::Direction3D(p),
                _ => XDataValue::Point3D(p),
            })
        }
        40 => {
            let v = f64::from_le_bytes(bytes.get(*i..*i + 8)?.try_into().ok()?);
            *i += 8;
            Some(XDataValue::Real(v))
        }
        41 => {
            let v = f64::from_le_bytes(bytes.get(*i..*i + 8)?.try_into().ok()?);
            *i += 8;
            Some(XDataValue::Distance(v))
        }
        42 => {
            let v = f64::from_le_bytes(bytes.get(*i..*i + 8)?.try_into().ok()?);
            *i += 8;
            Some(XDataValue::ScaleFactor(v))
        }
        70 => {
            let v = i16::from_le_bytes(bytes.get(*i..*i + 2)?.try_into().ok()?);
            *i += 2;
            Some(XDataValue::Integer16(v))
        }
        71 => {
            let v = i32::from_le_bytes(bytes.get(*i..*i + 4)?.try_into().ok()?);
            *i += 4;
            Some(XDataValue::Integer32(v))
        }
        _ => None,
    }
}

fn decode_record(app_name: &str, bytes: &[u8], wide: bool) -> Option<ExtendedDataRecord> {
    let mut rec = ExtendedDataRecord::new(app_name);
    let mut i = 0usize;
    while i < bytes.len() {
        let value = decode_value(bytes, &mut i, wide)?;
        rec.add_value(value);
    }
    if rec.is_empty() {
        return None;
    }
    Some(rec)
}

/// Populate parsed `records` from DWG `raw_dwg_eed` blobs (after open).
pub fn hydrate_entity_xdata(doc: &CadDocument, ent: &mut EntityType) {
    let wide = dwg_strings_wide(doc);
    let blobs: Vec<(u64, Vec<u8>)> = ent.common().extended_data.raw_dwg_eed.clone();
    if blobs.is_empty() {
        return;
    }
    for (app_handle, bytes) in blobs {
        let Some(name) = app_name_for_handle(doc, app_handle) else {
            continue;
        };
        if !is_hydro_app(&name) {
            continue;
        }
        let xd = &mut ent.common_mut().extended_data;
        if xd.get_record(&name).is_some() {
            continue;
        }
        if let Some(rec) = decode_record(&name, &bytes, wide) {
            xd.add_record(rec);
        }
    }
}

/// Encode in-memory HC `records` into `raw_dwg_eed` for DWG save.
pub fn commit_entity_xdata(doc: &CadDocument, ent: &mut EntityType) {
    let wide = dwg_strings_wide(doc);
    let hydro_handles: std::collections::HashSet<u64> = HYDRO_XDATA_APPS
        .iter()
        .filter_map(|name| app_handle_for_name(doc, name))
        .collect();

    let preserved: Vec<(u64, Vec<u8>)> = ent
        .common()
        .extended_data
        .raw_dwg_eed
        .iter()
        .filter(|(h, _)| !hydro_handles.contains(h))
        .cloned()
        .collect();

    let records: Vec<ExtendedDataRecord> = ent
        .common()
        .extended_data
        .records()
        .iter()
        .filter(|r| is_hydro_app(&r.application_name))
        .cloned()
        .collect();

    let mut encoded = Vec::new();
    for record in records {
        let Some(app_handle) = app_handle_for_name(doc, &record.application_name) else {
            continue;
        };
        encoded.push((app_handle, encode_record(&record, wide)));
    }

    let xd = &mut ent.common_mut().extended_data;
    xd.raw_dwg_eed.clear();
    xd.raw_dwg_eed.extend(preserved);
    xd.raw_dwg_eed.extend(encoded);
}

pub fn hydrate_document(doc: &mut CadDocument) {
    ensure_xdata_app_ids(doc);
    let wide = dwg_strings_wide(doc);
    let app_by_handle: std::collections::HashMap<u64, String> = doc
        .app_ids
        .iter()
        .map(|a| (a.handle.value(), a.name.clone()))
        .collect();
    for ent in doc.entities_mut() {
        let blobs: Vec<(u64, Vec<u8>)> = ent.common().extended_data.raw_dwg_eed.clone();
        for (app_handle, bytes) in blobs {
            let Some(name) = app_by_handle.get(&app_handle) else {
                continue;
            };
            if !is_hydro_app(name) {
                continue;
            }
            let xd = &mut ent.common_mut().extended_data;
            if xd.get_record(name).is_some() {
                continue;
            }
            if let Some(rec) = decode_record(name, &bytes, wide) {
                xd.add_record(rec);
            }
        }
    }
}

pub fn commit_document(doc: &mut CadDocument) {
    ensure_xdata_app_ids(doc);
    let wide = dwg_strings_wide(doc);
    let app_by_name: std::collections::HashMap<String, u64> = doc
        .app_ids
        .iter()
        .map(|a| (a.name.clone(), a.handle.value()))
        .collect();
    let hydro_handles: std::collections::HashSet<u64> = HYDRO_XDATA_APPS
        .iter()
        .filter_map(|name| app_by_name.get(*name).copied())
        .collect();

    for ent in doc.entities_mut() {
        let preserved: Vec<(u64, Vec<u8>)> = ent
            .common()
            .extended_data
            .raw_dwg_eed
            .iter()
            .filter(|(h, _)| !hydro_handles.contains(h))
            .cloned()
            .collect();
        let records: Vec<ExtendedDataRecord> = ent
            .common()
            .extended_data
            .records()
            .iter()
            .filter(|r| is_hydro_app(&r.application_name))
            .cloned()
            .collect();
        let mut encoded = Vec::new();
        for record in records {
            let Some(app_handle) = app_by_name.get(&record.application_name).copied() else {
                continue;
            };
            encoded.push((app_handle, encode_record(&record, wide)));
        }
        let xd = &mut ent.common_mut().extended_data;
        xd.raw_dwg_eed.clear();
        xd.raw_dwg_eed.extend(preserved);
        xd.raw_dwg_eed.extend(encoded);
    }
}

/// Host wrapper: hydrate then (after edits) commit for DWG persistence.
pub fn hydrate_host(host: &mut dyn HostApi) {
    hydrate_document(host.document_mut());
}

pub fn commit_host(host: &mut dyn HostApi) {
    commit_document(host.document_mut());
    host.set_dirty();
}

#[cfg(test)]
mod tests {
    use super::*;
    use acadrust::types::Vector3;
    use acadrust::{Circle, DwgReader, DwgWriter, EntityType, Line};
    use std::io::Cursor;
    use stormsewer::network::NodeKind;

    use crate::data::{network_from_entities, pipe_xdata, structure_xdata};

    fn inlet_outfall_pipe_doc() -> CadDocument {
        let mut doc = CadDocument::default();
        ensure_xdata_app_ids(&mut doc);

        let mut s1 = EntityType::Circle(Circle {
            center: Vector3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s1.common_mut().handle = Handle::new(1);
        s1.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Inlet, 100.0, 106.0, 1.0, 0.7));

        let mut s2 = EntityType::Circle(Circle {
            center: Vector3::new(100.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        s2.common_mut().handle = Handle::new(2);
        s2.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Outfall, 99.0, 104.0, 0.0, 0.0));

        let mut p = EntityType::Line(Line::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(100.0, 0.0, 0.0),
        ));
        p.common_mut().handle = Handle::new(3);
        p.common_mut()
            .extended_data
            .add_record(pipe_xdata(1.5, 0.013, Handle::new(1), Handle::new(2)));

        let _ = doc.add_entity(s1);
        let _ = doc.add_entity(s2);
        let _ = doc.add_entity(p);
        doc
    }

    #[test]
    fn encode_decode_roundtrip_record() {
        let rec = structure_xdata(NodeKind::Inlet, 100.0, 106.0, 1.5, 0.78);
        let bytes = encode_record(&rec, true);
        let back = decode_record(APP_STRUCT, &bytes, true).expect("decode");
        assert_eq!(back.application_name, APP_STRUCT);
        assert_eq!(back.values.len(), rec.values.len());
    }

    #[test]
    fn hydro_xdata_survives_dwg_roundtrip() {
        let mut doc = inlet_outfall_pipe_doc();
        commit_document(&mut doc);
        assert!(
            doc.entities().any(|e| !e.common().extended_data.raw_dwg_eed.is_empty()),
            "commit should populate raw_dwg_eed"
        );

        let bytes = DwgWriter::write_to_vec(&doc).expect("dwg write");
        let mut rt = DwgReader::from_stream(Cursor::new(bytes))
            .read()
            .expect("dwg read");

        // Simulate fresh open: only raw blobs, no parsed records.
        for ent in rt.entities_mut() {
            ent.common_mut().extended_data.clear();
        }
        hydrate_document(&mut rt);

        let net = network_from_entities(rt.entities()).expect("network after reopen");
        assert_eq!(net.nodes.len(), 2, "structures lost after DWG round-trip");
        assert_eq!(net.pipes.len(), 1, "pipes lost after DWG round-trip");
    }
}