//! Knowledge-graph types and traversal bounds (docs/MEMORY.md §4).
//!
//! The graph is relational, not a graph database: nodes keyed by
//! `(node_type, canonical_key)`, edges carrying provenance and validity
//! intervals. An edge whose fact was wrong is *closed* (`valid_until_ns`),
//! never deleted — the history of what LOLA believed is itself evidence.

use serde::{Deserialize, Serialize};

/// Default traversal depth. Two hops is the hard maximum.
pub const HOP_LIMIT_DEFAULT: u32 = 1;
pub const HOP_LIMIT_MAX: u32 = 2;
/// Maximum neighbours returned per traversal.
pub const NEIGHBOR_LIMIT: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    pub node_id: String,
    pub node_type: NodeType,
    pub canonical_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum NodeType {
    Instrument,
    Venue,
    MarketRegime,
    FeaturePattern,
    Strategy,
    StrategyVersion,
    Decision,
    Order,
    Trade,
    Outcome,
    Policy,
    Evidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub edge_id: String,
    pub source_node_id: String,
    pub relation: Relation,
    pub target_node_id: String,
    /// Provenance: what created this edge (decision id, tool run, policy).
    pub source_ref: String,
    pub confidence: Option<f64>,
    pub valid_from_ns: Option<i64>,
    pub valid_until_ns: Option<i64>,
}

impl GraphEdge {
    pub fn is_open(&self, at_ns: i64) -> bool {
        self.valid_from_ns.is_none_or(|from| from <= at_ns)
            && self.valid_until_ns.is_none_or(|until| at_ns < until)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Relation {
    TradesOn,
    HasRegime,
    MatchesPattern,
    UsedStrategy,
    SupportedBy,
    ResultedIn,
    EvaluatedBy,
    Supersedes,
    SimilarTo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_validity_windows_are_half_open() {
        let edge = GraphEdge {
            edge_id: "e".into(),
            source_node_id: "a".into(),
            relation: Relation::SimilarTo,
            target_node_id: "b".into(),
            source_ref: "decision-1".into(),
            confidence: Some(0.9),
            valid_from_ns: Some(100),
            valid_until_ns: Some(200),
        };
        assert!(edge.is_open(100));
        assert!(edge.is_open(199));
        assert!(!edge.is_open(200), "valid_until is exclusive");
        assert!(!edge.is_open(99));
    }
}
