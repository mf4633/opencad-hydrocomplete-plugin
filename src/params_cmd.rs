// Parse `HC_PARAMS` subcommands and update HydroTabState.

use stormsewer::idf::IdfCurve;
use stormsewer::params::StormAnalysisParams;

use super::state::HydroTabState;

fn parse_f64(s: &str) -> Result<f64, String> {
    s.trim()
        .replace(',', ".")
        .parse::<f64>()
        .map_err(|_| format!("`{s}` is not a number"))
}

/// Apply `HC_PARAMS …` tokens. Empty rest → show summary.
pub fn apply_params(state: &mut HydroTabState, rest: &str) -> Result<String, String> {
    let t: Vec<&str> = rest.split_whitespace().collect();
    if t.is_empty() {
        return Ok(format!("Storm params: {}", state.params.summary()));
    }
    let key = t[0].to_ascii_uppercase();
    match key.as_str() {
        "RP" | "RETURN" => {
            let rp: u32 = t
                .get(1)
                .ok_or("HC_PARAMS RP needs return period years (e.g. HC_PARAMS RP 25)")?
                .parse()
                .map_err(|_| "return period must be an integer year")?;
            state.params.idf.set_design_rp(rp);
            if state.params.idf.curve(rp).is_none() {
                // Seed a scaled curve from the design curve if missing.
                let base = state.params.idf.design_curve();
                let scale = (rp as f64 / 10.0).sqrt().max(1.0);
                state.params.idf.set_curve(rp, IdfCurve::new(base.a * scale, base.b, base.c));
            }
            Ok(format!("Design return period set to {rp} yr."))
        }
        "IDF" => {
            let (rp, ai) = if t.len() == 5 {
                let rp: u32 = t[1].parse().map_err(|_| "IDF return period must be integer")?;
                (rp, 2)
            } else if t.len() == 4 {
                (state.params.idf.design_rp, 1)
            } else {
                return Err("HC_PARAMS IDF [rp] <a> <b> <c>  (e.g. HC_PARAMS IDF 60 10 0.8)".into());
            };
            let a = parse_f64(t[ai])?;
            let b = parse_f64(t[ai + 1])?;
            let c = parse_f64(t[ai + 2])?;
            state.params.idf.set_curve(rp, IdfCurve::new(a, b, c));
            state.params.idf.set_design_rp(rp);
            state.preset_key = None;
            Ok(format!("IDF for {rp}-yr set: i = {a}/(t+{b})^{c}"))
        }
        "TAILWATER" | "TW" => {
            let v = t.get(1).map(|s| s.to_ascii_uppercase());
            match v.as_deref() {
                None => Err("HC_PARAMS TAILWATER <elev_ft> | NONE".into()),
                Some("NONE" | "FREE") => {
                    state.params.hydraulics.tailwater = None;
                    Ok("Tailwater: free outfall.".into())
                }
                Some(s) => {
                    let elev = parse_f64(s)?;
                    state.params.hydraulics.tailwater = Some(elev);
                    Ok(format!("Tailwater elevation set to {elev:.2} ft."))
                }
            }
        }
        "MINTC" => {
            let v = parse_f64(t.get(1).ok_or("HC_PARAMS MINTC <minutes>")?)?;
            state.params.hydraulics.min_tc = v;
            Ok(format!("Minimum Tc set to {v:.1} min."))
        }
        "JUNCTIONK" | "JK" => {
            let v = parse_f64(t.get(1).ok_or("HC_PARAMS JUNCTIONK <k>")?)?;
            state.params.hydraulics.junction_k = v;
            Ok(format!("Junction loss K set to {v:.2}."))
        }
        "VMIN" => {
            let v = parse_f64(t.get(1).ok_or("HC_PARAMS VMIN <ft/s>")?)?;
            state.params.sizing.min_velocity = v;
            Ok(format!("Minimum velocity set to {v:.2} ft/s."))
        }
        "VMAX" => {
            let v = parse_f64(t.get(1).ok_or("HC_PARAMS VMAX <ft/s>")?)?;
            state.params.sizing.max_velocity = v;
            Ok(format!("Maximum velocity set to {v:.2} ft/s."))
        }
        "MAXFULL" | "PFULL" => {
            let v = parse_f64(t.get(1).ok_or("HC_PARAMS MAXFULL <percent>")?)?;
            state.params.sizing.max_pct_full = (v / 100.0).clamp(0.1, 1.0);
            Ok(format!("Max % full set to {v:.0}%."))
        }
        "INLETLEN" | "GRATE" => {
            let v = parse_f64(t.get(1).ok_or("HC_PARAMS INLETLEN <ft>")?)?;
            state.params.inlet_grate_length_ft = v;
            Ok(format!("Inlet grate length set to {v:.2} ft."))
        }
        "INLETD" | "CURBDEPTH" => {
            let v = parse_f64(t.get(1).ok_or("HC_PARAMS INLETD <ft>")?)?;
            state.params.inlet_flow_depth_ft = v;
            Ok(format!("Inlet curb flow depth set to {v:.3} ft."))
        }
        "INLETS" | "GUTTERS" => {
            let v = parse_f64(t.get(1).ok_or("HC_PARAMS INLETS <ft/ft>")?)?;
            state.params.inlet_gutter_slope = v;
            Ok(format!("Inlet gutter slope set to {v:.4} ft/ft."))
        }
        "PRESET" => {
            let preset_key = t
                .get(1)
                .ok_or("HC_PARAMS PRESET <key> [return_period]  (e.g. HC_PARAMS PRESET charlotte-nc 10)")?;
            let rp: i32 = t.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
            let preset = hydrocomplete::atlas14_presets::find(preset_key)
                .ok_or_else(|| format!("Unknown Atlas 14 preset `{preset_key}`. Run HC_ATLAS14."))?;
            let curve = preset.to_curve(rp)?;
            let rp_u = rp as u32;
            state.params.idf.set_curve(rp_u, curve);
            state.params.idf.set_design_rp(rp_u);
            state.preset_key = Some(preset_key.to_string());
            Ok(format!(
                "Atlas 14 preset {preset_key} ({rp}-yr): i = {:.2}/(t+{:.2})^{:.3}",
                preset.a(),
                preset.b(),
                preset.c(),
            ))
        }
        "LIVE" => {
            let lat = parse_f64(t.get(1).ok_or("HC_PARAMS LIVE <lat> <lon> [return_period]")?)?;
            let lon = parse_f64(t.get(2).ok_or("HC_PARAMS LIVE <lat> <lon> [return_period]")?)?;
            let rp: i32 = t.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);
            let fetcher = hydrocomplete::atlas14_fetcher::Atlas14Fetcher::new(Some(
                hydrocomplete::atlas14_fetcher::default_cache_directory(),
            ));
            let res = fetcher.resolve_with_fallback(lat, lon, rp);
            let curve = res.to_curve();
            let rp_u = rp as u32;
            state.params.idf.set_curve(rp_u, curve);
            state.params.idf.set_design_rp(rp_u);
            state.preset_key = None;
            Ok(format!(
                "Atlas 14 {} ({rp}-yr, {}): i = {:.2}/(t+{:.2})^{:.3}",
                res.display_label,
                res.source.as_str(),
                res.a,
                res.b,
                res.c,
            ))
        }
        "RESET" => {
            state.params = StormAnalysisParams::municipal();
            state.preset_key = None;
            Ok("Storm params reset to municipal defaults.".into())
        }
        _ => Err(format!(
            "Unknown HC_PARAMS key `{key}`. Keys: RP, IDF, PRESET, LIVE, TAILWATER, MINTC, JUNCTIONK, VMIN, VMAX, MAXFULL, INLETLEN, INLETD, INLETS, RESET"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_return_period() {
        let mut s = HydroTabState::default();
        apply_params(&mut s, "RP 25").unwrap();
        assert_eq!(s.params.idf.design_rp, 25);
    }

    #[test]
    fn sets_idf_coefficients() {
        let mut s = HydroTabState::default();
        apply_params(&mut s, "IDF 70 12 0.75").unwrap();
        let c = s.params.idf.design_curve();
        assert!((c.a - 70.0).abs() < 1e-9);
    }
}