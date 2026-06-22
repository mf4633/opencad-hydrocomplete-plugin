//! Chainage-elevation profile export to ASCII DXF.

pub const INVERT_LAYER: &str = "HC-PROFILE-INVERT";
pub const CROWN_LAYER: &str = "HC-PROFILE-CROWN";
pub const HGL_LAYER: &str = "HC-PROFILE-HGL";
pub const LABEL_LAYER: &str = "HC-PROFILE-LABEL";

#[derive(Debug, Clone)]
pub struct ProfilePoint {
    pub chainage_ft: f64,
    pub elevation_ft: f64,
}

#[derive(Debug, Clone)]
pub struct ProfileStation {
    pub chainage_ft: f64,
    pub structure_name: String,
    pub invert_ft: f64,
    pub crown_ft: f64,
    pub hgl_ft: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct ProfileDxfData {
    pub network_name: String,
    pub invert_points: Vec<ProfilePoint>,
    pub crown_points: Vec<ProfilePoint>,
    pub hgl_points: Vec<ProfilePoint>,
    pub stations: Vec<ProfileStation>,
}

#[derive(Debug, Clone)]
pub struct ProfileDxfOptions {
    pub origin_x: f64,
    pub origin_y: f64,
    pub datum_elevation_ft: f64,
    pub horizontal_scale: f64,
    pub vertical_scale: f64,
    pub include_hgl: bool,
    pub text_height: f64,
}

impl Default for ProfileDxfOptions {
    fn default() -> Self {
        Self {
            origin_x: 0.0,
            origin_y: 0.0,
            datum_elevation_ft: 0.0,
            horizontal_scale: 20.0,
            vertical_scale: 20.0,
            include_hgl: false,
            text_height: 0.1,
        }
    }
}

pub fn write_to_string(data: &ProfileDxfData, options: &ProfileDxfOptions) -> String {
    let h_scale = options.horizontal_scale.max(1e-6);
    let v_scale = options.vertical_scale.max(1e-6);
    let text_h = options.text_height.max(1e-6);

    let mut sb = String::with_capacity(4096);
    write_line(&mut sb, 0, "SECTION");
    write_line(&mut sb, 2, "HEADER");
    write_line(&mut sb, 9, "$ACADVER");
    write_line(&mut sb, 1, "AC1014");
    write_line(&mut sb, 9, "$INSUNITS");
    write_line(&mut sb, 70, "1");
    write_line(&mut sb, 0, "ENDSEC");

    write_line(&mut sb, 0, "SECTION");
    write_line(&mut sb, 2, "TABLES");
    write_table(&mut sb, "LAYER", build_layers(options));
    write_line(&mut sb, 0, "ENDSEC");

    write_line(&mut sb, 0, "SECTION");
    write_line(&mut sb, 2, "ENTITIES");

    if data.invert_points.len() >= 2 {
        write_lw_polyline(&mut sb, INVERT_LAYER, &data.invert_points, options, h_scale, v_scale);
    }
    if data.crown_points.len() >= 2 {
        write_lw_polyline(&mut sb, CROWN_LAYER, &data.crown_points, options, h_scale, v_scale);
    }
    if options.include_hgl && data.hgl_points.len() >= 2 {
        write_lw_polyline(&mut sb, HGL_LAYER, &data.hgl_points, options, h_scale, v_scale);
    }

    for station in &data.stations {
        let (x, y) = to_dxf(station.chainage_ft, station.invert_ft, options, h_scale, v_scale);
        let hgl = station
            .hgl_ft
            .map(|h| format!("\\nHGL {h:.2}"))
            .unwrap_or_default();
        let text = format!(
            "{}\\nSTA {:.1}{}",
            station.structure_name, station.chainage_ft, hgl
        );
        write_text(&mut sb, LABEL_LAYER, x, y, text_h, &text);
    }

    write_line(&mut sb, 0, "ENDSEC");
    write_line(&mut sb, 0, "EOF");
    sb
}

pub fn write_file(path: &std::path::Path, data: &ProfileDxfData, options: &ProfileDxfOptions) -> std::io::Result<()> {
    let dxf = write_to_string(data, options);
    crate::output_paths::write_file(path, &dxf)
}

fn build_layers(options: &ProfileDxfOptions) -> Vec<&'static str> {
    let mut layers = vec![INVERT_LAYER, CROWN_LAYER, LABEL_LAYER];
    if options.include_hgl {
        layers.push(HGL_LAYER);
    }
    layers
}

fn write_table(sb: &mut String, table_name: &str, layer_names: Vec<&str>) {
    write_line(sb, 0, "TABLE");
    write_line(sb, 2, table_name);
    write_line(sb, 70, &layer_names.len().to_string());
    for layer in layer_names {
        write_line(sb, 0, "LAYER");
        write_line(sb, 2, layer);
        write_line(sb, 70, "0");
        write_line(sb, 62, "7");
        write_line(sb, 6, "CONTINUOUS");
    }
    write_line(sb, 0, "ENDTAB");
}

fn write_lw_polyline(
    sb: &mut String,
    layer: &str,
    points: &[ProfilePoint],
    options: &ProfileDxfOptions,
    h_scale: f64,
    v_scale: f64,
) {
    write_line(sb, 0, "LWPOLYLINE");
    write_line(sb, 8, layer);
    write_line(sb, 90, &points.len().to_string());
    write_line(sb, 70, "0");
    for pt in points {
        let (x, y) = to_dxf(pt.chainage_ft, pt.elevation_ft, options, h_scale, v_scale);
        write_line(sb, 10, &format!("{x:.6}"));
        write_line(sb, 20, &format!("{y:.6}"));
    }
}

fn write_text(sb: &mut String, layer: &str, x: f64, y: f64, height: f64, text: &str) {
    write_line(sb, 0, "TEXT");
    write_line(sb, 8, layer);
    write_line(sb, 10, &format!("{x:.6}"));
    write_line(sb, 20, &format!("{y:.6}"));
    write_line(sb, 30, "0");
    write_line(sb, 40, &format!("{height:.6}"));
    write_line(sb, 1, text);
}

fn to_dxf(
    chainage_ft: f64,
    elevation_ft: f64,
    options: &ProfileDxfOptions,
    h_scale: f64,
    v_scale: f64,
) -> (f64, f64) {
    let x = options.origin_x + chainage_ft / h_scale;
    let y = options.origin_y + (elevation_ft - options.datum_elevation_ft) / v_scale;
    (x, y)
}

fn write_line(sb: &mut String, code: i32, value: &str) {
    sb.push_str(&code.to_string());
    sb.push('\n');
    sb.push_str(value);
    sb.push('\n');
}