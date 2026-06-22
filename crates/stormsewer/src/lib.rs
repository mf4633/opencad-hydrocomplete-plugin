// SPDX-License-Identifier: GPL-3.0-or-later

//! # HydroComplete
//!
//! Native-Rust storm-sewer network **hydrology & hydraulics** engine.
//!
//! It implements the standard, public-domain methods used by tools such as
//! Autodesk Hydraflow HydroCompletes:
//!
//! * **Rational method** peak-flow accumulation down a pipe network,
//! * **Manning** open-channel / partial-flow hydraulics for circular conduits,
//! * normal-depth, critical-depth and full-flow capacity,
//! * **HGL backwater** with junction losses and **standard-pipe sizing**
//!   against velocity / capacity criteria (Hydraflow-style design checks).
//! * **HEC-22** inlet capacity (grate, curb opening, combination, sag) and multi-RP IDF sets.
//!
//! This is an **engine only**: no GUI and no CAD dependencies, so it compiles
//! to a native library, to WASM (for hydrocomplete.com), and is consumable as
//! a module by an Open CAD Studio fork.
//!
//! ```
//! use stormsewer::{Network, Node, NodeKind, Pipe};
//! let net = Network {
//!     nodes: vec![
//!         Node::inlet("N1", 100.0, 105.0, 2.0, 0.7),
//!         Node::outfall("OUT", 99.0, 104.0),
//!     ],
//!     pipes: vec![Pipe::new("P1", "N1", "OUT", 100.0, 1.5, 0.013)],
//! };
//! let results = net.analyze_rational(4.0).unwrap(); // i = 4 in/hr
//! assert_eq!(results.len(), 1);
//! assert!((results[0].design_q - 5.6).abs() < 1e-6); // 4 * (0.7*2.0)
//! ```

pub mod catchment;
pub mod design;
pub mod drawing;
pub mod hydraulics;
pub mod hydrology;
pub mod idf;
pub mod io;
pub mod network;
pub mod params;
pub mod parse;
pub mod report;
pub mod report_html;

pub use catchment::*;
pub use design::*;
pub use drawing::*;
pub use hydraulics::*;
pub use hydrology::*;
pub use idf::*;
pub use io::*;
pub use network::*;
pub use params::*;
pub use parse::*;
pub use report_html::*;
