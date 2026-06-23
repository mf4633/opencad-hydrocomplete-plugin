//! HC_EDIT — update structure or pipe XDATA by entity handle.

use acadrust::EntityType;
use acadrust::Handle;
use ocs_plugin_api::host::HostApi;

use stormsewer::network::NodeKind;

use super::data::{self, pipe_xdata};

pub fn usage() -> &'static str {
    "HC_EDIT <handle> <field> <value> [...]  — fields: invert, rim, area, c, tc (structures); diameter, n, from, to (pipes)"
}

fn parse_num(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok()
}

fn parse_handle(s: &str) -> Option<Handle> {
    data::parse_entity_handle(s)
}

fn find_entity_mut<'a>(
    host: &'a mut dyn HostApi,
    handle: Handle,
) -> Option<&'a mut EntityType> {
    host.document_mut()
        .entities_mut()
        .find(|e| e.common().handle == handle)
}

pub fn edit_entity(host: &mut dyn HostApi, args: &str) -> Result<String, String> {
    let tokens: Vec<&str> = args.split_whitespace().collect();
    if tokens.len() < 3 {
        return Err(usage().into());
    }

    let handle = data::resolve_entity_handle(host.document().entities(), tokens[0])
        .ok_or("Invalid entity handle")?;
    let mut changes = Vec::new();
    let mut i = 1;
    while i + 1 < tokens.len() {
        let field = tokens[i].to_ascii_lowercase();
        let value = tokens[i + 1];
        changes.push((field, value));
        i += 2;
    }

    if changes.is_empty() {
        return Err(usage().into());
    }

    host.push_undo("HC_EDIT");

    let Some(ent) = find_entity_mut(host, handle) else {
        return Err(format!("Entity handle {} not found", handle.value()));
    };

    if let Some(mut info) = data::read_structure_info(ent) {
        for (field, value) in &changes {
            match field.as_str() {
                "invert" => info.invert = parse_num(value).ok_or_else(|| format!("bad invert: {value}"))?,
                "rim" => info.rim = parse_num(value).ok_or_else(|| format!("bad rim: {value}"))?,
                "area" => info.area = parse_num(value).ok_or_else(|| format!("bad area: {value}"))?,
                "c" => info.c = parse_num(value).ok_or_else(|| format!("bad C: {value}"))?,
                "tc" | "tc_inlet" => {
                    info.tc_inlet = parse_num(value).ok_or_else(|| format!("bad tc: {value}"))?
                }
                other => return Err(format!("Unknown structure field: {other}")),
            }
        }
        if info.rim <= info.invert {
            return Err(format!("rim ({}) must be above invert ({})", info.rim, info.invert));
        }
        if info.kind == NodeKind::Outfall {
            info.area = 0.0;
            info.c = 0.0;
        }
        data::write_structure_info(ent, &info);
        host.bump_geometry();
        host.set_dirty();
        return Ok(format!(
            "Updated structure {} invert={:.2} rim={:.2}",
            handle.value(),
            info.invert,
            info.rim
        ));
    }

    if let Some(mut info) = data::read_pipe_info(ent) {
        for (field, value) in &changes {
            match field.as_str() {
                "diameter" | "dia" => {
                    info.diameter = parse_num(value).ok_or_else(|| format!("bad diameter: {value}"))?
                }
                "n" | "mannings" => info.n = parse_num(value).ok_or_else(|| format!("bad n: {value}"))?,
                "from" => info.from = parse_handle(value).ok_or_else(|| format!("bad from handle: {value}"))?,
                "to" => info.to = parse_handle(value).ok_or_else(|| format!("bad to handle: {value}"))?,
                other => return Err(format!("Unknown pipe field: {other}")),
            }
        }
        if info.diameter <= 0.0 {
            return Err("diameter must be > 0".into());
        }
        let EntityType::Line(_) = ent else {
            return Err("Pipe entity must be a LINE".into());
        };
        let xd = &mut ent.common_mut().extended_data;
        data::replace_pipe_xdata(xd, pipe_xdata(info.diameter, info.n, info.from, info.to));
        host.bump_geometry();
        host.set_dirty();
        return Ok(format!(
            "Updated pipe {} diameter={:.2} n={:.3}",
            handle.value(),
            info.diameter,
            info.n
        ));
    }

    Err(format!(
        "Handle {} is not a storm-sewer structure or pipe",
        handle.value()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use acadrust::types::Vector3;
    use acadrust::{Circle, EntityType, Handle, Line};
    use stormsewer::network::NodeKind;

    use crate::data::{pipe_xdata, structure_xdata};

    #[test]
    fn structure_info_roundtrip() {
        let mut e = EntityType::Circle(Circle {
            center: Vector3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        e.common_mut().handle = Handle::new(5);
        e.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Inlet, 100.0, 106.0, 1.0, 0.7));
        let info = data::read_structure_info(&e).unwrap();
        data::write_structure_info(&mut e, &data::StructureInfo {
            invert: 104.0,
            ..info
        });
        let info2 = data::read_structure_info(&e).unwrap();
        assert!((info2.invert - 104.0).abs() < 1e-6);
    }

    #[test]
    fn pipe_info_fields() {
        let mut e = EntityType::Line(Line::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(10.0, 0.0, 0.0),
        ));
        e.common_mut().handle = Handle::new(3);
        e.common_mut()
            .extended_data
            .add_record(pipe_xdata(1.5, 0.013, Handle::new(1), Handle::new(2)));
        let info = data::read_pipe_info(&e).unwrap();
        assert!((info.diameter - 1.5).abs() < 1e-9);
    }
}