//! The five-step retrieval pipeline — types and budgets (ADR-010/ADR-011).
//!
//! S4 implements the engine; the contract is fixed here so the decision
//! loop and its metrics can be built against it first. All steps are
//! deterministic: no vectors in v0.1.

/// End-to-end retrieval budget per decision cycle.
pub const RETRIEVAL_BUDGET_MS: u64 = 10;

/// Maximum candidates entering the bounded numeric kNN ranking step.
pub const KNN_CANDIDATE_LIMIT: usize = 512;
/// Top-k similar states returned.
pub const KNN_TOP_K: usize = 8;

/// A dependency that must match exactly for a decision-cache hit. A changed
/// dependency invalidates — a stale cache must never silently serve.
pub const CACHE_DEPENDENCIES: &[&str] = &[
    "snapshot_hash",
    "feature_schema_version",
    "question_schema_version", // ADR-036: schema drift invalidates cached d1 answers
    "strategy_version",
    "model_checkpoint",
    "adapter_version",
    "policy_version",
    "instrument",
    "timeframe",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalStep {
    /// 1. Exact decision-cache hit (all [`CACHE_DEPENDENCIES`] match).
    DecisionCache { hit: bool },
    /// 2. Exact state-signature lookup.
    Signature { signature: String, hit: bool },
    /// 3. Indexed history for instrument + strategy.
    History { decision_ids: usize },
    /// 4. Bucket-narrowed, numeric-distance-ranked similar states.
    SimilarStates { candidates: usize, returned: usize },
    /// 5. Bounded graph traversal (≤ [`HOP_LIMIT_MAX`][crate::graph::HOP_LIMIT_MAX] hops).
    GraphTraversal { hops: u32, neighbors: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RetrievalStats {
    /// Steps executed, in order.
    pub steps: Vec<RetrievalStep>,
    /// Decision ids surfaced for the decision path to weigh.
    pub decision_ids: Vec<String>,
}

// Compile-time invariants (clippy-clean alternative to constant asserts).
const _: () = assert!(KNN_TOP_K > 0);
const _: () = assert!(KNN_CANDIDATE_LIMIT >= KNN_TOP_K);
const _: () = assert!(CACHE_DEPENDENCIES.len() == 9);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_dependencies_are_the_full_invalidation_set() {
        assert!(CACHE_DEPENDENCIES.contains(&"policy_version"));
    }
}
