//! Feature-engine types and availability rules.
//!
//! S1 implements computation; the schema version and the lookahead rule are
//! fixed here because every later component depends on them.

use crate::contracts::FEATURE_SCHEMA_VERSION;

pub fn schema_version() -> &'static str {
    FEATURE_SCHEMA_VERSION
}

/// The single availability rule for every feature in every schema version:
/// computable from bars that CLOSED at or before `event_time_ns`. The S7
/// lookahead audit enforces it mechanically.
pub fn availability_rule() -> &'static str {
    "closed_bars_only"
}

/// Reference to a persisted feature snapshot (row in `feature_snapshots`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureSnapshotRef {
    pub snapshot_id: String,
    pub instrument_id: String,
    pub timeframe: String,
    pub event_time_ns: i64,
    pub state_signature: String,
    pub snapshot_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_is_pinned() {
        assert_eq!(schema_version(), "market-state-v1");
        assert_eq!(availability_rule(), "closed_bars_only");
    }
}
