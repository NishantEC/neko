//! The usage ledger's wire types: one line per finished agent run (`usage_events`), grouped for the Usage page.
//! The live figures come from the runs themselves (`USAGE` task events, routine runs); this is the history that outlives them.
use serde::{Deserialize, Serialize};

/// One local day in the range. Every day of the range is present, zeros included.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerDay {
    /// Local `YYYY-MM-DD`.
    pub day: String,
    pub runs: u32,
    /// Runs where the provider reported no price or tokens.
    pub estimated_runs: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_micro_usd: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerProvider {
    pub provider: String,
    pub runs: u32,
    pub estimated_runs: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_micro_usd: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerHistory {
    /// Oldest first.
    pub days: Vec<LedgerDay>,
    /// By cost, then runs.
    pub providers: Vec<LedgerProvider>,
    pub total_runs: u32,
    /// The start of the first day in `days`.
    pub since_ms: i64,
}
