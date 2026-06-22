//! Lenient-parse warnings before analysis (review Issue 13).

use acadrust::EntityType;

use super::data::{self, APP_CATCHMENT, APP_PIPE, APP_STRUCT};

#[derive(Debug, Default)]
pub struct ValidationReport {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl ValidationReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn emit_to_host(&self, host: &mut dyn ocs_plugin_api::host::HostApi) {
        for w in &self.warnings {
            host.push_info(&format!("HydroComplete warning: {w}"));
        }
        for e in &self.errors {
            host.push_error(&format!("HydroComplete: {e}"));
        }
    }
}

pub fn validate_entities<'a>(entities: impl Iterator<Item = &'a EntityType>) -> ValidationReport {
    let ents: Vec<&EntityType> = entities.collect();
    let mut report = ValidationReport::default();
    let mut struct_handles = std::collections::HashSet::new();
    let mut struct_count = 0usize;
    let mut pipe_count = 0usize;

    for e in &ents {
        let h = e.common().handle;

        if let Some(s) = data::read_structure_info(e) {
            struct_count += 1;
            struct_handles.insert(h);
            if s.rim <= s.invert {
                report.warnings.push(format!(
                    "Handle {}: rim ({:.2}) <= invert ({:.2})",
                    h.value(),
                    s.rim,
                    s.invert
                ));
            }
            if s.kind != stormsewer::network::NodeKind::Outfall {
                if s.area <= 0.0 {
                    report.warnings.push(format!(
                        "Handle {}: zero contributing area on {}",
                        h.value(),
                        data::kind_str(s.kind)
                    ));
                }
                if s.c <= 0.0 || s.c > 1.0 {
                    report.warnings.push(format!(
                        "Handle {}: unusual runoff coefficient C={:.2}",
                        h.value(),
                        s.c
                    ));
                }
            }
            continue;
        }

        // Partial / malformed tags (non-structure entities)
        if e.common().extended_data.get_record(APP_STRUCT).is_some() {
            report.warnings.push(format!(
                "Handle {}: incomplete HYDROCOMPLETE_STRUCT XDATA",
                h.value()
            ));
        }
        if e.common().extended_data.get_record(APP_PIPE).is_some() {
            report.warnings.push(format!(
                "Handle {}: incomplete HYDROCOMPLETE_PIPE XDATA",
                h.value()
            ));
        }
        if e.common().extended_data.get_record(APP_CATCHMENT).is_some() {
            report.warnings.push(format!(
                "Handle {}: incomplete HYDROCOMPLETE_CATCHMENT XDATA",
                h.value()
            ));
        }
    }

    for e in &ents {
        let h = e.common().handle;
        if let Some(p) = data::read_pipe_info(e) {
            pipe_count += 1;
            if p.diameter <= 0.0 {
                report.errors.push(format!("Handle {}: pipe diameter <= 0", h.value()));
            }
            if p.n <= 0.0 {
                report.warnings.push(format!("Handle {}: Manning n <= 0", h.value()));
            }
            if !struct_handles.contains(&p.from) {
                report.warnings.push(format!(
                    "Handle {}: pipe references missing from-structure {}",
                    h.value(),
                    p.from.value()
                ));
            }
            if !struct_handles.contains(&p.to) {
                report.warnings.push(format!(
                    "Handle {}: pipe references missing to-structure {}",
                    h.value(),
                    p.to.value()
                ));
            }
        }
    }

    if struct_count == 0 {
        report.errors.push("No storm-sewer structures in drawing".into());
    }
    if struct_count > 0 && pipe_count == 0 {
        report.warnings.push("Structures found but no tagged pipes".into());
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use acadrust::types::Vector3;
    use acadrust::{Circle, EntityType, Handle, Line};
    use stormsewer::network::NodeKind;

    use crate::data::{pipe_xdata, structure_xdata};

    #[test]
    fn catches_rim_below_invert() {
        let mut e = EntityType::Circle(Circle {
            center: Vector3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            ..Default::default()
        });
        e.common_mut().handle = Handle::new(1);
        e.common_mut()
            .extended_data
            .add_record(structure_xdata(NodeKind::Inlet, 110.0, 105.0, 1.0, 0.7));
        let r = validate_entities(std::iter::once(&e));
        assert!(!r.warnings.is_empty());
    }

    #[test]
    fn errors_without_structures() {
        let r = validate_entities(std::iter::empty());
        assert!(!r.ok());
    }

    #[test]
    fn warns_dangling_pipe_handle() {
        let mut p = EntityType::Line(Line::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(10.0, 0.0, 0.0),
        ));
        p.common_mut().handle = Handle::new(9);
        p.common_mut()
            .extended_data
            .add_record(pipe_xdata(1.5, 0.013, Handle::new(1), Handle::new(2)));
        let r = validate_entities(std::iter::once(&p));
        assert!(r.warnings.len() >= 2);
    }
}