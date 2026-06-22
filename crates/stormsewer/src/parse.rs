// SPDX-License-Identifier: GPL-3.0-only

//! Parser for the `.ssn` storm-sewer network text format.
//!
//! Whitespace-delimited; `#` starts a comment. Keywords:
//!
//! ```text
//! IDF <a> <b> <c>                 # IDF curve  i = a/(t+b)^c
//! INTENSITY <in/hr>               # OR a constant intensity (overrides IDF)
//! TAILWATER <elev_ft>             # outfall tailwater elevation
//! MINTC <min>                     # minimum time of concentration
//! JUNCTIONK <k>                   # junction loss coefficient
//! NODE <id> <inlet|junction|outfall> <x> <y> <invert> <rim> [area] [C] [tc_inlet]
//! PIPE <id> <from> <to> <length> <dia> <n>
//! ```
//!
//! For `outfall` nodes the trailing `area`, `C`, and `tc_inlet` are omitted.

use crate::idf::IdfCurve;
use crate::network::{AnalysisOptions, Network, Node, Pipe};

/// Result of parsing a `.ssn` document.
#[derive(Clone, Debug)]
pub struct ParsedNetwork {
    pub network: Network,
    pub idf: IdfCurve,
    pub options: AnalysisOptions,
}

/// Parse a `.ssn` document. Returns a human-readable error with the line number.
pub fn parse_ssn(text: &str) -> Result<ParsedNetwork, String> {
    let mut network = Network::default();
    let mut idf = IdfCurve::new(120.0, 10.0, 0.8);
    let mut options = AnalysisOptions::default();

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        let ln = lineno + 1;
        let num = |i: usize| -> Result<f64, String> {
            t.get(i)
                .ok_or_else(|| format!("line {ln}: missing value #{} in `{line}`", i + 1))?
                .parse::<f64>()
                .map_err(|_| format!("line {ln}: `{}` is not a number", t[i]))
        };

        match t[0].to_ascii_uppercase().as_str() {
            "IDF" => idf = IdfCurve::new(num(1)?, num(2)?, num(3)?),
            "INTENSITY" => options.intensity_override = Some(num(1)?),
            "TAILWATER" => options.tailwater = Some(num(1)?),
            "MINTC" => options.min_tc = num(1)?,
            "JUNCTIONK" => options.junction_k = num(1)?,
            "NODE" => {
                let id = *t.get(1).ok_or_else(|| format!("line {ln}: NODE needs an id"))?;
                let kind = t.get(2).ok_or_else(|| format!("line {ln}: NODE needs a kind"))?.to_ascii_lowercase();
                let (x, y, invert, rim) = (num(3)?, num(4)?, num(5)?, num(6)?);
                let node = match kind.as_str() {
                    "outfall" => Node::outfall(id, invert, rim),
                    "junction" => Node::junction(id, invert, rim, num(7)?, num(8)?),
                    "inlet" => Node::inlet(id, invert, rim, num(7)?, num(8)?),
                    other => return Err(format!("line {ln}: unknown node kind `{other}`")),
                }
                .at(x, y);
                let node = if t.len() > 9 { node.with_tc_inlet(num(9)?) } else { node };
                network.nodes.push(node);
            }
            "PIPE" => {
                let id = *t.get(1).ok_or_else(|| format!("line {ln}: PIPE needs an id"))?;
                let from = *t.get(2).ok_or_else(|| format!("line {ln}: PIPE needs a from-node"))?;
                let to = *t.get(3).ok_or_else(|| format!("line {ln}: PIPE needs a to-node"))?;
                network.pipes.push(Pipe::new(id, from, to, num(4)?, num(5)?, num(6)?));
            }
            other => return Err(format!("line {ln}: unknown keyword `{other}`")),
        }
    }
    Ok(ParsedNetwork { network, idf, options })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\
# demo
IDF 60 10 0.8
TAILWATER 100.5
NODE N1 inlet    0   0 104 110 1.0 0.70 12
NODE N2 inlet  300   0 102 108 1.0 0.70
NODE OUT outfall 600 0 100 106
PIPE P1 N1 N2 300 1.25 0.013
PIPE P2 N2 OUT 300 1.50 0.013
";

    #[test]
    fn parses_nodes_pipes_and_params() {
        let p = parse_ssn(DOC).unwrap();
        assert_eq!(p.network.nodes.len(), 3);
        assert_eq!(p.network.pipes.len(), 2);
        assert_eq!(p.options.tailwater, Some(100.5));
        let n2 = p.network.nodes.iter().find(|n| n.id == "N2").unwrap();
        assert_eq!((n2.x, n2.y), (300.0, 0.0));
        assert_eq!(n2.tc_inlet, 10.0); // defaulted (no trailing value)
        let n1 = p.network.nodes.iter().find(|n| n.id == "N1").unwrap();
        assert_eq!(n1.tc_inlet, 12.0);
    }

    #[test]
    fn analyzes_after_parse() {
        let p = parse_ssn(DOC).unwrap();
        let a = p.network.analyze(&p.idf, &p.options).unwrap();
        assert_eq!(a.pipes.len(), 2);
    }

    #[test]
    fn reports_bad_line() {
        let err = parse_ssn("NODE N1 inlet 0 0 oops 110 1 0.7").unwrap_err();
        assert!(err.contains("line 1"), "{err}");
    }
}
