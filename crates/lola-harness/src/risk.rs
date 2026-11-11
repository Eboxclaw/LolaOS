//! Risk-engine types (ADR-008/ADR-012).
//!
//! The engine is deterministic, independent of both models, and fails
//! closed. Model confidence can never raise a cap. The S1/S7 pipeline
//! implements the checks; the stable check names are fixed here so persisted
//! risk reports stay comparable across versions.

use serde::{Deserialize, Serialize};

/// One named check with its outcome. `name` must come from [`CHECK_NAMES`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Independent verdict for a proposed [`crate::contracts::OrderIntentV1`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskReport {
    pub decision_id: String,
    pub accepted: bool,
    pub policy_version: String,
    pub checks: Vec<CheckResult>,
}

impl RiskReport {
    pub fn any_failed(&self) -> bool {
        self.checks.iter().any(|check| !check.passed)
    }
}

/// Stable check identifiers, persisted in `risk_checks.checks_json`.
pub const CHECK_NAMES: &[&str] = &[
    "instrument_eligibility",
    "leverage_within_policy",
    "margin_available",
    "cash_available",
    "position_concentration",
    "per_instrument_exposure",
    "portfolio_exposure",
    "daily_loss_limit",
    "drawdown_limit",
    "spread_conditions",
    "liquidity_conditions",
    "data_freshness",
    "venue_available",
    "reduce_only_validity",
    "idempotency",
];
