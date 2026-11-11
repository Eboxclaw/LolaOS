//! Outcome-weighted decision case bank (ADR-031, `docs/CONTEXT.md` §5).
//!
//! Memento-style (arXiv 2508.16153): at decision time, retrieve a handful
//! of *settled* past decisions similar to the current state, weighted by
//! their realized outcomes and recency, and inject them into the d1
//! support pass. This is LOLA's learning loop — no fine-tuning, just the
//! ledger paying rent.

use serde::{Deserialize, Serialize};

/// Default number of cases injected per decision.
pub const DEFAULT_TOP_K: usize = 5;
/// Recency half-life for case weighting (30 days, in nanoseconds).
pub const RECENCY_HALF_LIFE_NS: i64 = 30 * 24 * 3600 * 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Pending,
    Settled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettledOutcome {
    pub horizon_bars: u32,
    /// Realized return after fees at this horizon.
    pub realized_return: f64,
    /// Canonical decimal string.
    pub fees: String,
    pub settled_at_ns: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseBankEntry {
    pub decision_id: String,
    /// Bucket signature of the deciding state (retrieval key).
    pub state_signature: String,
    pub strategy_id: Option<String>,
    pub outcome: SettledOutcome,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("case bank accepts only settled outcomes; '{decision_id}' is pending")]
pub struct UnsettledOutcomeError {
    pub decision_id: String,
}

impl CaseBankEntry {
    /// The settled-only invariant: entries are born from measured outcomes,
    /// never from hopes.
    pub fn try_new(
        decision_id: impl Into<String>,
        state_signature: impl Into<String>,
        strategy_id: Option<String>,
        status: OutcomeStatus,
        outcome: SettledOutcome,
    ) -> Result<Self, UnsettledOutcomeError> {
        if status != OutcomeStatus::Settled {
            return Err(UnsettledOutcomeError {
                decision_id: decision_id.into(),
            });
        }
        Ok(Self {
            decision_id: decision_id.into(),
            state_signature: state_signature.into(),
            strategy_id,
            outcome,
        })
    }

    /// Case weight: recency (exponential half-life decay) × outcome
    /// magnitude (win or loss — both teach; near-zero outcomes teach little).
    pub fn weight(&self, now_ns: i64) -> f64 {
        let age = (now_ns - self.outcome.settled_at_ns).max(0) as f64;
        let decayed = 0.5f64.powf(age / RECENCY_HALF_LIFE_NS as f64);
        decayed * (1.0 + self.outcome.realized_return.abs())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseBankQuery {
    pub state_signature: String,
    pub top_k: usize,
    pub max_age_ns: i64,
}

impl Default for CaseBankQuery {
    fn default() -> Self {
        Self {
            state_signature: String::new(),
            top_k: DEFAULT_TOP_K,
            max_age_ns: RECENCY_HALF_LIFE_NS * 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(return_: f64, at_ns: i64) -> SettledOutcome {
        SettledOutcome {
            horizon_bars: 12,
            realized_return: return_,
            fees: "0.40".into(),
            settled_at_ns: at_ns,
        }
    }

    #[test]
    fn pending_outcomes_are_rejected() {
        let err = CaseBankEntry::try_new(
            "d-1",
            "CRYPTO_PERP|5m|UpTrend|momentum-v1",
            None,
            OutcomeStatus::Pending,
            settled(0.01, 100),
        );
        assert!(err.is_err(), "only settled outcomes enter the case bank");
    }

    #[test]
    fn settled_outcomes_enter() {
        let entry = CaseBankEntry::try_new(
            "d-1",
            "sig",
            Some("momentum-v1".into()),
            OutcomeStatus::Settled,
            settled(0.02, 1000),
        )
        .expect("settled entry");
        assert_eq!(entry.decision_id, "d-1");
    }

    #[test]
    fn weights_decay_with_recency_and_scale_with_outcome() {
        let now = 10 * 24 * 3600 * 1_000_000_000; // day 10
        let half_life = RECENCY_HALF_LIFE_NS as f64;
        let _ = half_life;
        let small_old = CaseBankEntry::try_new(
            "d-old",
            "sig",
            None,
            OutcomeStatus::Settled,
            settled(0.005, now - 30 * 24 * 3600 * 1_000_000_000), // one half-life ago
        )
        .expect("entry");
        let big_new = CaseBankEntry::try_new(
            "d-new",
            "sig",
            None,
            OutcomeStatus::Settled,
            settled(0.08, now - 1_000_000),
        )
        .expect("entry");

        let w_old = small_old.weight(now);
        let w_new = big_new.weight(now);
        assert!(
            w_new > w_old,
            "recent, decisive cases outrank stale, small ones"
        );
        // One half-life ago → decay ≈ 0.5 × (1 + 0.005) ≈ 0.5025
        assert!((w_old - 0.5025).abs() < 1e-9);
    }

    #[test]
    fn default_query_is_bounded() {
        let query = CaseBankQuery::default();
        assert_eq!(query.top_k, DEFAULT_TOP_K);
        assert!(query.top_k <= 8, "hard cap on injected cases");
    }
}
