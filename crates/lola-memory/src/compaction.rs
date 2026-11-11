//! Compaction types — archive-with-witnesses (Hermes pattern, ADR-022).
//!
//! Compaction never deletes: the summary row references every event it
//! absorbed; the originals are soft-archived and remain FTS-searchable.
//! The same record drives the KV-side `seq_rm` eviction (see
//! docs/MEMORY.md, KV↔ledger twin ops).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionRecord {
    /// Structured summary that replaces the absorbed range in future prompts.
    pub summary: String,
    /// First event id still kept verbatim in context.
    pub first_kept_event_id: String,
    /// Every event id absorbed by this compaction — the audit witness.
    pub absorbed_event_ids: Vec<String>,
    /// Token count before compaction (for cache/accounting).
    pub tokens_before: u64,
}

impl CompactionRecord {
    pub fn absorbs(&self, event_id: &str) -> bool {
        self.absorbed_event_ids.iter().any(|id| id == event_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn witnesses_are_queryable() {
        let record = CompactionRecord {
            summary: "goal: benchmark A0".into(),
            first_kept_event_id: "ev-9".into(),
            absorbed_event_ids: vec!["ev-1".into(), "ev-2".into()],
            tokens_before: 4096,
        };
        assert!(record.absorbs("ev-1"));
        assert!(!record.absorbs("ev-9"));
    }
}
