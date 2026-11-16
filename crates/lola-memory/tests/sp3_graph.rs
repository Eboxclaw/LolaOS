//! SP3 tests: validity-filtered traversal, 2-hop reach, and the edge
//! hot-cache semantics (warm == cold; invalidate sees writes).

use lola_memory::graph::query::{two_hop, EdgeHotCache, EdgeRow};
use lola_memory::persistence;
use rusqlite::params;

static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn seeded() -> (rusqlite::Connection, std::path::PathBuf) {
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("lola-sp3-{}-{n}.db", std::process::id()));
    let mut conn = persistence::open_fast(&path).expect("open");
    persistence::migrate(&mut conn).expect("migrate");

    // Nodes: n-instrument, n-regime, n-pattern (2-hop), n-decision.
    for (id, ty, key) in [
        ("n-instrument", "Instrument", "BTC-PERP"),
        ("n-regime", "MarketRegime", "UpTrend"),
        ("n-pattern", "FeaturePattern", "vol-spike"),
        ("n-decision", "Decision", "d-1"),
    ] {
        conn.execute(
            "INSERT OR IGNORE INTO graph_nodes (node_id, node_type, canonical_key,
                 created_at_ns, updated_at_ns)
             VALUES (?1, ?2, ?3, 1, 1)",
            params![id, ty, key],
        )
        .expect("node");
    }
    // Edges from n-instrument: one valid now, one expired, one future.
    //   valid:    -> n-regime (HasRegime)
    //   expired:  -> n-pattern (closed at t=50)
    //   future:   -> n-decision (opens at t=500)
    let edges: [(&str, &str, &str, i64, i64); 3] = [
        ("e1", "n-regime", "HasRegime", 0, 0),
        ("e2", "n-pattern", "MatchesPattern", 0, 50),
        ("e3", "n-decision", "ResultedIn", 500, 0),
    ];
    for (edge_id, target, relation, from, until) in edges {
        let (vf, vu) = (
            if from == 0 { None } else { Some(from) },
            if until == 0 { None } else { Some(until) },
        );
        conn.execute(
            "INSERT INTO graph_edges (edge_id, source_node_id, relation, target_node_id,
                 attributes_json, source_type, source_ref, valid_from_ns, valid_until_ns,
                 created_at_ns)
             VALUES (?1, 'n-instrument', ?2, ?3, '{}', 'test', 'sp3', ?4, ?5, 1)",
            params![edge_id, relation, target, vf, vu],
        )
        .expect("edge");
    }
    // Second degree: n-regime -> n-pattern (valid).
    conn.execute(
        "INSERT INTO graph_edges (edge_id, source_node_id, relation, target_node_id,
             attributes_json, source_type, source_ref, created_at_ns)
         VALUES ('e4', 'n-regime', 'MatchesPattern', 'n-pattern', '{}', 'test', 'sp3', 1)",
        params![],
    )
    .expect("edge2");

    (conn, path)
}

fn cleanup(path: &std::path::PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn one_hop_filters_validity() {
    let (conn, path) = seeded();
    // now = 100: e1 valid, e2 expired (until 50), e3 not yet open (from 500).
    let edges =
        lola_memory::graph::query::open_edges_from(&conn, "n-instrument", 100, 20).expect("edges");
    assert_eq!(edges.len(), 1, "only the currently-valid edge");
    assert_eq!(edges[0].target_node_id, "n-regime");
    drop(conn);
    cleanup(&path);
}

#[test]
fn two_hop_reaches_through_valid_edges() {
    let (conn, path) = seeded();
    let reach = two_hop(&conn, "n-instrument", 100, 20).expect("two hop");
    assert!(
        reach
            .iter()
            .any(|p| p.via == "n-regime" && p.reached == "n-pattern"),
        "instrument -> regime -> pattern must be reachable"
    );
    drop(conn);
    cleanup(&path);
}

#[test]
fn hot_cache_warm_equals_cold_and_invalidations_see_writes() {
    let (conn, path) = seeded();
    let mut cache = EdgeHotCache::new(64);
    let now = 100;

    let cold = cache.cold(&conn, "n-instrument", now).expect("cold");
    let warm = cache.warm("n-instrument").expect("warm");
    assert_eq!(cold, warm, "warm must equal cold");

    // A write lands underneath; the cache must not see it until invalidated.
    conn.execute(
        "INSERT INTO graph_edges (edge_id, source_node_id, relation, target_node_id,
             attributes_json, source_type, source_ref, created_at_ns)
         VALUES ('e5', 'n-instrument', 'SupportedBy', 'n-decision', '{}', 'test', 'sp3', 2)",
        params![],
    )
    .expect("new edge");
    let stale = cache.warm("n-instrument").expect("stale warm");
    assert_eq!(stale.len(), 1, "stale until invalidated");

    cache.invalidate_all();
    let fresh = cache.edges_for(&conn, "n-instrument", now).expect("fresh");
    assert_eq!(fresh.len(), 2, "invalidation sees the write");
    let (hits, misses) = cache.stats();
    assert!(hits >= 2 && misses >= 2);
    drop(conn);
    cleanup(&path);
}

#[test]
fn edge_row_shape_round_trips() {
    let row = EdgeRow {
        edge_id: "e1".into(),
        target_node_id: "n-regime".into(),
        relation: "HasRegime".into(),
        confidence: Some(0.9),
    };
    assert_eq!(row.relation, "HasRegime");
    assert_eq!(row.confidence, Some(0.9));
}
