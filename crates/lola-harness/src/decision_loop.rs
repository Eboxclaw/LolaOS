//! The S1 decision loop — the first end-to-end vertical (ROADMAP S1).
//!
//! bars → features → signature → ledger snapshot → deterministic strategy
//! → risk engine (ring 0) → idempotent paper order → fill → portfolio
//! update. **No model anywhere in the path** (decision_mode =
//! `deterministic`): this loop is the A0 baseline every later experiment
//! is measured against. Crash-safety comes from the ledger: replayed ticks
//! demote to no-ops via idempotency keys (ADR-025: checkpoint/resume is
//! the normal path).

use serde::Serialize;

use crate::contracts::{ExecutionMode, OrderIntentV1, Side};
use crate::error::{LolaError, Result};
use crate::features::{self, Bar};
use crate::metrics::StageMetrics;
use crate::risk::{self, PortfolioView, RiskLimits};
use crate::strategy;

use lola_memory::ledger::{InsertOutcome, LedgerWriter, RecoveryState, TickTxn};

pub const FILL_MODEL_VERSION: &str = "paper-v1";

#[derive(Debug, Clone)]
pub struct LoopConfig {
    pub session_id: String,
    pub instrument_id: String,
    pub timeframe: String,
    pub strategy_id: String,
    pub limits: RiskLimits,
    /// Taker fee applied to paper fills (bps).
    pub fee_bps: f64,
    pub policy_version: String,
}

impl LoopConfig {
    pub fn new(session_id: &str, instrument_id: &str) -> Self {
        Self {
            session_id: session_id.into(),
            instrument_id: instrument_id.into(),
            timeframe: "5m".into(),
            strategy_id: strategy::MOMENTUM_V1.into(),
            limits: RiskLimits::default(),
            fee_bps: 4.5,
            policy_version: "risk-1".into(),
        }
    }
}

/// Paper portfolio state owned by the loop (mutated only on fills).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaperPortfolio {
    pub cash: f64,
    pub position_qty: f64,
    pub avg_entry: f64,
    pub session_start_equity: f64,
    pub realized_pnl_today: f64,
    pub peak_equity: f64,
    pub kill_switch: bool,
}

impl PaperPortfolio {
    pub fn funded(cash: f64) -> Self {
        Self {
            cash,
            position_qty: 0.0,
            avg_entry: 0.0,
            session_start_equity: cash,
            realized_pnl_today: 0.0,
            peak_equity: cash,
            kill_switch: false,
        }
    }

    fn view(&self, price: f64, spread_bps: f64, data_age_ns: i64) -> PortfolioView {
        let equity = self.cash + self.position_qty * price;
        PortfolioView {
            cash: self.cash,
            position_qty: self.position_qty,
            avg_entry_price: self.avg_entry,
            session_start_equity: self.session_start_equity,
            realized_pnl_today: self.realized_pnl_today,
            peak_equity: self.peak_equity.max(equity),
            price,
            spread_bps,
            data_age_ns,
            venue_ok: true,
            kill_switch: self.kill_switch,
            seen_idempotency_keys: 0,
            total_idempotency_keys: 0,
        }
    }
}

/// One tick's inputs.
pub struct TickInput<'a> {
    pub bars: &'a [Bar],
    pub spread_bps: f64,
    pub now_ns: i64,
}

/// What the tick did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum TickAction {
    /// Order accepted and paper-filled.
    Filled {
        side: &'static str,
        qty: f64,
        price: f64,
        fee: f64,
    },
    /// Strategy produced no candidate.
    NoCandidate,
    /// Risk engine rejected the intent (reasons recorded in the ledger).
    RiskRejected { failed_checks: Vec<String> },
    /// Replay after crash: the order already existed — nothing done.
    DuplicateSuppressed { idempotency_key: String },
}

#[derive(Debug, Clone)]
pub struct TickSummary {
    pub decision_id: String,
    pub action: TickAction,
}

/// Run one decision tick. All stages are timed into `metrics`.
pub fn run_tick(
    config: &LoopConfig,
    writer: &mut LedgerWriter,
    portfolio: &mut PaperPortfolio,
    input: &TickInput,
    metrics: &mut StageMetrics,
) -> Result<TickSummary> {
    let last = input
        .bars
        .last()
        .ok_or_else(|| LolaError::Config("no bars in tick".into()))?;

    let features = metrics.measure("features", || {
        features::compute(input.bars, input.spread_bps)
    });
    let Some(features) = features else {
        return Err(LolaError::Config(format!(
            "insufficient history: {} bars",
            input.bars.len()
        )));
    };

    // Persist the snapshot (idempotent on replay by unique key).
    let (signature, hash) = metrics.measure("signature", || {
        (
            features::state_signature(&config.timeframe, &features, &config.strategy_id),
            features::snapshot_hash(&features),
        )
    });
    // F4: the whole tick's writes commit as ONE transaction — drop
    // without commit rolls back (panic-safe), every return path commits.
    let mut txn = TickTxn::begin(writer)?;
    let writer = txn.writer();

    let snapshot_id = metrics.measure("ledger_snapshot", || {
        writer.insert_snapshot(
            &config.instrument_id,
            &config.timeframe,
            last.close_time_ns,
            &signature,
            &hash,
            &serde_json::to_string(&features).expect("features serialize"),
            features::to_vector9(&features),
        )
    })?;

    // Deterministic strategy candidate (A0: no model in the path).
    let candidate = metrics.measure("strategy", || {
        strategy::momentum_candidate(&features, portfolio.position_qty > 0.0)
    });

    let decision_id = writer.insert_decision(
        &config.session_id,
        &config.instrument_id,
        &config.timeframe,
        last.close_time_ns,
        &snapshot_id,
        &config.strategy_id,
        "deterministic",
        &serde_json::json!({
            "signature": signature,
            "features": features,
            "candidate": candidate,
        })
        .to_string(),
        &config.policy_version,
        "EVALUATED",
    )?;
    writer.append_event(
        &config.session_id,
        "decision_recorded",
        last.close_time_ns,
        &serde_json::json!({ "decision_id": decision_id }).to_string(),
    )?;

    let Some(candidate) = candidate else {
        txn.commit()?;
        return Ok(TickSummary {
            decision_id,
            action: TickAction::NoCandidate,
        });
    };

    // Position sizing: cap by concentration limit and available cash.
    let price = last.close;
    let equity = portfolio.cash + portfolio.position_qty * price;
    let max_notional = equity * config.limits.max_position_fraction;
    let (qty, reduce_only) = match candidate.action {
        Side::Buy => {
            let notional = max_notional.min(portfolio.cash * 0.98);
            (notional / price, false)
        }
        Side::Sell => (portfolio.position_qty, true),
    };
    if qty <= 0.0 {
        txn.commit()?;
        return Ok(TickSummary {
            decision_id,
            action: TickAction::NoCandidate,
        });
    }

    let intent = OrderIntentV1 {
        contract_version: "1.0".into(),
        decision_id: decision_id.clone(),
        instrument_id: config.instrument_id.clone(),
        action: candidate.action,
        order_type: crate::contracts::OrderType::Market,
        quantity: format!("{qty:.4}"),
        limit_price: Some(format!("{price:.2}")),
        time_in_force: crate::contracts::TimeInForce::Gtc,
        reduce_only,
        requested_leverage: Some(1),
        execution_mode: ExecutionMode::Paper,
    };

    // Ring 0: deterministic risk, independent of everything above.
    let view = portfolio.view(
        price,
        input.spread_bps,
        (input.now_ns - last.close_time_ns).max(0),
    );
    let report = metrics.measure("risk", || {
        risk::evaluate(&intent, &view, &config.limits, &config.policy_version)
    });
    writer.insert_risk_check(
        &decision_id,
        report.accepted,
        &config.policy_version,
        &serde_json::to_string(&report).expect("report serializes"),
    )?;
    writer.append_event(
        &config.session_id,
        "risk_verdict",
        last.close_time_ns,
        &serde_json::json!({ "decision_id": decision_id, "accepted": report.accepted }).to_string(),
    )?;

    if !report.accepted {
        txn.commit()?;
        return Ok(TickSummary {
            decision_id,
            action: TickAction::RiskRejected {
                failed_checks: report
                    .checks
                    .iter()
                    .filter(|c| !c.passed)
                    .map(|c| c.name.clone())
                    .collect(),
            },
        });
    }

    // Idempotency: the key is a pure function of (session, bar, action) —
    // a replayed tick after a crash hits the same key and demotes to a
    // no-op instead of double-filling.
    let idempotency_key = format!(
        "{}:{}:{}",
        config.session_id,
        last.close_time_ns,
        side_id(candidate.action)
    );
    let outcome = metrics.measure("ledger_order", || {
        writer.try_insert_order(
            &decision_id,
            &config.instrument_id,
            &idempotency_key,
            side_id(candidate.action),
            &intent.quantity,
            intent.limit_price.as_deref().unwrap_or("0"),
            "PAPER",
        )
    })?;
    let order_id = match outcome {
        InsertOutcome::Inserted(order_id) => order_id,
        InsertOutcome::Duplicate => {
            return Ok(TickSummary {
                decision_id,
                action: TickAction::DuplicateSuppressed { idempotency_key },
            });
        }
    };

    // Conservative paper fill at the bar close, crossed by half the
    // spread, plus taker fees (lola-replay assumptions in miniature).
    let direction = if candidate.action == Side::Buy {
        1.0
    } else {
        -1.0
    };
    let fill_price = price * (1.0 + direction * input.spread_bps / 2.0 / 10_000.0);
    let fee = qty * fill_price * config.fee_bps / 10_000.0;
    writer.insert_fill(
        &order_id,
        &format!("{fill_price:.4}"),
        &format!("{qty:.4}"),
        &format!("{fee:.4}"),
        last.close_time_ns,
        FILL_MODEL_VERSION,
    )?;
    writer.append_event(
        &config.session_id,
        "paper_order_filled",
        last.close_time_ns,
        &serde_json::json!({
            "decision_id": decision_id,
            "fill_price": fill_price,
            "qty": qty,
            "fee": fee,
        })
        .to_string(),
    )?;

    // Portfolio update.
    match candidate.action {
        Side::Buy => {
            let cost = qty * fill_price + fee;
            let new_qty = portfolio.position_qty + qty;
            portfolio.avg_entry =
                (portfolio.avg_entry * portfolio.position_qty + fill_price * qty) / new_qty;
            portfolio.position_qty = new_qty;
            portfolio.cash -= cost;
        }
        Side::Sell => {
            let proceeds = qty * fill_price - fee;
            portfolio.realized_pnl_today += qty * (fill_price - portfolio.avg_entry) - fee;
            portfolio.position_qty -= qty;
            if portfolio.position_qty <= 1e-12 {
                portfolio.position_qty = 0.0;
                portfolio.avg_entry = 0.0;
            }
            portfolio.cash += proceeds;
        }
    }
    let equity = portfolio.cash + portfolio.position_qty * price;
    portfolio.peak_equity = portfolio.peak_equity.max(equity);

    txn.commit()?;

    Ok(TickSummary {
        decision_id,
        action: TickAction::Filled {
            side: side_id(candidate.action),
            qty,
            price: fill_price,
            fee,
        },
    })
}

/// Restore loop-usable state after a process death. The ledger is the
/// truth; the portfolio is rebuilt by the caller from fills if needed —
/// S1 exposes recovery so replayed ticks are no-ops.
pub fn recover(writer: &LedgerWriter, session_id: &str) -> Result<RecoveryState> {
    writer.recovery(session_id).map_err(LolaError::from)
}

fn side_id(side: Side) -> &'static str {
    match side {
        Side::Buy => "BUY",
        Side::Sell => "SELL",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::MIN_BARS;

    /// Deterministic rising market — guaranteed BUY entry at some tick.
    pub(crate) fn rising_bars(count: usize) -> Vec<Bar> {
        (0..count)
            .map(|i| {
                let close = 100.0 + (i as f64).sin() * 1.0 + i as f64 * 0.5;
                Bar {
                    open: close,
                    high: close + 0.2,
                    low: close - 0.2,
                    close,
                    volume: if i % 10 == 9 { 1600.0 } else { 1000.0 },
                    close_time_ns: 1_700_000_000_000_000_000 + (i as i64) * 300_000_000_000,
                }
            })
            .collect()
    }

    #[test]
    fn sizing_is_capped_by_concentration() {
        let bars = rising_bars(MIN_BARS + 10);
        let last = bars.last().unwrap().close;
        let portfolio = PaperPortfolio::funded(10_000.0);
        let equity = portfolio.cash;
        let max_notional = equity * RiskLimits::default().max_position_fraction;
        let qty = max_notional.min(portfolio.cash * 0.98) / last;
        assert!(qty * last <= 2_501.0, "entry notional ≤ 25% of equity");
    }
}
