//! Integration tests for the persistence layer.

use std::path::PathBuf;

use lola_core::persistence;

fn scratch_db() -> (PathBuf, rusqlite::Connection) {
    let path = std::env::temp_dir().join(format!("lola-core-test-{}.db", uuid::Uuid::new_v4()));
    let conn = persistence::open(&path).expect("open scratch db");
    (path, conn)
}

#[test]
fn migrations_apply_and_are_idempotent() {
    let (path, mut conn) = scratch_db();

    let first = persistence::migrate(&mut conn).expect("first migration run");
    assert_eq!(first, vec![1]);

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
        "expected the full v1 schema, found {tables} tables"
    );

    drop(conn);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn wal_mode_is_enabled_on_file_databases() {
    let (path, conn) = scratch_db();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("read journal mode");
    assert_eq!(mode.to_lowercase(), "wal");

    drop(conn);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
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
         INSERT INTO sessions VALUES ('s1', 'paper_trading', NULL, NULL, 'ACTIVE', 1, 1, '{}');
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

    drop(conn);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}
