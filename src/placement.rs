//! Coordinate- and handle-based structure/pipe placement (v0.2 — no interactive pick).

use acadrust::types::Vector3;
use acadrust::{Circle, EntityType, Handle, Line};
use ocs_plugin_api::host::HostApi;

use stormsewer::network::NodeKind;

use super::data::{self, nearest_structure_at_point, pipe_xdata, structure_xdata};

const DEFAULT_INVERT: f64 = 100.0;
const DEFAULT_RIM: f64 = 106.0;
/// Default pipe slope (ft/ft) applied when stepping downstream inverts on placement.
const DEFAULT_PIPE_SLOPE: f64 = 0.01;
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
    "HC_PIPE <x1>,<y1> <x2>,<y2> [diameter] [n]  — or HC_PIPE x1:y1:x2:y2 (serve-safe)"
}

fn parse_num(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok()
}

fn parse_handle(s: &str) -> Option<Handle> {
    data::parse_entity_handle(s)
}

/// Diameter (ft) and Manning n from trailing tokens.
/// GUI: `1.25 0.013`. OCS `--serve`: `d15 n13` (15 in dia, n=0.013) or `d15n13`.
fn parse_pipe_hydraulics(tokens: &[&str], start: usize) -> (f64, f64) {
    let mut diameter = DEFAULT_DIAMETER;
    let mut n = DEFAULT_N;
    let mut have_dia = false;
    let mut have_n = false;

    for t in tokens.iter().skip(start) {
        let tl = t.to_ascii_lowercase();
        if let Some(rest) = tl.strip_prefix('d') {
            if let Some((din, n_milli)) = parse_d_n_compound(rest) {
                diameter = din as f64 / 12.0;
                n = n_milli as f64 / 1000.0;
                have_dia = true;
                have_n = true;
                continue;
            }
            if let Ok(inches) = rest.parse::<u32>() {
                diameter = inches as f64 / 12.0;
                have_dia = true;
                continue;
            }
        }
        if let Some(rest) = tl.strip_prefix('n') {
            if let Ok(milli) = rest.parse::<u32>() {
                n = milli as f64 / 1000.0;
                have_n = true;
                continue;
            }
        }
        if let Some(v) = parse_num(t) {
            if !have_dia {
                diameter = v;
                have_dia = true;
            } else if !have_n {
                n = v;
                have_n = true;
            }
        }
    }
    (diameter, n)
}

/// `15n13` → (15 in, n milli 13).
fn parse_d_n_compound(rest: &str) -> Option<(u32, u32)> {
    let pos = rest.find('n')?;
    let (din, n_part) = rest.split_at(pos);
    let n_part = &n_part[1..];
    Some((din.parse().ok()?, n_part.parse().ok()?))
}

pub fn usage_pipe_args() -> &'static str {
    "HC_PIPE_ARGS <from> <to> [diameter] [n]  — serve-safe; use d15 n13 (in, milli-n) when decimals are stripped"
}

/// Parse `100,200`, `100;200`, or two tokens `100` `200`.
fn parse_xy(tokens: &[&str]) -> Option<(f64, f64)> {
    if tokens.is_empty() {
        return None;
    }
    let sep = tokens[0].find([',', ';', ':']);
    if let Some(i) = sep {
        let (a, b) = tokens[0].split_at(i);
        let b = &b[1..];
        return Some((parse_num(a)?, parse_num(b)?));
    }
    if tokens.len() >= 2 {
        return Some((parse_num(tokens[0])?, parse_num(tokens[1])?));
    }
    None
}

/// `0:0:50:0` — single token avoids OCS `--serve` comma-splitting on `0,0 50,0`.
fn parse_xy_quad(token: &str) -> Option<(f64, f64, f64, f64)> {
    let parts: Vec<&str> = token.split([',', ';', ':']).collect();
    if parts.len() == 4 {
        return Some((
            parse_num(parts[0])?,
            parse_num(parts[1])?,
            parse_num(parts[2])?,
            parse_num(parts[3])?,
        ));
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
    if tokens.is_empty() {
        return false;
    }
    if tokens.len() == 1 && parse_xy_quad(tokens[0]).is_some() {
        return true;
    }
    if tokens.len() < 2 {
        return false;
    }
    if tokens[0].contains([',', ';', ':']) || tokens[1].contains([',', ';', ':']) {
        return true;
    }
    // `<handle> <handle> [dia] [n]` — not coordinates (fixes `43 44 1.25 0.013` mis-parse).
    if parse_handle(tokens[0]).is_some() && parse_handle(tokens[1]).is_some() {
        return false;
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
        let (x0, y0, x1, y1, num_start) = if tokens.len() == 1 {
            let (x0, y0, x1, y1) = parse_xy_quad(tokens[0])
                .ok_or_else(|| format!("Bad coordinates. {}", usage_pipe_coords()))?;
            (x0, y0, x1, y1, 1)
        } else if tokens[0].contains([',', ';', ':']) || tokens[1].contains([',', ';', ':']) {
            let (x0, y0) = parse_xy(&tokens[..1])
                .or_else(|| parse_xy(&tokens[..2]))
                .ok_or_else(|| format!("Bad start coordinates. {}", usage_pipe_coords()))?;
            let (x1, y1) = if tokens[1].contains([',', ';', ':']) {
                parse_xy(&[tokens[1]])
                    .ok_or_else(|| format!("Bad end coordinates. {}", usage_pipe_coords()))?
            } else {
                parse_xy(&tokens[1..3])
                    .ok_or_else(|| format!("Bad end coordinates. {}", usage_pipe_coords()))?
            };
            let ns = if tokens[0].contains([',', ';', ':']) && tokens[1].contains([',', ';', ':']) {
                2
            } else if tokens[0].contains([',', ';', ':']) {
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
    let (diameter, n) = parse_pipe_hydraulics(&tokens, num_start);



    if from_h == to_h {
        return Err("Pipe start and end must be different structures".into());
    }

    let (x0, y0, x1, y1) = pipe_endpoints_from_handles(host, from_h, to_h)?;
    let length = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();

    host.push_undo("HC_PIPE");
    let invert_note = step_downstream_invert_if_flat(host, from_h, to_h, length)?;
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

    let mut msg = format!(
        "Pipe handle={} from={} to={} dia={diameter:.2} n={n:.3} L={length:.0}",
        h.value(),
        from_h.value(),
        to_h.value()
    );

    if let Some(note) = invert_note {
        msg.push_str(&format!(" ({note})"));
    }
    Ok(msg)
}

/// When downstream invert is not lower than upstream, step it down by default pipe slope.
fn step_downstream_invert_if_flat(
    host: &mut dyn HostApi,
    from_h: Handle,
    to_h: Handle,
    length_ft: f64,
) -> Result<Option<String>, String> {
    if length_ft <= 0.0 {
        return Ok(None);
    }
    let from_info = structure_info_by_handle(host, from_h)?;
    let drop = DEFAULT_PIPE_SLOPE * length_ft;
    let target_invert = from_info.invert - drop;
    let mut to_ent = find_structure_mut(host, to_h)?;
    let mut to_info = data::read_structure_info(&to_ent)
        .ok_or_else(|| format!("Structure handle {} not found", to_h.value()))?;
    if to_info.invert > from_info.invert - drop + 1e-6 {
        to_info.invert = target_invert;
        if to_info.rim <= to_info.invert {
            to_info.rim = to_info.invert + (DEFAULT_RIM - DEFAULT_INVERT);
        }
        data::write_structure_info(&mut to_ent, &to_info);
        host.set_dirty();
        host.bump_geometry();
        return Ok(Some(format!(
            "downstream invert stepped to {:.2} ({DEFAULT_PIPE_SLOPE:.1}% slope)",
            to_info.invert
        )));
    }
    Ok(None)
}

fn structure_info_by_handle(host: &dyn HostApi, handle: Handle) -> Result<data::StructureInfo, String> {
    for e in host.document().entities() {
        if e.common().handle == handle {
            return data::read_structure_info(e)
                .ok_or_else(|| format!("Handle {} is not a structure", handle.value()));
        }
    }
    Err(format!("Structure handle {} not found", handle.value()))
}

fn find_structure_mut<'a>(
    host: &'a mut dyn HostApi,
    handle: Handle,
) -> Result<&'a mut EntityType, String> {
    host.document_mut()
        .entities_mut()
        .find(|e| e.common().handle == handle)
        .ok_or_else(|| format!("Structure handle {} not found", handle.value()))
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

    #[test]
    fn parse_handle_decimal_and_hex() {
        assert_eq!(parse_handle("43").unwrap().value(), 43);
        assert_eq!(parse_handle("2B").unwrap().value(), 43);
        assert_eq!(parse_handle("2b").unwrap().value(), 43);
        assert!(parse_handle("0").is_none());
    }

    #[test]
    fn four_token_handle_command_is_not_coordinate_mode() {
        let tokens: Vec<&str> = "43 44 1.25 0.013".split_whitespace().collect();
        assert!(!is_coordinate_mode(&tokens));
        let hex: Vec<&str> = "2B 2C 1.25 0.013".split_whitespace().collect();
        assert!(!is_coordinate_mode(&hex));
        let coords: Vec<&str> = "0 0 50 0".split_whitespace().collect();
        assert!(is_coordinate_mode(&coords));
    }

    #[test]
    fn quad_coord_token_parses() {
        assert_eq!(parse_xy_quad("0:0:50:0"), Some((0.0, 0.0, 50.0, 0.0)));
        assert!(is_coordinate_mode(&["0:0:50:0"]));
    }

    #[test]
    fn semicolon_xy_parses() {
        assert_eq!(parse_xy(&["0;0"]), Some((0.0, 0.0)));
    }

    #[test]
    fn handle_mode_diameter_parsing() {
        let tokens: Vec<&str> = "2B 2C 1.25 0.013".split_whitespace().collect();
        let (d, n) = parse_pipe_hydraulics(&tokens, 2);
        assert!((d - 1.25).abs() < 1e-9);
        assert!((n - 0.013).abs() < 1e-9);
    }

    #[test]
    fn serve_safe_diameter_inches_and_milli_n() {
        let tokens: Vec<&str> = "2B 2C d15 n13".split_whitespace().collect();
        let (d, n) = parse_pipe_hydraulics(&tokens, 2);
        assert!((d - 1.25).abs() < 1e-9);
        assert!((n - 0.013).abs() < 1e-9);
    }

    #[test]
    fn serve_safe_d15n13_compound() {
        let tokens: Vec<&str> = "2B 2C d15n13".split_whitespace().collect();
        let (d, n) = parse_pipe_hydraulics(&tokens, 2);
        assert!((d - 1.25).abs() < 1e-9);
        assert!((n - 0.013).abs() < 1e-9);
    }
}