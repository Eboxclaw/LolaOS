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

/// SP3 — DB-backed traversal + edge hot-cache (ADR-041 R3,
/// docs/PIPELINE.md §2). Precompiled hop queries (prepare_cached),
/// validity-filtered, bounded; the hot cache holds the active
/// instrument's neighborhood in RAM (small by design — MEMORY §4 keeps
/// provenance + validity, and our subgraph is *our* subgraph).
pub mod query {
    use rusqlite::Connection;

    /// One outgoing edge, projected for recall (validity already checked).
    #[derive(Debug, Clone, PartialEq)]
    pub struct EdgeRow {
        pub edge_id: String,
        pub target_node_id: String,
        pub relation: String,
        pub confidence: Option<f64>,
    }

    /// One second-degree reach: (via node, reached node).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TwoHop {
        pub via: String,
        pub reached: String,
    }

    fn validity_clause(alias: &str) -> String {
        format!(
            "({a}.valid_until_ns IS NULL OR {a}.valid_until_ns > ?2)
             AND ({a}.valid_from_ns IS NULL OR {a}.valid_from_ns <= ?2)",
            a = alias
        )
    }

    /// Outgoing valid edges from `node_id`, bounded.
    pub fn open_edges_from(
        conn: &Connection,
        node_id: &str,
        now_ns: i64,
        limit: u32,
    ) -> crate::Result<Vec<EdgeRow>> {
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT edge_id, target_node_id, relation, confidence
             FROM graph_edges e
             WHERE e.source_node_id = ?1 AND {}
             ORDER BY e.created_at_ns DESC
             LIMIT ?3",
            validity_clause("e")
        ))?;
        let rows = stmt.query_map(rusqlite::params![node_id, now_ns, limit], |row| {
            Ok(EdgeRow {
                edge_id: row.get(0)?,
                target_node_id: row.get(1)?,
                relation: row.get(2)?,
                confidence: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Two-hop reach: pairs (via, reached), validity-filtered on both
    /// legs, bounded. Cold path — SQL self-join on the indexed columns.
    pub fn two_hop(
        conn: &Connection,
        node_id: &str,
        now_ns: i64,
        limit: u32,
    ) -> crate::Result<Vec<TwoHop>> {
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT e1.target_node_id, e2.target_node_id
             FROM graph_edges e1
             JOIN graph_edges e2 ON e2.source_node_id = e1.target_node_id
             WHERE e1.source_node_id = ?1 AND {}
               AND {}
             LIMIT ?3",
            validity_clause("e1"),
            validity_clause("e2")
        ))?;
        let rows = stmt.query_map(rusqlite::params![node_id, now_ns, limit], |row| {
            Ok(TwoHop {
                via: row.get(0)?,
                reached: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// RAM hot-cache of outgoing edges for the active neighborhood
    /// (bounded, stamp-evicted like SP2). The caller invalidates on any
    /// graph write for correctness-critical reads.
    pub struct EdgeHotCache {
        entries: std::collections::HashMap<String, (Vec<EdgeRow>, u64)>,
        capacity: usize,
        stamp: u64,
        hits: u64,
        misses: u64,
    }

    impl EdgeHotCache {
        pub fn new(capacity: usize) -> Self {
            Self {
                entries: std::collections::HashMap::with_capacity(capacity),
                capacity,
                stamp: 0,
                hits: 0,
                misses: 0,
            }
        }

        /// Warm lookup: RAM only, no SQL. Gate < 20 µs (measured ~ns).
        pub fn warm(&mut self, node_id: &str) -> Option<Vec<EdgeRow>> {
            match self.entries.get_mut(node_id) {
                Some((edges, stamp)) => {
                    self.stamp += 1;
                    *stamp = self.stamp;
                    self.hits += 1;
                    Some(edges.clone())
                }
                None => {
                    self.misses += 1;
                    None
                }
            }
        }

        /// Cold load through SQL, then cached.
        pub fn cold(
            &mut self,
            conn: &Connection,
            node_id: &str,
            now_ns: i64,
        ) -> crate::Result<Vec<EdgeRow>> {
            let edges = open_edges_from(conn, node_id, now_ns, 20)?;
            if self.entries.len() >= self.capacity && !self.entries.contains_key(node_id) {
                if let Some(oldest) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, (_, stamp))| *stamp)
                    .map(|(k, _)| k.clone())
                {
                    self.entries.remove(&oldest);
                }
            }
            self.stamp += 1;
            self.entries
                .insert(node_id.to_string(), (edges.clone(), self.stamp));
            Ok(edges)
        }

        /// Edges for a node: warm when cached, cold through SQL otherwise.
        pub fn edges_for(
            &mut self,
            conn: &Connection,
            node_id: &str,
            now_ns: i64,
        ) -> crate::Result<Vec<EdgeRow>> {
            match self.warm(node_id) {
                Some(edges) => Ok(edges),
                None => self.cold(conn, node_id, now_ns),
            }
        }

        /// Invalidate the whole cache (call on any graph write).
        pub fn invalidate_all(&mut self) {
            self.entries.clear();
            self.misses += 1;
        }

        pub fn stats(&self) -> (u64, u64) {
            (self.hits, self.misses)
        }
    }
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
