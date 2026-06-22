//! HydroComplete — external add-on for Open CAD Studio.
//!
//! Mirrors [`HydroComplete.Civil3D`](https://hydrocomplete.com/civil3d): stormwater
//! hydrology/hydraulics from drawing entities via XDATA + `HC_*` commands.
//!
//! Engine: `crates/hydrocomplete` (headless; extends `stormsewer`).
//! CAD bridge: `ocs_plugin_api` v2.

mod analysis;
mod analyze_full;
mod commands;
mod data;
mod dispatch;
mod edit;
mod interactive;
#[cfg(test)]
mod integration_tests;
mod landxml_import;
mod params_cmd;
mod placement;
mod report_export;
mod sizing;
mod state;
mod style;
mod validation;
mod write_labels;

use ocs_plugin_api::host::{BuiltinPlugin, HostApi};
use ocs_plugin_api::manifest::PluginManifest;
use ocs_plugin_api::ribbon::{CadModule, IconKind, ModuleEvent, RibbonGroup, RibbonItem, ToolDef};

pub mod manifest {
    use ocs_plugin_api::manifest::{ApiVersion, PluginManifest};

    pub const PLUGIN_ID: &str = "opencad.hydrocomplete";

    pub static MANIFEST: PluginManifest = PluginManifest {
        id: PLUGIN_ID,
        name: "HydroComplete",
        version: "0.2.0",
        description: "Stormwater hydrology and hydraulics — mirrors HydroComplete.Civil3D",
        api_version: ApiVersion::CURRENT,
        ribbon_order: 45,
        xdata_apps: &[
            "HYDROCOMPLETE_STRUCT",
            "HYDROCOMPLETE_PIPE",
            "HYDROCOMPLETE_CATCHMENT",
        ],
        command_prefixes: &["HC_"],
    };
}

use manifest::MANIFEST;

struct HydroCompleteModule;

fn tool(id: &'static str, label: &'static str, glyph: &'static str) -> ToolDef {
    ToolDef {
        id,
        label,
        icon: IconKind::Glyph(glyph),
        event: ModuleEvent::Command(id.to_string()),
    }
}

fn file_tool(id: &'static str, label: &'static str, glyph: &'static str, cmd: &str, title: &str) -> ToolDef {
    ToolDef {
        id,
        label,
        icon: IconKind::Glyph(glyph),
        event: ModuleEvent::PluginFileDialog {
            command: cmd.to_string(),
            title: title.to_string(),
            filter_name: "LandXML".to_string(),
            extensions: vec!["xml".to_string(), "landxml".to_string()],
        },
    }
}

impl CadModule for HydroCompleteModule {
    fn id(&self) -> &'static str {
        "hydrocomplete"
    }
    fn title(&self) -> &'static str {
        "HydroComplete"
    }

    fn ribbon_groups(&self) -> Vec<RibbonGroup> {
        vec![
            RibbonGroup {
                title: "Network",
                tools: vec![
                    RibbonItem::LargeTool(tool("HC_INLET", "Inlet", "◉")),
                    RibbonItem::LargeTool(tool("HC_JUNCTION", "Junction", "◎")),
                    RibbonItem::LargeTool(tool("HC_OUTFALL", "Outfall", "▽")),
                    RibbonItem::LargeTool(tool("HC_PIPE", "Pipe\nRun", "╱")),
                    RibbonItem::Tool(file_tool(
                        "HC_LANDXML_IMPORT",
                        "Import\nLandXML",
                        "⬇",
                        "HC_LANDXML_IMPORT",
                        "Import LandXML pipe network",
                    )),
                    RibbonItem::Tool(tool("HC_EDIT", "Edit", "✎")),
                    RibbonItem::Tool(tool("HC_NETWORK_EDIT", "Network\nEdit", "✎")),
                ],
            },
            RibbonGroup {
                title: "Analysis",
                tools: vec![
                    RibbonItem::LargeTool(tool("HC_ANALYZE", "Full\nAnalysis", "⚡")),
                    RibbonItem::LargeTool(tool("HC_PIPES", "Pipe\nCapacity", "⌀")),
                    RibbonItem::LargeTool(tool("HC_CAPACITY", "Design\nCapacity", "◎")),
                    RibbonItem::Tool(tool("HC_VALIDATE", "Validate", "✓")),
                    RibbonItem::Tool(tool("HC_SIZE", "Size\nPipes", "⌀")),
                    RibbonItem::Tool(tool("HC_HGL", "HGL\nProfile", "▤")),
                    RibbonItem::Tool(tool("HC_RATIONAL", "Rational\nQ", "Q")),
                    RibbonItem::Tool(tool("HC_MULTIRP", "Multi-RP", "≋")),
                    RibbonItem::Tool(tool("HC_REVIEW", "Design\nReview", "✓")),
                    RibbonItem::Tool(tool("HC_REPORT", "HTML\nReport", "📋")),
                ],
            },
            RibbonGroup {
                title: "Stormwater",
                tools: vec![
                    RibbonItem::Tool(tool("HC_SCS", "SCS\nRunoff", "R")),
                    RibbonItem::Tool(tool("HC_WQV", "Water\nQuality", "W")),
                    RibbonItem::Tool(tool("HC_DETENTION", "Detention", "⌁")),
                    RibbonItem::Tool(tool("HC_PREPOST", "Pre/Post\nPeaks", "↕")),
                    RibbonItem::Tool(tool("HC_ATLAS14", "Atlas 14\nIDF", "≋")),
                    RibbonItem::Tool(tool("HC_TC", "TR-55\nTc", "⏱")),
                    RibbonItem::Tool(tool("HC_INLETS", "Inlet\nCheck", "◉")),
                ],
            },
            RibbonGroup {
                title: "More",
                tools: vec![
                    RibbonItem::Tool(tool("HC_NETWORK", "Network\nSummary", "≡")),
                    RibbonItem::Tool(tool("HC_NETWORK_DIAGRAM", "Network\nDiagram", "◎")),
                    RibbonItem::Tool(tool("HC_CULVERT", "Culvert\nHW", "⌂")),
                    RibbonItem::Tool(tool("HC_ABOUT", "About", "?")),
                    RibbonItem::Tool(tool("HC_LICENSE", "License", "🔑")),
                ],
            },
        ]
    }
}

struct HydroCompletePlugin;

impl BuiltinPlugin for HydroCompletePlugin {
    fn manifest(&self) -> &'static PluginManifest {
        &MANIFEST
    }
    fn ribbon(&self) -> Box<dyn CadModule> {
        Box::new(HydroCompleteModule)
    }
    fn dispatch(&self, host: &mut dyn HostApi, cmd: &str) -> bool {
        dispatch::handle(host, cmd)
    }
}

ocs_plugin_api::export_plugin!(HydroCompletePlugin);