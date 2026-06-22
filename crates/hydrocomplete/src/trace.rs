//! Formula-transparent calculation steps (mirrors `CalcStep` / `TracedResult`).

#[derive(Debug, Clone, PartialEq)]
pub struct CalcStep {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub formula: String,
}

impl CalcStep {
    pub fn new(name: impl Into<String>, value: f64, unit: impl Into<String>, formula: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value,
            unit: unit.into(),
            formula: formula.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TracedResult {
    pub steps: Vec<CalcStep>,
}