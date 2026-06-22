//! # hydrocomplete
//!
//! Portable stormwater engine — Rust counterpart to `HydroComplete.Engine`.
//! Builds on [`stormsewer`] for network topology, Rational method, Manning
//! (circular), HGL, and IDF; adds box/arch conduits, SCS runoff, and shared
//! models used by the OpenCAD plugin and future WASM/desktop targets.

pub mod about;
pub mod arch_conduit;
pub mod box_conduit;
pub mod manning;
pub mod models;
pub mod scs_runoff;

pub use stormsewer;