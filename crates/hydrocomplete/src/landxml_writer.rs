//! LandXML 1.2 storm sewer export.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandXmlPipeShape {
    Circular,
    Box,
}

#[derive(Debug, Clone)]
pub struct LandXmlPipeRecord {
    pub name: String,
    pub network_name: String,
    pub length_ft: f64,
    pub diameter_ft: f64,
    pub slope: f64,
    pub start_invert_ft: f64,
    pub end_invert_ft: f64,
    pub manning_n: f64,
    pub design_flow_cfs: Option<f64>,
    pub start_structure_name: String,
    pub end_structure_name: String,
    pub shape: LandXmlPipeShape,
    pub width_ft: f64,
    pub height_ft: f64,
}

#[derive(Debug, Clone)]
pub struct LandXmlStructureRecord {
    pub name: String,
    pub network_name: String,
    pub rim_ft: Option<f64>,
    pub invert_ft: Option<f64>,
    pub northing_ft: Option<f64>,
    pub easting_ft: Option<f64>,
    pub diameter_ft: Option<f64>,
}

pub fn write_to_string(
    pipes: &[LandXmlPipeRecord],
    structures: Option<&[LandXmlStructureRecord]>,
    project_name: Option<&str>,
) -> String {
    let project = project_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("HydroComplete");
    let mut sb = String::new();
    sb.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    sb.push('\n');
    sb.push_str(r#"<LandXML xmlns="http://www.landxml.org/schema/LandXML-1.2" version="1.2">"#);
    sb.push('\n');
    sb.push_str("  <Units><Imperial areaUnit=\"squareFoot\" linearUnit=\"foot\" volumeUnit=\"cubicYard\" temperatureUnit=\"fahrenheit\" pressureUnit=\"PSI\" diameterUnit=\"foot\"/></Units>\n");
    sb.push_str(&format!("  <Project name=\"{}\">\n", xml_escape(project)));
    sb.push_str(&format!("    <PipeNetworks name=\"{} Networks\">\n", xml_escape(project)));

    let mut networks: std::collections::BTreeMap<String, Vec<&LandXmlPipeRecord>> =
        std::collections::BTreeMap::new();
    for pipe in pipes {
        let net = if pipe.network_name.trim().is_empty() {
            "Network".to_string()
        } else {
            pipe.network_name.trim().to_string()
        };
        networks.entry(net).or_default().push(pipe);
    }

    for (network_name, net_pipes) in &networks {
        sb.push_str(&format!(
            "      <PipeNetwork name=\"{}\" pipeNetType=\"storm\">\n",
            xml_escape(network_name)
        ));
        sb.push_str("        <Structs>\n");
        let net_structs = filter_structures_for_network(structures, network_name, net_pipes);
        for structure in &net_structs {
            write_structure(&mut sb, structure, net_pipes);
        }
        sb.push_str("        </Structs>\n");
        sb.push_str("        <Pipes>\n");
        for pipe in net_pipes {
            write_pipe(&mut sb, pipe);
        }
        sb.push_str("        </Pipes>\n");
        sb.push_str("      </PipeNetwork>\n");
    }

    sb.push_str("    </PipeNetworks>\n");
    sb.push_str("  </Project>\n");
    sb.push_str("</LandXML>\n");
    sb
}

pub fn write_file(
    path: &std::path::Path,
    pipes: &[LandXmlPipeRecord],
    structures: Option<&[LandXmlStructureRecord]>,
    project_name: Option<&str>,
) -> std::io::Result<()> {
    crate::output_paths::write_file(path, &write_to_string(pipes, structures, project_name))
}

fn write_structure(sb: &mut String, structure: &LandXmlStructureRecord, pipes: &[&LandXmlPipeRecord]) {
    let struct_name = resolve_structure_name(&structure.name, &structure.network_name);
    let sump = structure
        .invert_ft
        .or_else(|| lowest_connected_invert(&struct_name, pipes));
    sb.push_str(&format!("          <Struct name=\"{}\"" , xml_escape(&struct_name)));
    if let Some(rim) = structure.rim_ft {
        sb.push_str(&format!(" elevRim=\"{:.4}\"", rim));
    }
    if let Some(s) = sump {
        sb.push_str(&format!(" elevSump=\"{:.4}\"", s));
    }
    sb.push_str(">\n");
    if let (Some(n), Some(e)) = (structure.northing_ft, structure.easting_ft) {
        sb.push_str(&format!("            <Center>{:.4} {:.4}</Center>\n", n, e));
    }
    let diameter = structure.diameter_ft.filter(|d| *d > 0.0).unwrap_or(4.0);
    sb.push_str(&format!(
        "            <CircStruct diameter=\"{:.4}\"/>\n",
        diameter
    ));
    for pipe in pipes {
        let start = resolve_structure_name(&pipe.start_structure_name, &pipe.network_name);
        let end = resolve_structure_name(&pipe.end_structure_name, &pipe.network_name);
        let pipe_name = resolve_pipe_name(&pipe.name, &pipe.network_name);
        if start.eq_ignore_ascii_case(&struct_name) {
            write_invert(sb, pipe.start_invert_ft, "out", &pipe_name);
        }
        if end.eq_ignore_ascii_case(&struct_name) {
            write_invert(sb, pipe.end_invert_ft, "in", &pipe_name);
        }
    }
    sb.push_str("          </Struct>\n");
}

fn write_invert(sb: &mut String, elevation_ft: f64, flow_dir: &str, pipe_name: &str) {
    sb.push_str(&format!(
        "            <Invert elev=\"{:.4}\" flowDir=\"{}\" refPipe=\"{}\"/>\n",
        elevation_ft,
        flow_dir,
        xml_escape(pipe_name)
    ));
}

fn write_pipe(sb: &mut String, pipe: &LandXmlPipeRecord) {
    let pipe_name = resolve_pipe_name(&pipe.name, &pipe.network_name);
    let start = resolve_structure_name(&pipe.start_structure_name, &pipe.network_name);
    let end = resolve_structure_name(&pipe.end_structure_name, &pipe.network_name);
    sb.push_str(&format!("          <Pipe name=\"{}\"", xml_escape(&pipe_name)));
    if !start.is_empty() {
        sb.push_str(&format!(" refStart=\"{}\"", xml_escape(&start)));
    }
    if !end.is_empty() {
        sb.push_str(&format!(" refEnd=\"{}\"", xml_escape(&end)));
    }
    sb.push_str(&format!(" slope=\"{:.4}\"", pipe.slope));
    if let Some(q) = pipe.design_flow_cfs.filter(|q| *q > 0.0) {
        sb.push_str(&format!(" flow=\"{:.4}\"", q));
    }
    sb.push_str(">\n");
    if pipe.shape == LandXmlPipeShape::Box && pipe.width_ft > 0.0 && pipe.height_ft > 0.0 {
        sb.push_str(&format!(
            "            <BoxPipe width=\"{:.4}\" height=\"{:.4}\" manningsN=\"{:.4}\"",
            pipe.width_ft, pipe.height_ft, pipe.manning_n
        ));
        if pipe.length_ft > 0.0 {
            sb.push_str(&format!(" length=\"{:.4}\"", pipe.length_ft));
        }
        sb.push_str("/>\n");
    } else {
        sb.push_str(&format!(
            "            <CircPipe diameter=\"{:.4}\" manningsN=\"{:.4}\"",
            pipe.diameter_ft, pipe.manning_n
        ));
        if pipe.length_ft > 0.0 {
            sb.push_str(&format!(" length=\"{:.4}\"", pipe.length_ft));
        }
        sb.push_str("/>\n");
    }
    sb.push_str("          </Pipe>\n");
}

fn filter_structures_for_network(
    structures: Option<&[LandXmlStructureRecord]>,
    network_name: &str,
    network_pipes: &[&LandXmlPipeRecord],
) -> Vec<LandXmlStructureRecord> {
    let mut structure_names = std::collections::BTreeSet::new();
    for pipe in network_pipes {
        if !pipe.start_structure_name.trim().is_empty() {
            structure_names.insert(resolve_structure_name(
                &pipe.start_structure_name,
                &pipe.network_name,
            ));
        }
        if !pipe.end_structure_name.trim().is_empty() {
            structure_names.insert(resolve_structure_name(
                &pipe.end_structure_name,
                &pipe.network_name,
            ));
        }
    }

    let mut results = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    if let Some(structures) = structures {
        for structure in structures {
            if !structure.network_name.is_empty()
                && !structure.network_name.eq_ignore_ascii_case(network_name)
            {
                continue;
            }
            let name = resolve_structure_name(&structure.name, network_name);
            if !structure_names.contains(&name) || !seen.insert(name.clone()) {
                continue;
            }
            results.push(LandXmlStructureRecord {
                name,
                network_name: network_name.to_string(),
                rim_ft: structure.rim_ft,
                invert_ft: structure.invert_ft,
                northing_ft: structure.northing_ft,
                easting_ft: structure.easting_ft,
                diameter_ft: structure.diameter_ft,
            });
        }
    }
    for name in structure_names {
        if seen.insert(name.clone()) {
            results.push(LandXmlStructureRecord {
                name,
                network_name: network_name.to_string(),
                rim_ft: None,
                invert_ft: None,
                northing_ft: None,
                easting_ft: None,
                diameter_ft: None,
            });
        }
    }
    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

fn lowest_connected_invert(structure_name: &str, pipes: &[&LandXmlPipeRecord]) -> Option<f64> {
    let mut lowest: Option<f64> = None;
    for pipe in pipes {
        let start = resolve_structure_name(&pipe.start_structure_name, &pipe.network_name);
        let end = resolve_structure_name(&pipe.end_structure_name, &pipe.network_name);
        if start.eq_ignore_ascii_case(structure_name) {
            lowest = min_opt(lowest, pipe.start_invert_ft);
        }
        if end.eq_ignore_ascii_case(structure_name) {
            lowest = min_opt(lowest, pipe.end_invert_ft);
        }
    }
    lowest
}

fn min_opt(current: Option<f64>, value: f64) -> Option<f64> {
    Some(match current {
        Some(c) => c.min(value),
        None => value,
    })
}

fn resolve_pipe_name(name: &str, network_name: &str) -> String {
    if !name.trim().is_empty() {
        name.trim().to_string()
    } else if network_name.trim().is_empty() {
        "Pipe".into()
    } else {
        format!("{}-Pipe", network_name.trim())
    }
}

fn resolve_structure_name(name: &str, network_name: &str) -> String {
    if !name.trim().is_empty() {
        name.trim().to_string()
    } else if network_name.trim().is_empty() {
        "Structure".into()
    } else {
        format!("{}-Struct", network_name.trim())
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}