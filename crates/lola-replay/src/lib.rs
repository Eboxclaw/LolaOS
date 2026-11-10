//! Conservative replay assumptions for the S7 experiment ladder (A0–A3).
//!
//! Types only in S0. The defaults are deliberately pessimistic: the replay
//! engine must earn trust by being harder on fills than reality, not easier.
//! Bar data cannot reconstruct order books, so bar-compatible execution is
//! the honest ceiling for candle-driven backtests.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FillAssumptions {
    /// Orders fill on the next completed bar — never intrabar.
    pub next_bar_fills: bool,
    /// Taker fees in basis points applied to every fill.
    pub taker_fee_bps: f64,
    /// Crossing penalty in basis points added to every fill price.
    pub spread_penalty_bps: f64,
    /// Perps: accrue funding on held positions at historical rates.
    pub funding_accrual: bool,
    /// Limit orders only fill when price trades through, not touches.
    pub no_limit_fill_optimism: bool,
}

impl Default for FillAssumptions {
    fn default() -> Self {
        Self {
            next_bar_fills: true,
            taker_fee_bps: 4.5,
            spread_penalty_bps: 2.0,
            funding_accrual: true,
            no_limit_fill_optimism: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReplayConfig {
    pub assumptions: FillAssumptions,
    pub instruments: Vec<String>,
    pub start_ns: Option<i64>,
    pub end_ns: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_conservative() {
        let assumptions = FillAssumptions::default();
        assert!(assumptions.next_bar_fills, "no intrabar fills");
        assert!(assumptions.funding_accrual, "perps pay funding");
        assert!(assumptions.no_limit_fill_optimism, "no touch-fills");
        assert!(assumptions.taker_fee_bps > 0.0);
        assert!(assumptions.spread_penalty_bps > 0.0);
    }
}
