//! Coordinate- and handle-based structure/pipe placement (v0.2 — no interactive pick).

use acadrust::types::Vector3;
use acadrust::{Circle, EntityType, Handle, Line};
use ocs_plugin_api::host::HostApi;

use stormsewer::network::NodeKind;

use super::data::{self, nearest_structure_at_point, pipe_xdata, structure_xdata};

const DEFAULT_INVERT: f64 = 100.0;
const DEFAULT_RIM: f64 = 106.0;
const DEFAULT_AREA: f64 = 1.0;
const DEFAULT_C: f64 = 0.7;
const DEFAULT_DIAMETER: f64 = 1.5;
const DEFAULT_N: f64 = 0.013;
const PICK_PADDING_FT: f64 = 5.0;

pub fn usage_inlet() -> &'static str {
    "HC_INLET <x>,<y> [invert] [rim] [area] [C]  — e.g. HC_INLET 100,200 104 110 1.0 0.7"
}

pub fn usage_junction() -> &'static str {
    "HC_JUNCTION <x>,<y> [invert] [rim] [area] [C]"
}

pub fn usage_outfall() -> &'static str {
    "HC_OUTFALL <x>,<y> [invert] [rim]  — e.g. HC_OUTFALL 500,0 100 106"
}

pub fn usage_pipe_handles() -> &'static str {
    "HC_PIPE <from_handle> <to_handle> [diameter] [n]  — e.g. HC_PIPE 1 2 1.5 0.013"
}

pub fn usage_pipe_coords() -> &'static str {
    "HC_PIPE <x1>,<y1> <x2>,<y2> [diameter] [n]  — snaps to nearest structures"
}

fn parse_num(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok()
}

fn parse_handle(s: &str) -> Option<Handle> {
    let v = s.trim().parse::<u64>().ok()?;
    Some(Handle::new(v))
}

/// Parse `100,200` or two tokens `100` `200`.
fn parse_xy(tokens: &[&str]) -> Option<(f64, f64)> {
    if tokens.is_empty() {
        return None;
    }
    if tokens[0].contains(',') {
        let parts: Vec<&str> = tokens[0].split(',').collect();
        if parts.len() == 2 {
            return Some((parse_num(parts[0])?, parse_num(parts[1])?));
        }
    }
    if tokens.len() >= 2 {
        return Some((parse_num(tokens[0])?, parse_num(tokens[1])?));
    }
    None
}

fn default_radius(kind: NodeKind) -> f64 {
    match kind {
        NodeKind::Inlet => 3.0,
        NodeKind::Junction => 4.0,
        NodeKind::Outfall => 6.0,
    }
}

fn structure_entity(
    kind: NodeKind,
    x: f64,
    y: f64,
    invert: f64,
    rim: f64,
    area: f64,
    c: f64,
) -> EntityType {
    let mut e = EntityType::Circle(Circle {
        center: Vector3::new(x, y, 0.0),
        radius: default_radius(kind),
        ..Default::default()
    });
    let (area, c) = if kind == NodeKind::Outfall {
        (0.0, 0.0)
    } else {
        (area, c)
    };
    e.common_mut()
        .extended_data
        .add_record(structure_xdata(kind, invert, rim, area, c));
    e
}

pub fn place_structure(
    host: &mut dyn HostApi,
    kind: NodeKind,
    args: &str,
) -> Result<String, String> {
    let tokens: Vec<&str> = args.split_whitespace().collect();
    let (x, y) = parse_xy(&tokens).ok_or_else(|| {
        format!(
            "Expected coordinates. {}",
            match kind {
                NodeKind::Inlet => usage_inlet(),
                NodeKind::Junction => usage_junction(),
                NodeKind::Outfall => usage_outfall(),
            }
        )
    })?;

    let mut nums: Vec<f64> = Vec::new();
    let start = if tokens[0].contains(',') { 1 } else { 2 };
    for t in tokens.iter().skip(start) {
        if let Some(v) = parse_num(t) {
            nums.push(v);
        }
    }

    let (invert, rim, area, c) = if kind == NodeKind::Outfall {
        (
            nums.first().copied().unwrap_or(DEFAULT_INVERT),
            nums.get(1).copied().unwrap_or(DEFAULT_RIM),
            0.0,
            0.0,
        )
    } else {
        (
            nums.first().copied().unwrap_or(DEFAULT_INVERT),
            nums.get(1).copied().unwrap_or(DEFAULT_RIM),
            nums.get(2).copied().unwrap_or(DEFAULT_AREA),
            nums.get(3).copied().unwrap_or(DEFAULT_C),
        )
    };

    if rim <= invert {
        return Err(format!("rim ({rim}) must be above invert ({invert})"));
    }

    host.push_undo(match kind {
        NodeKind::Inlet => "HC_INLET",
        NodeKind::Junction => "HC_JUNCTION",
        NodeKind::Outfall => "HC_OUTFALL",
    });
    let ent = structure_entity(kind, x, y, invert, rim, area, c);
    let h = host.add_entity(ent);
    host.bump_geometry();
    host.set_dirty();

    Ok(format!(
        "Placed {} at ({x:.2}, {y:.2}) handle={} invert={invert:.2} rim={rim:.2}",
        data::kind_str(kind),
        h.value()
    ))
}

fn is_coordinate_mode(tokens: &[&str]) -> bool {
    if tokens.len() < 2 {
        return false;
    }
    if tokens[0].contains(',') || tokens[1].contains(',') {
        return true;
    }
    tokens.len() >= 4
        && parse_num(tokens[0]).is_some()
        && parse_num(tokens[1]).is_some()
        && parse_num(tokens[2]).is_some()
        && parse_num(tokens[3]).is_some()
}

pub fn place_pipe(host: &mut dyn HostApi, args: &str) -> Result<String, String> {
    let tokens: Vec<&str> = args.split_whitespace().collect();
    if tokens.len() < 2 {
        return Err(format!(
            "Expected two handles or two coordinate pairs. {} or {}",
            usage_pipe_handles(),
            usage_pipe_coords()
        ));
    }

    let coord_mode = is_coordinate_mode(&tokens);

    let (from_h, to_h, x0, y0, x1, y1, num_start) = if coord_mode {
        let (x0, y0, x1, y1, num_start) = if tokens[0].contains(',') || tokens[1].contains(',') {
            let (x0, y0) = parse_xy(&tokens[..1])
                .or_else(|| parse_xy(&tokens[..2]))
                .ok_or_else(|| format!("Bad start coordinates. {}", usage_pipe_coords()))?;
            let (x1, y1) = if tokens[1].contains(',') {
                parse_xy(&[tokens[1]])
                    .ok_or_else(|| format!("Bad end coordinates. {}", usage_pipe_coords()))?
            } else {
                parse_xy(&tokens[1..3])
                    .ok_or_else(|| format!("Bad end coordinates. {}", usage_pipe_coords()))?
            };
            let ns = if tokens[0].contains(',') && tokens[1].contains(',') {
                2
            } else if tokens[0].contains(',') {
                2
            } else {
                3
            };
            (x0, y0, x1, y1, ns)
        } else {
            (
                parse_num(tokens[0]).ok_or("bad x0")?,
                parse_num(tokens[1]).ok_or("bad y0")?,
                parse_num(tokens[2]).ok_or("bad x1")?,
                parse_num(tokens[3]).ok_or("bad y1")?,
                4,
            )
        };
        let from_h = nearest_structure_at_point(
            host.document().entities(),
            x0,
            y0,
            PICK_PADDING_FT,
            true,
        )
        .ok_or_else(|| format!("No structure near start ({x0:.2}, {y0:.2})"))?;
        let to_h = nearest_structure_at_point(
            host.document().entities(),
            x1,
            y1,
            PICK_PADDING_FT,
            true,
        )
        .ok_or_else(|| format!("No structure near end ({x1:.2}, {y1:.2})"))?;
        (from_h, to_h, x0, y0, x1, y1, num_start)
    } else {
        let from_h = parse_handle(tokens[0]).ok_or("Invalid from_handle")?;
        let to_h = parse_handle(tokens[1]).ok_or("Invalid to_handle")?;
        let (x0, y0, x1, y1) = pipe_endpoints_from_handles(host, from_h, to_h)?;
        (from_h, to_h, x0, y0, x1, y1, 2)
    };
    let nums: Vec<f64> = tokens
        .iter()
        .skip(num_start)
        .filter_map(|t| parse_num(t))
        .collect();
    let diameter = nums.first().copied().unwrap_or(DEFAULT_DIAMETER);
    let n = nums.get(1).copied().unwrap_or(DEFAULT_N);

    if from_h == to_h {
        return Err("Pipe start and end must be different structures".into());
    }

    host.push_undo("HC_PIPE");
    let mut e = EntityType::Line(Line::from_points(
        Vector3::new(x0, y0, 0.0),
        Vector3::new(x1, y1, 0.0),
    ));
    e.common_mut()
        .extended_data
        .add_record(pipe_xdata(diameter, n, from_h, to_h));
    let h = host.add_entity(e);
    host.bump_geometry();
    host.set_dirty();

    Ok(format!(
        "Pipe handle={} from={} to={} dia={diameter:.2} n={n:.3}",
        h.value(),
        from_h.value(),
        to_h.value()
    ))
}

fn pipe_endpoints_from_handles(
    host: &dyn HostApi,
    from: Handle,
    to: Handle,
) -> Result<(f64, f64, f64, f64), String> {
    let mut from_xy = None;
    let mut to_xy = None;
    for e in host.document().entities() {
        let h = e.common().handle;
        if let Some(s) = data::read_structure_info(e) {
            if h == from {
                from_xy = Some((s.x, s.y));
            }
            if h == to {
                to_xy = Some((s.x, s.y));
            }
        }
    }
    let (x0, y0) = from_xy.ok_or_else(|| format!("Structure handle {} not found", from.value()))?;
    let (x1, y1) = to_xy.ok_or_else(|| format!("Structure handle {} not found", to.value()))?;
    Ok((x0, y0, x1, y1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_xy_comma_and_space() {
        assert_eq!(parse_xy(&["100,200"]), Some((100.0, 200.0)));
        assert_eq!(parse_xy(&["100", "200"]), Some((100.0, 200.0)));
    }

    #[test]
    fn structure_entity_tags_xdata() {
        let e = structure_entity(NodeKind::Inlet, 10.0, 20.0, 100.0, 106.0, 1.0, 0.7);
        assert!(data::is_structure_entity(&e));
    }
}