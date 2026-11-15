//! Integration tests for the persistence layer.

use std::path::PathBuf;

use lola_memory::persistence;

fn scratch_db() -> (PathBuf, rusqlite::Connection) {
    let path = std::env::temp_dir().join(format!("lola-memory-test-{}.db", uuid::Uuid::new_v4()));
    let conn = persistence::open(&path).expect("open scratch db");
    (path, conn)
}

fn cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn migrations_apply_in_order_and_are_idempotent() {
    let (path, mut conn) = scratch_db();

    let first = persistence::migrate(&mut conn).expect("first migration run");
    assert_eq!(first, vec![1, 2, 3]);

    let second = persistence::migrate(&mut conn).expect("second migration run");
    assert!(second.is_empty(), "migrations must be idempotent");

    let tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'view')
             AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .expect("count tables");
    assert!(
        tables >= 15,
        "expected the full schema, found {tables} tables"
    );

    // Migration 0002 columns exist.
    let snapshot_col: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions')
             WHERE name = 'memory_snapshot_hash'",
            [],
            |row| row.get(0),
        )
        .expect("check sessions columns");
    assert_eq!(snapshot_col, 1, "memory_snapshot_hash column must exist");

    drop(conn);
    cleanup(&path);
}

#[test]
fn wal_mode_is_enabled_on_file_databases() {
    let (path, conn) = scratch_db();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("read journal mode");
    assert_eq!(mode.to_lowercase(), "wal");

    drop(conn);
    cleanup(&path);
}

#[test]
fn append_only_ledger_rejects_overwrites() {
    // The schema itself must make silent mutation impossible: decisions have
    // no mutable columns and outcomes are keyed by (decision_id, horizon).
    let (path, mut conn) = scratch_db();
    persistence::migrate(&mut conn).expect("migrate");

    conn.execute_batch(
        "INSERT INTO venues VALUES ('hyperliquid', 'perp_dex', 'lola-venue-hl', 'TESTNET', 'ACTIVE', '{}');
         INSERT INTO instruments (instrument_id, venue_id, native_symbol, instrument_class,
             quote_currency, price_increment, quantity_increment)
             VALUES ('HYPERLIQUID:BTC-PERP', 'hyperliquid', 'BTC-PERP', 'PERP',
             'USDC', '0.10', '0.001');
         INSERT INTO sessions (session_id, session_type, status, created_at_ns, updated_at_ns)
             VALUES ('s1', 'paper_trading', 'ACTIVE', 1, 1);
         INSERT INTO session_events (event_id, session_id, sequence_no, event_type,
             event_time_ns, recorded_at_ns, payload_hash, media_uri)
             VALUES ('ev-1', 's1', 1, 'message', 1, 1, 'hash', 'blob:media-abc');
         INSERT INTO feature_snapshots VALUES ('snap1', 'HYPERLIQUID:BTC-PERP', '5m', 1, 1,
             'market-state-v1', 'sig', 'hash', '{}');
         INSERT INTO decision_runs (decision_id, instrument_id, timeframe, event_time_ns,
             snapshot_id, decision_mode, decision_json, policy_version, decision_hash,
             status, created_at_ns)
             VALUES ('d1', 'HYPERLIQUID:BTC-PERP', '5m', 1, 'snap1', 'deterministic',
             '{}', 'policy-1', 'hash', 'EXECUTED', 1);
         INSERT INTO decision_outcomes VALUES ('o1', 'd1', 12, 0.01, 0.02, 0.03, -0.01,
             '12.50', '0.40', 'MEASURED', 1);",
    )
    .expect("seed ledger rows");

    // Duplicate outcome for the same horizon must violate uniqueness.
    let dup = conn.execute(
        "INSERT INTO decision_outcomes VALUES ('o2', 'd1', 12, 0, 0, 0, 0, '0', '0', 'MEASURED', 1)",
        [],
    );
    assert!(
        dup.is_err(),
        "duplicate (decision_id, horizon) must be rejected"
    );

    // Duplicate session sequence must violate uniqueness.
    let dup_seq = conn.execute(
        "INSERT INTO session_events (event_id, session_id, sequence_no, event_type,
            event_time_ns, recorded_at_ns, payload_hash)
            VALUES ('ev-2', 's1', 1, 'message', 2, 2, 'hash')",
        [],
    );
    assert!(
        dup_seq.is_err(),
        "duplicate (session, seq) must be rejected"
    );

    drop(conn);
    cleanup(&path);
}
