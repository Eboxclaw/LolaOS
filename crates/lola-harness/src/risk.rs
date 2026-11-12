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
const _: () = assert!(CHECK_NAMES.len() == 15);

// ---------------------------------------------------------------------------
// S1 evaluation: deterministic, independent of both models, fail-closed.
// ---------------------------------------------------------------------------

use crate::contracts::{OrderIntentV1, Side};

/// Explicit limits (ADR-012: model confidence can never raise these).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RiskLimits {
    /// Max fraction of equity in one instrument.
    pub max_position_fraction: f64,
    /// Max leverage the policy allows (paper S1: 1).
    pub max_leverage: u32,
    /// Daily realized-loss limit as a fraction of session-start equity.
    pub max_daily_loss_fraction: f64,
    /// Max drawdown fraction from peak equity.
    pub max_drawdown_fraction: f64,
    /// Max acceptable spread (bps) at decision time.
    pub max_spread_bps: f64,
    /// Max age of the data the decision is based on (ns).
    pub max_data_age_ns: i64,
    /// Max notional per order (paper cap).
    pub max_notional: f64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position_fraction: 0.25,
            max_leverage: 1,
            max_daily_loss_fraction: 0.03,
            max_drawdown_fraction: 0.10,
            max_spread_bps: 15.0,
            max_data_age_ns: 90_000_000_000, // 90 s
            max_notional: 10_000.0,
        }
    }
}

/// Everything the engine needs to judge an intent — the portfolio as the
/// risk engine sees it, never as a model describes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortfolioView {
    pub cash: f64,
    pub position_qty: f64,
    pub avg_entry_price: f64,
    /// Session-start equity (daily-loss baseline).
    pub session_start_equity: f64,
    /// Realized PnL today.
    pub realized_pnl_today: f64,
    /// Peak equity seen this session.
    pub peak_equity: f64,
    /// Current mark price of the instrument.
    pub price: f64,
    /// Current spread in bps.
    pub spread_bps: f64,
    /// Age of the latest data the decision used.
    pub data_age_ns: i64,
    pub venue_ok: bool,
    pub kill_switch: bool,
    /// Idempotency keys already seen for this session.
    pub seen_idempotency_keys: usize,
    pub total_idempotency_keys: usize,
}

impl PortfolioView {
    pub fn equity(&self) -> f64 {
        self.cash + self.position_qty * self.price
    }
}

/// Evaluate an [`OrderIntentV1`] against all 15 named checks. Fail-closed:
/// missing information or a kill switch rejects.
pub fn evaluate(
    intent: &OrderIntentV1,
    view: &PortfolioView,
    limits: &RiskLimits,
    policy_version: &str,
) -> RiskReport {
    let qty: f64 = intent.quantity.parse().unwrap_or(f64::NAN);
    let price = intent
        .limit_price
        .as_deref()
        .unwrap_or("0")
        .parse()
        .unwrap_or(view.price);
    let notional = qty * price;
    let leverage = intent.requested_leverage.unwrap_or(1);
    let equity = view.equity();

    let mut checks = Vec::with_capacity(CHECK_NAMES.len());
    let mut check = |name: &str, passed: bool, detail: String| {
        checks.push(CheckResult {
            name: name.into(),
            passed,
            detail,
        });
    };

    let kill = view.kill_switch;
    check(
        "instrument_eligibility",
        !kill && qty.is_finite() && qty > 0.0,
        if kill {
            "kill switch engaged".into()
        } else {
            format!("qty={qty}")
        },
    );
    check(
        "leverage_within_policy",
        leverage <= limits.max_leverage,
        format!("{leverage} <= {}", limits.max_leverage),
    );
    check(
        "margin_available",
        leverage <= 1 || view.cash * leverage as f64 >= notional,
        "spot-style margin policy".into(),
    );
    check(
        "cash_available",
        intent.action == Side::Sell || view.cash + 1e-9 >= notional,
        format!("cash {:.2} vs notional {notional:.2}", view.cash),
    );
    let position_notional = (view.position_qty
        + if intent.action == Side::Buy {
            qty
        } else {
            -qty
        })
    .abs()
        * view.price;
    check(
        "position_concentration",
        equity <= 0.0 || position_notional / equity <= limits.max_position_fraction + 1e-9,
        format!("{position_notional:.2}/{equity:.2}"),
    );
    check(
        "per_instrument_exposure",
        position_notional <= limits.max_notional,
        format!("{position_notional:.2} <= {}", limits.max_notional),
    );
    check(
        "portfolio_exposure",
        equity <= 0.0 || view.cash >= 0.0,
        "single-instrument portfolio".into(),
    );
    let loss_fraction = if view.session_start_equity > 0.0 {
        (view.session_start_equity - equity).max(0.0) / view.session_start_equity
    } else {
        1.0
    };
    check(
        "daily_loss_limit",
        loss_fraction < limits.max_daily_loss_fraction,
        format!(
            "loss {loss_fraction:.4} < {}",
            limits.max_daily_loss_fraction
        ),
    );
    let drawdown = if view.peak_equity > 0.0 {
        (view.peak_equity - equity).max(0.0) / view.peak_equity
    } else {
        1.0
    };
    check(
        "drawdown_limit",
        drawdown < limits.max_drawdown_fraction,
        format!("drawdown {drawdown:.4} < {}", limits.max_drawdown_fraction),
    );
    check(
        "spread_conditions",
        view.spread_bps <= limits.max_spread_bps,
        format!("{} bps", view.spread_bps),
    );
    check(
        "liquidity_conditions",
        view.spread_bps <= limits.max_spread_bps * 2.0,
        "thin-liquidity backstop".into(),
    );
    check(
        "data_freshness",
        view.data_age_ns <= limits.max_data_age_ns,
        format!("{} ns old", view.data_age_ns),
    );
    check(
        "venue_available",
        view.venue_ok,
        if view.venue_ok {
            "ok".into()
        } else {
            "venue down".into()
        },
    );
    // No shorting (S1 policy): a SELL may only reduce an existing position.
    let sell_ok =
        intent.action == Side::Buy || intent.reduce_only || view.position_qty + 1e-9 >= qty;
    check(
        "reduce_only_validity",
        sell_ok,
        if sell_ok {
            "long-only policy satisfied".into()
        } else {
            format!("SELL {qty} > position {}", view.position_qty)
        },
    );
    check(
        "idempotency",
        true,
        "uniqueness enforced at the ledger layer (INSERT OR IGNORE + UNIQUE)".into(),
    );

    RiskReport {
        decision_id: intent.decision_id.clone(),
        accepted: checks.iter().all(|c| c.passed),
        policy_version: policy_version.into(),
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ExecutionMode, OrderType, Side, TimeInForce};

    fn intent(action: Side, qty: f64, price: f64) -> OrderIntentV1 {
        OrderIntentV1 {
            contract_version: "1.0".into(),
            decision_id: "d-test".into(),
            instrument_id: "HYPERLIQUID:BTC-PERP".into(),
            action,
            order_type: OrderType::Market,
            quantity: format!("{qty:.4}"),
            limit_price: Some(format!("{price:.2}")),
            time_in_force: TimeInForce::Gtc,
            reduce_only: false,
            requested_leverage: Some(1),
            execution_mode: ExecutionMode::Paper,
        }
    }

    fn view() -> PortfolioView {
        PortfolioView {
            cash: 10_000.0,
            position_qty: 0.0,
            avg_entry_price: 0.0,
            session_start_equity: 10_000.0,
            realized_pnl_today: 0.0,
            peak_equity: 10_000.0,
            price: 100.0,
            spread_bps: 2.0,
            data_age_ns: 1_000_000_000,
            venue_ok: true,
            kill_switch: false,
            seen_idempotency_keys: 0,
            total_idempotency_keys: 0,
        }
    }

    #[test]
    fn all_checks_present_and_sane_buy_passes() {
        let report = evaluate(
            &intent(Side::Buy, 5.0, 100.0),
            &view(),
            &RiskLimits::default(),
            "p1",
        );
        assert_eq!(report.checks.len(), 15);
        assert!(
            report.accepted,
            "failed: {:?}",
            report
                .checks
                .iter()
                .filter(|c| !c.passed)
                .collect::<Vec<_>>()
        );
        assert!(report.any_failed() != report.accepted);
    }

    #[test]
    fn sell_beyond_position_is_rejected_long_only() {
        let report = evaluate(
            &intent(Side::Sell, 5.0, 100.0),
            &view(),
            &RiskLimits::default(),
            "p1",
        );
        assert!(!report.accepted);
        assert!(report
            .checks
            .iter()
            .any(|c| c.name == "reduce_only_validity" && !c.passed));
    }

    #[test]
    fn reduce_only_sell_within_position_is_accepted() {
        let mut v = view();
        v.position_qty = 5.0;
        let mut order = intent(Side::Sell, 3.0, 100.0);
        order.reduce_only = true;
        let report = evaluate(&order, &v, &RiskLimits::default(), "p1");
        assert!(
            report.accepted,
            "failed: {:?}",
            report
                .checks
                .iter()
                .filter(|c| !c.passed)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn insufficient_cash_is_rejected() {
        let mut v = view();
        v.cash = 100.0;
        let report = evaluate(
            &intent(Side::Buy, 5.0, 100.0),
            &v,
            &RiskLimits::default(),
            "p1",
        );
        assert!(!report.accepted);
        assert!(report
            .checks
            .iter()
            .any(|c| c.name == "cash_available" && !c.passed));
    }

    #[test]
    fn leverage_above_policy_is_rejected() {
        let mut order = intent(Side::Buy, 5.0, 100.0);
        order.requested_leverage = Some(10);
        let report = evaluate(&order, &view(), &RiskLimits::default(), "p1");
        assert!(report
            .checks
            .iter()
            .any(|c| c.name == "leverage_within_policy" && !c.passed));
    }

    #[test]
    fn stale_data_and_kill_switch_fail_closed() {
        let mut stale = view();
        stale.data_age_ns = 10 * 60_000_000_000; // 10 min
        let report = evaluate(
            &intent(Side::Buy, 5.0, 100.0),
            &stale,
            &RiskLimits::default(),
            "p1",
        );
        assert!(!report.accepted);

        let mut killed = view();
        killed.kill_switch = true;
        let report = evaluate(
            &intent(Side::Buy, 5.0, 100.0),
            &killed,
            &RiskLimits::default(),
            "p1",
        );
        assert!(!report.accepted);
    }

    #[test]
    fn concentration_cap_blocks_oversized_entries() {
        // 30% of equity in one position > 25% cap.
        let report = evaluate(
            &intent(Side::Buy, 30.0, 100.0),
            &view(),
            &RiskLimits::default(),
            "p1",
        );
        assert!(report
            .checks
            .iter()
            .any(|c| c.name == "position_concentration" && !c.passed));
    }
}
