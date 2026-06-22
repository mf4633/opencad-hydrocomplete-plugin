// SPDX-License-Identifier: GPL-3.0-only

//! LandXML 1.2 pipe-network import (Civil 3D / InfraModel compatible subset).

use crate::network::NodeKind;
use quick_xml::events::Event;
use quick_xml::Reader;
/// Linear units used in the source document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LinearUnit {
    #[default]
    Foot,
    Meter,
}

/// Diameter units for circular pipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DiameterUnit {
    #[default]
    Inch,
    Foot,
    Millimeter,
    Meter,
}

/// Parsed LandXML document (one or more pipe networks).
#[derive(Clone, Debug, Default)]
pub struct LandXmlDocument {
    pub linear_unit: LinearUnit,
    pub diameter_unit: DiameterUnit,
    pub networks: Vec<LandXmlNetwork>,
}

/// A single pipe network from LandXML.
#[derive(Clone, Debug, Default)]
pub struct LandXmlNetwork {
    pub name: String,
    pub structures: Vec<LandXmlStruct>,
    pub pipes: Vec<LandXmlPipe>,
}

/// Structure (manhole / inlet / outfall) from LandXML.
#[derive(Clone, Debug)]
pub struct LandXmlStruct {
    pub name: String,
    pub kind: NodeKind,
    /// Easting / X (ft).
    pub x: f64,
    /// Northing / Y (ft).
    pub y: f64,
    pub invert: f64,
    pub rim: f64,
    pub area_ac: f64,
    pub c: f64,
}

/// Pipe link from LandXML.
#[derive(Clone, Debug)]
pub struct LandXmlPipe {
    pub name: String,
    pub from: String,
    pub to: String,
    /// Internal diameter (ft).
    pub diameter_ft: f64,
    pub n: f64,
}

impl LandXmlDocument {
    /// First network, or an error when the file contains none.
    pub fn primary_network(&self) -> Result<&LandXmlNetwork, String> {
        self.networks.first().ok_or_else(|| "LandXML: no <PipeNetwork> found".into())
    }
}

/// Parse a LandXML document string into structures and pipes.
pub fn parse_landxml(xml: &str) -> Result<LandXmlDocument, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut doc = LandXmlDocument::default();
    let mut buf = Vec::new();

    let mut in_imperial = false;
    let mut in_metric = false;

    let mut cur_network: Option<LandXmlNetwork> = None;
    let mut in_structs = false;
    let mut in_pipes = false;

    let mut cur_struct: Option<LandXmlStruct> = None;
    let mut cur_pipe: Option<LandXmlPipe> = None;
    let mut text_buf = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let name = local_name(&raw);
                if name == "CircPipe" {
                    if let Some(d) = attr_value(&e, "diameter").and_then(|s| s.parse::<f64>().ok()) {
                        if let Some(p) = cur_pipe.as_mut() {
                            p.diameter_ft = to_diameter_ft(d, doc.diameter_unit);
                        }
                    }
                }
            }
            Ok(Event::Start(e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let name = local_name(&raw);
                text_buf.clear();

                match name.as_str() {
                    "Imperial" => {
                        in_imperial = true;
                        if let Some(u) = attr_value(&e, "linearUnit") {
                            doc.linear_unit = parse_linear_unit(&u);
                        }
                        if let Some(u) = attr_value(&e, "diameterUnit") {
                            doc.diameter_unit = parse_diameter_unit(&u);
                        }
                    }
                    "Metric" => {
                        in_metric = true;
                        doc.linear_unit = LinearUnit::Meter;
                    }
                    "PipeNetwork" => {
                        let net_name = attr_value(&e, "name").unwrap_or_else(|| "Network".into());
                        cur_network = Some(LandXmlNetwork { name: net_name, ..Default::default() });
                    }
                    "Structs" if cur_network.is_some() => in_structs = true,
                    "Pipes" if cur_network.is_some() => in_pipes = true,
                    "Struct" if in_structs => {
                        let sname = attr_value(&e, "name")
                            .or_else(|| attr_value(&e, "id"))
                            .unwrap_or_else(|| format!("S{}", cur_network.as_ref().map(|n| n.structures.len()).unwrap_or(0) + 1));
                        let role = attr_value(&e, "role").unwrap_or_default();
                        let kind = infer_kind(&sname, &role);
                        cur_struct = Some(LandXmlStruct {
                            name: sname,
                            kind,
                            x: 0.0,
                            y: 0.0,
                            invert: 0.0,
                            rim: 0.0,
                            area_ac: 0.0,
                            c: 0.7,
                        });
                    }
                    "Pipe" if in_pipes => {
                        let pname = attr_value(&e, "name")
                            .or_else(|| attr_value(&e, "id"))
                            .unwrap_or_else(|| format!("P{}", cur_network.as_ref().map(|n| n.pipes.len()).unwrap_or(0) + 1));
                        cur_pipe = Some(LandXmlPipe {
                            name: pname,
                            from: String::new(),
                            to: String::new(),
                            diameter_ft: 1.0,
                            n: 0.013,
                        });
                    }
                    "CircPipe" if cur_pipe.is_some() => {
                        if let Some(d) = attr_value(&e, "diameter").and_then(|s| s.parse::<f64>().ok()) {
                            if let Some(p) = cur_pipe.as_mut() {
                                p.diameter_ft = to_diameter_ft(d, doc.diameter_unit);
                            }
                        }
                    }
                    "Center" if cur_struct.is_some() => {
                        if let (Some(n), Some(ea)) = (attr_value(&e, "north").and_then(|s| s.parse().ok()), attr_value(&e, "east").and_then(|s| s.parse().ok())) {
                            if let Some(s) = cur_struct.as_mut() {
                                s.y = to_linear_ft(n, doc.linear_unit);
                                s.x = to_linear_ft(ea, doc.linear_unit);
                                if let Some(el) = attr_value(&e, "elev").and_then(|v| v.parse().ok()) {
                                    s.rim = to_linear_ft(el, doc.linear_unit);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                text_buf.push_str(&t.unescape().map_err(|e| e.to_string())?);
            }
            Ok(Event::End(e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let name = local_name(&raw);
                let text = text_buf.trim().to_string();

                if !text.is_empty() {
                    match name.as_str() {
                        "linearUnit" if in_imperial => doc.linear_unit = parse_linear_unit(&text),
                        "diameterUnit" if in_imperial => doc.diameter_unit = parse_diameter_unit(&text),
                        "linearUnit" if in_metric => doc.linear_unit = LinearUnit::Meter,
                        "diameterUnit" if in_metric => doc.diameter_unit = parse_diameter_unit(&text),
                        "Center" if let Some(s) = cur_struct.as_mut() => {
                            if let Some((a, b, c)) = parse_coords(&text) {
                                // LandXML may be N E Z or E N Z; prefer E N when third is elevation.
                                s.x = to_linear_ft(a, doc.linear_unit);
                                s.y = to_linear_ft(b, doc.linear_unit);
                                if c.abs() > 1e-6 {
                                    s.rim = to_linear_ft(c, doc.linear_unit);
                                }
                            }
                        }
                        "Invert" | "InvertElev" if let Some(s) = cur_struct.as_mut() => {
                            if let Ok(v) = text.parse::<f64>() {
                                s.invert = to_linear_ft(v, doc.linear_unit);
                            }
                        }
                        "ElevRim" | "Rim" | "RimElev" if let Some(s) = cur_struct.as_mut() => {
                            if let Ok(v) = text.parse::<f64>() {
                                s.rim = to_linear_ft(v, doc.linear_unit);
                            }
                        }
                        "StartStruct" | "RefStart" | "BegStruct" if let Some(p) = cur_pipe.as_mut() => {
                            p.from = text;
                        }
                        "EndStruct" | "RefEnd" | "EndStructRef" if let Some(p) = cur_pipe.as_mut() => {
                            p.to = text;
                        }
                        "CircPipe" if let Some(p) = cur_pipe.as_mut() => {
                            if let Ok(d) = text.parse::<f64>() {
                                p.diameter_ft = to_diameter_ft(d, doc.diameter_unit);
                            }
                        }
                        _ => {}
                    }
                }

                match name.as_str() {
                    "Imperial" => in_imperial = false,
                    "Metric" => in_metric = false,
                    "Struct" => {
                        if let Some(s) = cur_struct.take() {
                            if let Some(net) = cur_network.as_mut() {
                                if s.rim <= s.invert {
                                    let mut s = s;
                                    s.rim = s.invert + 5.0;
                                    net.structures.push(s);
                                } else {
                                    net.structures.push(s);
                                }
                            }
                        }
                    }
                    "Pipe" => {
                        if let Some(p) = cur_pipe.take() {
                            if !p.from.is_empty() && !p.to.is_empty() {
                                if let Some(net) = cur_network.as_mut() {
                                    net.pipes.push(p);
                                }
                            }
                        }
                    }
                    "Structs" => in_structs = false,
                    "Pipes" => in_pipes = false,
                    "PipeNetwork" => {
                        if let Some(net) = cur_network.take() {
                            if !net.structures.is_empty() {
                                doc.networks.push(net);
                            }
                        }
                    }
                    _ => {}
                }
                text_buf.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("LandXML parse error at {}: {e}", reader.error_position())),
            _ => {}
        }
        buf.clear();
    }

    if doc.networks.is_empty() {
        return Err("LandXML: no pipe network with structures found".into());
    }

    // Fill missing inverts from rim when needed.
    for net in &mut doc.networks {
        for s in &mut net.structures {
            if s.invert == 0.0 && s.rim != 0.0 {
                s.invert = s.rim - 5.0;
            }
            if s.rim == 0.0 && s.invert != 0.0 {
                s.rim = s.invert + 5.0;
            }
        }
        drop_dangling_pipes(net);
    }

    Ok(doc)
}

fn drop_dangling_pipes(net: &mut LandXmlNetwork) {
    let names: std::collections::HashSet<_> = net.structures.iter().map(|s| s.name.as_str()).collect();
    net.pipes.retain(|p| names.contains(p.from.as_str()) && names.contains(p.to.as_str()));
}

fn local_name(tag: &str) -> String {
    tag.rsplit(':').next().unwrap_or(tag).to_string()
}

fn attr_value(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == key.as_bytes())
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

fn parse_coords(text: &str) -> Option<(f64, f64, f64)> {
    let nums: Vec<f64> = text.split_whitespace().filter_map(|s| s.parse().ok()).collect();
    match nums.len() {
        0 => None,
        1 => Some((nums[0], 0.0, 0.0)),
        2 => Some((nums[0], nums[1], 0.0)),
        _ => Some((nums[0], nums[1], nums[2])),
    }
}

fn parse_linear_unit(s: &str) -> LinearUnit {
    let l = s.to_ascii_lowercase();
    if l.contains("meter") || l == "m" {
        LinearUnit::Meter
    } else {
        LinearUnit::Foot
    }
}

fn parse_diameter_unit(s: &str) -> DiameterUnit {
    let l = s.to_ascii_lowercase();
    if l.contains("milli") {
        DiameterUnit::Millimeter
    } else if l.contains("meter") || l == "m" {
        DiameterUnit::Meter
    } else if l.contains("foot") || l == "ft" {
        DiameterUnit::Foot
    } else {
        DiameterUnit::Inch
    }
}

fn to_linear_ft(v: f64, unit: LinearUnit) -> f64 {
    match unit {
        LinearUnit::Foot => v,
        LinearUnit::Meter => v * 3.280_839_895,
    }
}

fn to_diameter_ft(v: f64, unit: DiameterUnit) -> f64 {
    match unit {
        DiameterUnit::Inch => v / 12.0,
        DiameterUnit::Foot => v,
        DiameterUnit::Millimeter => v / 304.8,
        DiameterUnit::Meter => v * 3.280_839_895,
    }
}

fn infer_kind(name: &str, role: &str) -> NodeKind {
    let n = name.to_ascii_lowercase();
    let r = role.to_ascii_lowercase();
    if r.contains("outfall") || n.contains("outfall") || n.starts_with("of") {
        NodeKind::Outfall
    } else if r.contains("inlet") || n.contains("inlet") || n.starts_with("in") {
        NodeKind::Inlet
    } else if r.contains("junction") || n.contains("mh") || n.contains("manhole") || n.contains("cb") {
        NodeKind::Junction
    } else {
        NodeKind::Junction
    }
}

/// Convert a parsed LandXML network into a HydroComplete [`Network`](crate::network::Network).
pub fn network_from_landxml(net: &LandXmlNetwork) -> Result<crate::network::Network, String> {
    use crate::network::{Network, Node, Pipe};
    use std::collections::HashMap;

    if net.structures.is_empty() {
        return Err("LandXML network has no structures".into());
    }

    let mut id_of: HashMap<String, String> = HashMap::new();
    let mut nodes = Vec::with_capacity(net.structures.len());
    for (i, s) in net.structures.iter().enumerate() {
        let id = format!("N{}", i + 1);
        id_of.insert(s.name.clone(), id.clone());
        let node = match s.kind {
            NodeKind::Inlet => Node::inlet(&id, s.invert, s.rim, s.area_ac, s.c),
            NodeKind::Junction => Node::junction(&id, s.invert, s.rim, s.area_ac, s.c),
            NodeKind::Outfall => Node::outfall(&id, s.invert, s.rim),
        }
        .at(s.x, s.y);
        nodes.push(node);
    }

    let coord: HashMap<_, _> = net.structures.iter().map(|s| (s.name.as_str(), (s.x, s.y))).collect();
    let mut pipes = Vec::new();
    for (k, p) in net.pipes.iter().enumerate() {
        let Some(from_id) = id_of.get(&p.from) else { continue };
        let Some(to_id) = id_of.get(&p.to) else { continue };
        let length = match (coord.get(p.from.as_str()), coord.get(p.to.as_str())) {
            (Some((x0, y0)), Some((x1, y1))) => ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt(),
            _ => 100.0,
        };
        pipes.push(Pipe::new(
            &format!("P{}", k + 1),
            from_id,
            to_id,
            length,
            p.diameter_ft,
            p.n,
        ));
    }

    if pipes.is_empty() {
        return Err("LandXML network has no connected pipes".into());
    }

    Ok(Network { nodes, pipes })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<LandXML xmlns="http://www.landxml.org/schema/LandXML-1.2" version="1.2" date="2026-06-09">
  <Units>
    <Imperial areaUnit="squareFoot" linearUnit="foot" diameterUnit="inch"/>
  </Units>
  <PipeNetworks>
    <PipeNetwork name="Main">
      <Structs>
        <Struct name="IN1" role="inlet">
          <Center>0.0 0.0 110.0</Center>
          <Invert>104.0</Invert>
          <ElevRim>110.0</ElevRim>
        </Struct>
        <Struct name="OUT1" role="outfall">
          <Center>300.0 0.0 106.0</Center>
          <Invert>100.0</Invert>
          <ElevRim>106.0</ElevRim>
        </Struct>
      </Structs>
      <Pipes>
        <Pipe name="P1">
          <CircPipe diameter="18"/>
          <StartStruct>IN1</StartStruct>
          <EndStruct>OUT1</EndStruct>
        </Pipe>
      </Pipes>
    </PipeNetwork>
  </PipeNetworks>
</LandXML>"#;

    #[test]
    fn parses_sample_network() {
        let doc = parse_landxml(SAMPLE).expect("parse");
        let net = doc.primary_network().unwrap();
        assert_eq!(net.structures.len(), 2);
        assert_eq!(net.pipes.len(), 1);
        assert!((net.pipes[0].diameter_ft - 1.5).abs() < 1e-6);
    }

    #[test]
    fn builds_engine_network() {
        let doc = parse_landxml(SAMPLE).unwrap();
        let net = doc.primary_network().unwrap();
        let engine = network_from_landxml(net).unwrap();
        assert_eq!(engine.nodes.len(), 2);
        assert_eq!(engine.pipes.len(), 1);
        assert!((engine.pipes[0].length - 300.0).abs() < 1e-3);
    }
}