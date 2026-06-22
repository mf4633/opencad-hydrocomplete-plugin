use std::collections::HashMap;

use acadrust::types::Vector3;
use acadrust::{Circle, EntityType, Handle, Line};
use ocs_plugin_api::host::HostApi;

use stormsewer::io::landxml::{parse_landxml, LandXmlStruct};

use super::data;

fn structure_entity(s: &LandXmlStruct, radius: f64) -> EntityType {
    let mut e = EntityType::Circle(Circle {
        center: Vector3::new(s.x, s.y, 0.0),
        radius,
        ..Default::default()
    });
    let (area, c) = if s.kind == stormsewer::network::NodeKind::Outfall {
        (0.0, 0.0)
    } else {
        (s.area_ac, s.c)
    };
    e.common_mut()
        .extended_data
        .add_record(data::structure_xdata(s.kind, s.invert, s.rim, area, c));
    e
}

fn pipe_entity(
    diameter: f64,
    n: f64,
    from: Handle,
    to: Handle,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) -> EntityType {
    let mut e = EntityType::Line(Line::from_points(
        Vector3::new(x0, y0, 0.0),
        Vector3::new(x1, y1, 0.0),
    ));
    e.common_mut()
        .extended_data
        .add_record(data::pipe_xdata(diameter, n, from, to));
    e
}

/// Import a LandXML document into the active drawing tab.
pub fn import_landxml(host: &mut dyn HostApi, xml: &str) -> Result<String, String> {
    let doc = parse_landxml(xml)?;
    let net = doc.primary_network()?.clone();

    if net.structures.is_empty() {
        return Err("LandXML: no structures to import".into());
    }

    host.push_undo("HC_LANDXML");

    let mut name_to_handle: HashMap<String, Handle> = HashMap::new();
    let mut coord: HashMap<String, (f64, f64)> = HashMap::new();

    for s in &net.structures {
        let radius = match s.kind {
            stormsewer::network::NodeKind::Outfall => 6.0,
            stormsewer::network::NodeKind::Inlet => 3.0,
            stormsewer::network::NodeKind::Junction => 4.0,
        };
        let ent = structure_entity(s, radius);
        let h = host.add_entity(ent);
        name_to_handle.insert(s.name.clone(), h);
        coord.insert(s.name.clone(), (s.x, s.y));
    }

    let mut pipe_count = 0;
    for p in &net.pipes {
        let Some(&from_h) = name_to_handle.get(&p.from) else {
            continue;
        };
        let Some(&to_h) = name_to_handle.get(&p.to) else {
            continue;
        };
        let (x0, y0) = coord.get(&p.from).copied().unwrap_or((0.0, 0.0));
        let (x1, y1) = coord.get(&p.to).copied().unwrap_or((0.0, 0.0));
        host.add_entity(pipe_entity(
            p.diameter_ft, p.n, from_h, to_h, x0, y0, x1, y1,
        ));
        pipe_count += 1;
    }

    if pipe_count == 0 {
        return Err(
            "LandXML: structures imported but no pipes could be connected — check StartStruct/EndStruct names.".into(),
        );
    }

    host.bump_geometry();
    host.set_dirty();

    Ok(format!(
        "Imported LandXML \"{}\": {} structure(s), {} pipe(s).",
        net.name,
        net.structures.len(),
        pipe_count
    ))
}

#[cfg(test)]
mod tests {
    use stormsewer::io::landxml::parse_landxml;

    const SAMPLE: &str = include_str!("../crates/stormsewer/examples/sample_landxml.xml");

    #[test]
    fn sample_xml_parses_three_structures() {
        let doc = parse_landxml(SAMPLE).unwrap();
        let net = doc.primary_network().unwrap();
        assert_eq!(net.structures.len(), 3);
        assert_eq!(net.pipes.len(), 2);
    }
}