//! Plan-view pipe direction and deflection helpers.

pub fn flow_direction(
    upstream_x: f64,
    upstream_y: f64,
    downstream_x: f64,
    downstream_y: f64,
) -> (f64, f64) {
    let dx = downstream_x - upstream_x;
    let dy = downstream_y - upstream_y;
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1e-9 {
        (1.0, 0.0)
    } else {
        (dx / length, dy / length)
    }
}

pub fn deflection_degrees(in_dir_x: f64, in_dir_y: f64, out_dir_x: f64, out_dir_y: f64) -> f64 {
    let dot = (in_dir_x * out_dir_x + in_dir_y * out_dir_y).clamp(-1.0, 1.0);
    dot.acos().to_degrees()
}