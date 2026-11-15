//! SQLite persistence: WAL, single-writer discipline, versioned migrations.
//!
//! Rules (docs/MEMORY.md):
//! - One authoritative writer process; many readers.
//! - WAL must live on a local filesystem, never a network mount.
//! - Migrations are single-sourced from the repo `migrations/` directory and
//!   embedded at compile time, so a mobile build carries them without a
//!   filesystem layout.
//! - Writes are serialized through the future persistence service; crates
//!   must not open their own write connections.

use std::path::Path;

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::error::Result;

/// Canonical migrations, single-sourced from the repo `migrations/` directory.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../../migrations/0001_init.sql")),
    (
        2,
        include_str!("../../../migrations/0002_omni_sessions.sql"),
    ),
    (
        3,
        include_str!("../../../migrations/0003_feature_vectors.sql"),
    ),
];

/// Durability profile (F4, ADR-038). Under WAL, `NORMAL` is crash-safe
/// (an app crash never corrupts; at most the last checkpoint is lost —
/// exactly the ADR-025 checkpoint/resume model) and removes the
/// fsync-per-commit cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// fsync per commit (strict power-loss durability).
    Full,
    /// WAL-normal: single-digit-µs commit class.
    Normal,
}

/// Open with an explicit durability profile + bounded WAL (rarer
/// autocheckpoints so the committing thread stalls less often; K6 moves
/// them off the decode thread entirely).
pub fn open_with_durability(path: &Path, durability: Durability) -> Result<Connection> {
    let conn = Connection::open(path)?;
    // foreign_keys and synchronous setters return no rows.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    match durability {
        Durability::Full => conn.pragma_update(None, "synchronous", "FULL")?,
        Durability::Normal => conn.pragma_update(None, "synchronous", "NORMAL")?,
    }
    // journal_mode and busy_timeout setters return the new value as a row,
    // so read them back instead of using pragma_update.
    if path != Path::new(":memory:") {
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
    }
    let _: i64 = conn.query_row("PRAGMA busy_timeout=5000", [], |row| row.get(0))?;
    let _: i64 = conn.query_row("PRAGMA wal_autocheckpoint=2000", [], |row| row.get(0))?;
    Ok(conn)
}

/// Open with the loop-path profile (F4: WAL + synchronous NORMAL +
/// autocheckpoint DISABLED — checkpoints are explicit maintenance via
/// [`crate::ledger::LedgerWriter::checkpoint`], run between ticks by the
/// scheduler, never inside the decision path; the F4 tail test proves
/// the 28 ms autocheckpoint stall stays off the write path).
pub fn open_fast(path: &Path) -> Result<Connection> {
    let conn = open_with_durability(path, Durability::Normal)?;
    let _: i64 = conn.query_row("PRAGMA wal_autocheckpoint=0", [], |row| row.get(0))?;
    Ok(conn)
}

/// Legacy entry point: strict FULL durability (operator-chosen).
pub fn open(path: &Path) -> Result<Connection> {
    open_with_durability(path, Durability::Full)
}

/// Apply pending migrations in order. Returns the versions applied now
/// (empty when the database is already current).
pub fn migrate(conn: &mut Connection) -> Result<Vec<i64>> {
    let mut applied = Vec::new();
    let tx = conn.transaction()?;

    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version       INTEGER PRIMARY KEY,
            checksum      TEXT NOT NULL,
            applied_at_ns INTEGER NOT NULL
        );",
    )?;

    for (version, sql) in MIGRATIONS {
        let present: i64 = tx.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
            [version],
            |row| row.get(0),
        )?;
        if present > 0 {
            continue;
        }
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, checksum, applied_at_ns)
             VALUES (?1, ?2, ?3)",
            params![version, checksum(sql), now_ns()],
        )?;
        applied.push(*version);
    }

    tx.commit()?;
    Ok(applied)
}

/// Monotonic-ish wall clock in nanoseconds (uuid v7 shares this epoch).
pub fn now_ns() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn checksum(sql: &str) -> String {
    let digest = Sha256::digest(sql.as_bytes());
    hex::encode(&digest[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_checksums_are_stable() {
        // Guards against editing an already-applied migration in place.
        assert_eq!(checksum(MIGRATIONS[0].1).len(), 16);
        assert_eq!(checksum(MIGRATIONS[1].1).len(), 16);
        assert_ne!(checksum(MIGRATIONS[0].1), checksum(MIGRATIONS[1].1));
    }
}
