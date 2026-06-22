// SPDX-License-Identifier: GPL-3.0-only

//! `HydroComplete-cli` — run a storm-sewer analysis from a `.ssn` network file.
//!
//! Usage:  HydroComplete-cli <network-file>
//!
//! See `stormsewer::parse` for the file format.

use std::process::exit;
use stormsewer::parse::parse_ssn;
use stormsewer::report::format_analysis;

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(1);
}

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => die("usage: HydroComplete-cli <network-file>"),
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| die(&format!("cannot read {path}: {e}")));
    let parsed = parse_ssn(&text).unwrap_or_else(|e| die(&e));
    match parsed.network.analyze(&parsed.idf, &parsed.options) {
        Ok(a) => print!("{}", format_analysis(&a)),
        Err(e) => die(&e.to_string()),
    }
}
