//! Viewport click-to-place via `ocs_plugin_api::host::InteractiveCommand` (API v2).
//!
//! Ribbon tools call these through `HostApi::start_interactive`. The same flow
//! works over `--serve` when coordinates or handles are fed to `run`.

use acadrust::types::Vector3;
use acadrust::{Circle, EntityType, Handle, Line};
use ocs_plugin_api::host::{CommandStep, InteractiveCommand};

use stormsewer::network::NodeKind;

use super::data::{self, pipe_xdata, structure_xdata};

const DEFAULT_INVERT: f64 = 100.0;
const DEFAULT_RIM: f64 = 106.0;
const DEFAULT_AREA: f64 = 1.0;
const DEFAULT_C: f64 = 0.7;
const DEFAULT_DIAMETER: f64 = 1.5;
const DEFAULT_N: f64 = 0.013;

fn default_radius(kind: NodeKind) -> f64 {
    match kind {
        NodeKind::Inlet => 3.0,
        NodeKind::Junction => 4.0,
        NodeKind::Outfall => 6.0,
    }
}

/// Click once to place a structure circle with default hydraulics.
pub struct PlaceStructureInteractive {
    kind: NodeKind,
    radius: f64,
    invert: f64,
    rim: f64,
    area: f64,
    c: f64,
}

impl PlaceStructureInteractive {
    pub fn inlet() -> Self {
        Self::new(NodeKind::Inlet)
    }
    pub fn junction() -> Self {
        Self::new(NodeKind::Junction)
    }
    pub fn outfall() -> Self {
        Self::new(NodeKind::Outfall)
    }

    fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            radius: default_radius(kind),
            invert: DEFAULT_INVERT,
            rim: DEFAULT_RIM,
            area: DEFAULT_AREA,
            c: DEFAULT_C,
        }
    }
}

impl InteractiveCommand for PlaceStructureInteractive {
    fn prompt(&self) -> String {
        format!(
            "Click to place {} (invert {:.1}, rim {:.1}) — or type coordinates via command line:",
            data::kind_str(self.kind),
            self.invert,
            self.rim
        )
    }

    fn on_point(&mut self, pt: [f64; 3]) -> CommandStep {
        let circ = Circle {
            center: Vector3::new(pt[0], pt[1], pt[2]),
            radius: self.radius,
            ..Default::default()
        };
        let mut ent = EntityType::Circle(circ);
        let (area, c) = if self.kind == NodeKind::Outfall {
            (0.0, 0.0)
        } else {
            (self.area, self.c)
        };
        ent.common_mut()
            .extended_data
            .add_record(structure_xdata(self.kind, self.invert, self.rim, area, c));
        CommandStep::CommitAndEnd(ent)
    }
}

enum PipeStep {
    PickStart,
    PickEnd,
}

/// Pick two structures (object pick) to draw a tagged pipe between them.
pub struct PlacePipeInteractive {
    step: PipeStep,
    diameter: f64,
    n: f64,
    start_handle: Option<Handle>,
    start_xy: (f64, f64),
}

impl PlacePipeInteractive {
    pub fn new() -> Self {
        Self {
            step: PipeStep::PickStart,
            diameter: DEFAULT_DIAMETER,
            n: DEFAULT_N,
            start_handle: None,
            start_xy: (0.0, 0.0),
        }
    }
}

impl Default for PlacePipeInteractive {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractiveCommand for PlacePipeInteractive {
    fn prompt(&self) -> String {
        match self.step {
            PipeStep::PickStart => {
                "Pipe run: click START structure (or run HC_PIPE <from> <to>):".into()
            }
            PipeStep::PickEnd => format!(
                "Pipe run: click END structure (dia {:.2} ft, n {:.3}):",
                self.diameter, self.n
            ),
        }
    }

    fn needs_object_pick(&self) -> bool {
        true
    }

    fn on_point(&mut self, _pt: [f64; 3]) -> CommandStep {
        CommandStep::NeedPoint
    }

    fn on_object_pick(&mut self, handle: Handle, pt: [f64; 3]) -> CommandStep {
        match self.step {
            PipeStep::PickStart => {
                self.start_handle = Some(handle);
                self.start_xy = (pt[0], pt[1]);
                self.step = PipeStep::PickEnd;
                CommandStep::NeedPoint
            }
            PipeStep::PickEnd => {
                let from = self.start_handle.unwrap_or(Handle::new(0));
                if from == handle {
                    return CommandStep::Cancel;
                }
                let line = Line::from_points(
                    Vector3::new(self.start_xy.0, self.start_xy.1, 0.0),
                    Vector3::new(pt[0], pt[1], 0.0),
                );
                let mut ent = EntityType::Line(line);
                ent.common_mut()
                    .extended_data
                    .add_record(pipe_xdata(self.diameter, self.n, from, handle));
                CommandStep::CommitAndEnd(ent)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure_click_commits_circle() {
        let mut cmd = PlaceStructureInteractive::inlet();
        match cmd.on_point([10.0, 20.0, 0.0]) {
            CommandStep::CommitAndEnd(EntityType::Circle(c)) => {
                assert!((c.center.x - 10.0).abs() < 1e-9);
                assert!(data::is_structure_entity(&EntityType::Circle(c)));
            }
            _ => panic!("expected CommitAndEnd(Circle)"),
        }
    }

    #[test]
    fn pipe_two_picks_commit_line() {
        let mut cmd = PlacePipeInteractive::new();
        assert!(cmd.needs_object_pick());
        assert!(matches!(
            cmd.on_object_pick(Handle::new(1), [0.0, 0.0, 0.0]),
            CommandStep::NeedPoint
        ));
        match cmd.on_object_pick(Handle::new(2), [100.0, 0.0, 0.0]) {
            CommandStep::CommitAndEnd(EntityType::Line(l)) => {
                assert!((l.end.x - 100.0).abs() < 1e-9);
            }
            _ => panic!("expected CommitAndEnd(Line)"),
        }
    }
}