//! The single-writer ledger (S1). One authoritative writer process; every
//! mutation goes through here (ADR-009). Orders are idempotent by key —
//! a replayed tick after a crash can never double-fill.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::Result;
use crate::persistence;

/// Owning wrapper: takes `&mut self` for writes — the single-writer
/// discipline is enforced by the type system.
pub struct LedgerWriter {
    conn: Connection,
}

impl LedgerWriter {
    /// Open (and migrate) the operational database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = persistence::open(path)?;
        persistence::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// Wrap an already-migrated connection (tests, recovery reopen).
    pub fn from_connection(conn: Connection) -> Result<Self> {
        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Register the paper venue + one instrument (S1 bootstrap).
    pub fn register_paper_instrument(
        &mut self,
        venue_id: &str,
        instrument_id: &str,
        native_symbol: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO venues (venue_id, market_family, adapter_name, network, status)
             VALUES (?1, 'perp_dex', 'lola-paper', 'PAPER', 'ACTIVE')",
            params![venue_id],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO instruments
                 (instrument_id, venue_id, native_symbol, instrument_class,
                  quote_currency, price_increment, quantity_increment, max_leverage)
             VALUES (?1, ?2, ?3, 'PERP', 'USDC', '0.01', '0.0001', 1)",
            params![instrument_id, venue_id, native_symbol],
        )?;
        Ok(())
    }

    pub fn begin_session(&mut self, session_id: &str, session_type: &str) -> Result<()> {
        let now = persistence::now_ns();
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions
                 (session_id, session_type, status, created_at_ns, updated_at_ns)
             VALUES (?1, ?2, 'ACTIVE', ?3, ?3)",
            params![session_id, session_type, now],
        )?;
        Ok(())
    }

    /// Append one session event; returns its id. Sequence numbers are
    /// allocated by the writer (monotonic per session).
    pub fn append_event(
        &mut self,
        session_id: &str,
        event_type: &str,
        event_time_ns: i64,
        payload_json: &str,
    ) -> Result<String> {
        let seq: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(sequence_no), 0) + 1 FROM session_events WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )?;
        let event_id = format!("ev-{}", Uuid::new_v4());
        let hash = hash16(payload_json);
        self.conn.execute(
            "INSERT INTO session_events
                 (event_id, session_id, sequence_no, event_type, event_time_ns,
                  recorded_at_ns, payload_json, payload_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event_id,
                session_id,
                seq,
                event_type,
                event_time_ns,
                persistence::now_ns(),
                payload_json,
                hash
            ],
        )?;
        Ok(event_id)
    }

    /// Persist a feature snapshot; returns its id.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_snapshot(
        &mut self,
        instrument_id: &str,
        timeframe: &str,
        event_time_ns: i64,
        state_signature: &str,
        snapshot_hash: &str,
        features_json: &str,
    ) -> Result<String> {
        let snapshot_id = format!("snap-{}", Uuid::new_v4());
        self.conn.execute(
            "INSERT OR IGNORE INTO feature_snapshots
                 (snapshot_id, instrument_id, timeframe, event_time_ns,
                  ingestion_time_ns, feature_schema_version, state_signature,
                  snapshot_hash, features_json)
             VALUES (?1, ?2, ?3, ?4, ?5, 'market-state-v1', ?6, ?7, ?8)",
            params![
                snapshot_id,
                instrument_id,
                timeframe,
                event_time_ns,
                persistence::now_ns(),
                state_signature,
                snapshot_hash,
                features_json
            ],
        )?;
        if self.conn.changes() == 0 {
            // Already present (replay): fetch the existing id.
            return self
                .conn
                .query_row(
                    "SELECT snapshot_id FROM feature_snapshots
                     WHERE instrument_id = ?1 AND timeframe = ?2 AND event_time_ns = ?3
                       AND snapshot_hash = ?4",
                    params![instrument_id, timeframe, event_time_ns, snapshot_hash],
                    |row| row.get(0),
                )
                .map_err(Into::into);
        }
        Ok(snapshot_id)
    }

    /// Append an immutable decision row; returns its id.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_decision(
        &mut self,
        session_id: &str,
        instrument_id: &str,
        timeframe: &str,
        event_time_ns: i64,
        snapshot_id: &str,
        strategy_id: &str,
        decision_mode: &str,
        decision_json: &str,
        policy_version: &str,
        status: &str,
    ) -> Result<String> {
        let decision_id = format!("dec-{}", Uuid::new_v4());
        self.conn.execute(
            "INSERT INTO decision_runs
                 (decision_id, session_id, instrument_id, timeframe, event_time_ns,
                  snapshot_id, strategy_id, strategy_version, decision_mode,
                  decision_json, policy_version, decision_hash, status, created_at_ns)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                decision_id,
                session_id,
                instrument_id,
                timeframe,
                event_time_ns,
                snapshot_id,
                strategy_id,
                decision_mode,
                decision_json,
                policy_version,
                hash16(decision_json),
                status,
                persistence::now_ns()
            ],
        )?;
        Ok(decision_id)
    }

    pub fn insert_risk_check(
        &mut self,
        decision_id: &str,
        accepted: bool,
        policy_version: &str,
        checks_json: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO risk_checks
                 (risk_check_id, decision_id, accepted, policy_version,
                  checks_json, checked_at_ns)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                format!("rc-{}", Uuid::new_v4()),
                decision_id,
                accepted as i64,
                policy_version,
                checks_json,
                persistence::now_ns()
            ],
        )?;
        Ok(())
    }

    /// Idempotent order insert. Returns `Duplicate` when the idempotency
    /// key already exists — a replayed tick never double-fills.
    #[allow(clippy::too_many_arguments)]
    pub fn try_insert_order(
        &mut self,
        decision_id: &str,
        instrument_id: &str,
        idempotency_key: &str,
        side: &str,
        quantity: &str,
        limit_price: &str,
        execution_mode: &str,
    ) -> Result<InsertOutcome> {
        self.conn.execute(
            "INSERT OR IGNORE INTO paper_orders
                 (paper_order_id, decision_id, instrument_id, client_order_id,
                  idempotency_key, side, order_type, quantity, limit_price,
                  leverage, execution_mode, status, created_at_ns, updated_at_ns)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'MARKET', ?7, ?8, 1, ?9,
                     'FILLED', ?10, ?10)",
            params![
                format!("po-{}", Uuid::new_v4()),
                decision_id,
                instrument_id,
                format!("co-{idempotency_key}"),
                idempotency_key,
                side,
                quantity,
                limit_price,
                execution_mode,
                persistence::now_ns()
            ],
        )?;
        if self.conn.changes() == 0 {
            return Ok(InsertOutcome::Duplicate);
        }
        let order_id: String = self.conn.query_row(
            "SELECT paper_order_id FROM paper_orders WHERE idempotency_key = ?1",
            [idempotency_key],
            |row| row.get(0),
        )?;
        Ok(InsertOutcome::Inserted(order_id))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_fill(
        &mut self,
        paper_order_id: &str,
        fill_price: &str,
        fill_quantity: &str,
        fee_amount: &str,
        event_time_ns: i64,
        fill_model_version: &str,
    ) -> Result<String> {
        let fill_id = format!("fill-{}", Uuid::new_v4());
        self.conn.execute(
            "INSERT INTO paper_fills
                 (fill_id, paper_order_id, fill_price, fill_quantity,
                  fee_amount, fee_currency, event_time_ns, fill_model_version)
             VALUES (?1, ?2, ?3, ?4, ?5, 'USDC', ?6, ?7)",
            params![
                fill_id,
                paper_order_id,
                fill_price,
                fill_quantity,
                fee_amount,
                event_time_ns,
                fill_model_version
            ],
        )?;
        Ok(fill_id)
    }

    /// Recovery snapshot after a process death: where the session stopped
    /// and which idempotency keys already exist (so a replay demotes to
    /// no-ops instead of double-filling).
    pub fn recovery(&self, session_id: &str) -> Result<RecoveryState> {
        let last_seq: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(sequence_no) FROM session_events WHERE session_id = ?1",
                [session_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let last_decision: Option<String> = self
            .conn
            .query_row(
                "SELECT decision_id FROM decision_runs WHERE session_id = ?1
                 ORDER BY created_at_ns DESC LIMIT 1",
                [session_id],
                |row| row.get(0),
            )
            .optional()?;
        let orders: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT idempotency_key FROM paper_orders")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        let decisions: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM decision_runs WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )?;
        Ok(RecoveryState {
            session_id: session_id.into(),
            last_event_sequence: last_seq,
            last_decision_id: last_decision,
            known_idempotency_keys: orders,
            decision_count: decisions,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted(String),
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryState {
    pub session_id: String,
    pub last_event_sequence: Option<i64>,
    pub last_decision_id: Option<String>,
    pub known_idempotency_keys: Vec<String>,
    pub decision_count: i64,
}

fn hash16(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    hex::encode(&digest[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writer() -> (LedgerWriter, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("lola-ledger-{}.db", Uuid::new_v4()));
        let mut w = LedgerWriter::open(&path).expect("open");
        w.register_paper_instrument("paper", "PAPER:BTC-PERP", "BTC-PERP")
            .expect("instrument");
        (w, path)
    }

    fn cleanup(path: &std::path::PathBuf) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    /// Orders reference decisions (FK): create a real one per attempt.
    fn decision(w: &mut LedgerWriter, n: u8) -> String {
        w.begin_session("s-dup", "paper_trading").unwrap();
        let snap = w
            .insert_snapshot(
                "PAPER:BTC-PERP",
                "5m",
                n as i64,
                "sig",
                &format!("h{n}"),
                "{}",
            )
            .unwrap();
        w.insert_decision(
            "s-dup",
            "PAPER:BTC-PERP",
            "5m",
            n as i64,
            &snap,
            "momentum-v1",
            "deterministic",
            "{}",
            "p1",
            "EXECUTED",
        )
        .unwrap()
    }

    #[test]
    fn duplicate_idempotency_keys_never_double_insert() {
        let (mut w, path) = writer();
        let dec1 = decision(&mut w, 1);
        let dec2 = decision(&mut w, 2);
        let first = w
            .try_insert_order(
                &dec1,
                "PAPER:BTC-PERP",
                "sess:1:BUY",
                "BUY",
                "1.0",
                "100.00",
                "PAPER",
            )
            .expect("insert");
        let second = w
            .try_insert_order(
                &dec2,
                "PAPER:BTC-PERP",
                "sess:1:BUY",
                "BUY",
                "1.0",
                "100.00",
                "PAPER",
            )
            .expect("insert");
        assert!(matches!(first, InsertOutcome::Inserted(_)));
        assert_eq!(second, InsertOutcome::Duplicate);
        let count: i64 = w
            .conn
            .query_row("SELECT COUNT(*) FROM paper_orders", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "exactly one order for the key");
        cleanup(&path);
    }

    #[test]
    fn recovery_reports_session_state() {
        let (mut w, path) = writer();
        w.begin_session("s1", "paper_trading").unwrap();
        w.append_event("s1", "decision_recorded", 1, "{}").unwrap();
        let snap = w
            .insert_snapshot("PAPER:BTC-PERP", "5m", 42, "sig", "hash", "{}")
            .unwrap();
        let decision = w
            .insert_decision(
                "s1",
                "PAPER:BTC-PERP",
                "5m",
                42,
                &snap,
                "momentum-v1",
                "deterministic",
                "{}",
                "p1",
                "EXECUTED",
            )
            .unwrap();
        let state = w.recovery("s1").unwrap();
        assert_eq!(state.last_event_sequence, Some(1));
        assert_eq!(state.last_decision_id.as_deref(), Some(decision.as_str()));
        assert_eq!(state.decision_count, 1);
        assert!(state.known_idempotency_keys.is_empty());
        cleanup(&path);
    }
}
