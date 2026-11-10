-- LOLA OS migration 0001 — initial trimmed schema (S0).
--
-- Design rules (docs/MEMORY.md):
--   * Hybrid semantics: canonical facts are upserted (one row per key);
--     decisions, orders, fills, outcomes and risk reports are append-only
--     and immutable. Graph edges carry validity intervals, never deletes.
--   * Money, prices and quantities travel as canonical decimal strings.
--     No REAL columns for order accounting.
--   * Two timestamps everywhere it matters: event_time_ns (market) vs
--     recorded/ingestion time (kernel).
--   * Bulky payloads are content-addressed blobs; rows stay lean.

CREATE TABLE venues (
    venue_id         TEXT PRIMARY KEY,
    market_family    TEXT NOT NULL,
    adapter_name     TEXT NOT NULL,
    network          TEXT NOT NULL DEFAULT 'TESTNET',
    status           TEXT NOT NULL DEFAULT 'ACTIVE',
    metadata_json    TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE instruments (
    instrument_id       TEXT PRIMARY KEY,
    venue_id            TEXT NOT NULL REFERENCES venues(venue_id),
    native_symbol       TEXT NOT NULL,
    instrument_class    TEXT NOT NULL CHECK (instrument_class IN ('SPOT', 'PERP')),
    base_currency       TEXT,
    quote_currency      TEXT NOT NULL,
    price_increment     TEXT NOT NULL,
    quantity_increment  TEXT NOT NULL,
    min_order_notional  TEXT,
    max_leverage        INTEGER NOT NULL DEFAULT 1 CHECK (max_leverage BETWEEN 1 AND 20),
    status              TEXT NOT NULL DEFAULT 'ACTIVE',
    metadata_json       TEXT NOT NULL DEFAULT '{}',
    UNIQUE (venue_id, native_symbol)
);

CREATE TABLE sessions (
    session_id          TEXT PRIMARY KEY,
    session_type        TEXT NOT NULL,
    parent_session_id   TEXT REFERENCES sessions(session_id),
    title               TEXT,
    status              TEXT NOT NULL,
    created_at_ns       INTEGER NOT NULL,
    updated_at_ns       INTEGER NOT NULL,
    metadata_json       TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE session_events (
    event_id          TEXT PRIMARY KEY,
    session_id        TEXT NOT NULL REFERENCES sessions(session_id),
    sequence_no       INTEGER NOT NULL,
    parent_event_id   TEXT REFERENCES session_events(event_id),
    event_type        TEXT NOT NULL,
    event_time_ns     INTEGER NOT NULL,
    recorded_at_ns    INTEGER NOT NULL,
    payload_json      TEXT,
    payload_uri       TEXT,               -- content-addressed blob (sha256)
    payload_hash      TEXT NOT NULL,
    UNIQUE (session_id, sequence_no)
);

-- Canonical fact table: one row per (scope, canonical_key), upserted.
-- Corrections supersede; they never append duplicates.
CREATE TABLE memory_items (
    memory_id        TEXT PRIMARY KEY,
    scope            TEXT NOT NULL,
    memory_type      TEXT NOT NULL,
    canonical_key    TEXT NOT NULL,
    value_json       TEXT NOT NULL,
    provenance_ref   TEXT NOT NULL,
    confidence       REAL,
    valid_from_ns    INTEGER,
    valid_until_ns   INTEGER,
    created_at_ns    INTEGER NOT NULL,
    updated_at_ns    INTEGER NOT NULL,
    UNIQUE (scope, canonical_key)
);

CREATE TABLE feature_snapshots (
    snapshot_id             TEXT PRIMARY KEY,
    instrument_id           TEXT NOT NULL REFERENCES instruments(instrument_id),
    timeframe               TEXT NOT NULL,
    event_time_ns           INTEGER NOT NULL,
    ingestion_time_ns       INTEGER NOT NULL,
    feature_schema_version  TEXT NOT NULL,
    state_signature         TEXT NOT NULL,
    snapshot_hash           TEXT NOT NULL,
    features_json           TEXT,         -- offloaded to blob store by TTL policy (S4)
    UNIQUE (instrument_id, timeframe, event_time_ns, snapshot_hash)
);

-- Append-only decision ledger. The heart of LOLA: never updated, never deleted.
CREATE TABLE decision_runs (
    decision_id            TEXT PRIMARY KEY,
    session_id             TEXT REFERENCES sessions(session_id),
    instrument_id          TEXT NOT NULL REFERENCES instruments(instrument_id),
    timeframe              TEXT NOT NULL,
    event_time_ns          INTEGER NOT NULL,
    snapshot_id            TEXT NOT NULL REFERENCES feature_snapshots(snapshot_id),
    strategy_id            TEXT,
    strategy_version       INTEGER,
    decision_mode          TEXT NOT NULL,   -- deterministic | shadow | gated
    d1_invocation_ref      TEXT,            -- model_invocations FK lands in S2
    agent_invocation_ref   TEXT,
    decision_json          TEXT NOT NULL,
    policy_version         TEXT NOT NULL,
    decision_hash          TEXT NOT NULL,
    status                 TEXT NOT NULL,
    created_at_ns          INTEGER NOT NULL
);

CREATE TABLE decision_outcomes (
    outcome_id                   TEXT PRIMARY KEY,
    decision_id                  TEXT NOT NULL REFERENCES decision_runs(decision_id),
    horizon_bars                 INTEGER NOT NULL,
    benchmark_return             REAL,
    strategy_return              REAL,
    maximum_favourable_excursion REAL,
    maximum_adverse_excursion    REAL,
    realised_pnl                 TEXT,
    fees                         TEXT,
    outcome_status               TEXT NOT NULL,
    measured_at_ns               INTEGER NOT NULL,
    UNIQUE (decision_id, horizon_bars)
);

CREATE TABLE risk_checks (
    risk_check_id    TEXT PRIMARY KEY,
    decision_id      TEXT NOT NULL REFERENCES decision_runs(decision_id),
    accepted         INTEGER NOT NULL,
    policy_version   TEXT NOT NULL,
    checks_json      TEXT NOT NULL,
    checked_at_ns    INTEGER NOT NULL
);

CREATE TABLE paper_orders (
    paper_order_id    TEXT PRIMARY KEY,
    decision_id       TEXT NOT NULL REFERENCES decision_runs(decision_id),
    instrument_id     TEXT NOT NULL REFERENCES instruments(instrument_id),
    venue_order_id    TEXT,
    client_order_id   TEXT NOT NULL,
    idempotency_key   TEXT NOT NULL UNIQUE,
    side              TEXT NOT NULL CHECK (side IN ('BUY', 'SELL')),
    order_type        TEXT NOT NULL,
    quantity          TEXT NOT NULL,
    limit_price       TEXT,
    leverage          INTEGER NOT NULL DEFAULT 1,
    execution_mode    TEXT NOT NULL CHECK (execution_mode IN ('PAPER', 'TESTNET', 'MAINNET')),
    status            TEXT NOT NULL,
    created_at_ns     INTEGER NOT NULL,
    updated_at_ns     INTEGER NOT NULL
);

CREATE TABLE paper_fills (
    fill_id             TEXT PRIMARY KEY,
    paper_order_id      TEXT NOT NULL REFERENCES paper_orders(paper_order_id),
    fill_price          TEXT NOT NULL,
    fill_quantity       TEXT NOT NULL,
    fee_amount          TEXT NOT NULL,
    fee_currency        TEXT NOT NULL,
    event_time_ns       INTEGER NOT NULL,
    fill_model_version  TEXT NOT NULL
);

CREATE TABLE graph_nodes (
    node_id         TEXT PRIMARY KEY,
    node_type       TEXT NOT NULL,
    canonical_key   TEXT NOT NULL,
    attributes_json TEXT NOT NULL DEFAULT '{}',
    created_at_ns   INTEGER NOT NULL,
    updated_at_ns   INTEGER NOT NULL,
    UNIQUE (node_type, canonical_key)
);

CREATE TABLE graph_edges (
    edge_id          TEXT PRIMARY KEY,
    source_node_id   TEXT NOT NULL REFERENCES graph_nodes(node_id),
    relation         TEXT NOT NULL,
    target_node_id   TEXT NOT NULL REFERENCES graph_nodes(node_id),
    attributes_json  TEXT NOT NULL DEFAULT '{}',
    source_type      TEXT NOT NULL,
    source_ref       TEXT NOT NULL,
    confidence       REAL,
    valid_from_ns    INTEGER,
    valid_until_ns   INTEGER,
    created_at_ns    INTEGER NOT NULL,
    UNIQUE (source_node_id, relation, target_node_id, source_ref)
);

CREATE INDEX idx_instruments_class   ON instruments(instrument_class, status);
CREATE INDEX idx_events_session      ON session_events(session_id, sequence_no);
CREATE INDEX idx_snapshots_state     ON feature_snapshots(state_signature, timeframe, event_time_ns);
CREATE INDEX idx_decisions_lookup    ON decision_runs(instrument_id, event_time_ns);
CREATE INDEX idx_outcomes_decision   ON decision_outcomes(decision_id, horizon_bars);
CREATE INDEX idx_orders_decision     ON paper_orders(decision_id);
CREATE INDEX idx_orders_instrument   ON paper_orders(instrument_id, created_at_ns);
CREATE INDEX idx_edges_source        ON graph_edges(source_node_id, relation);
CREATE INDEX idx_edges_target        ON graph_edges(target_node_id, relation);

-- Full-text search is secondary; original records remain authoritative.
CREATE VIRTUAL TABLE session_search USING fts5(
    event_id UNINDEXED,
    session_id UNINDEXED,
    searchable_text,
    tokenize = 'unicode61'
);

CREATE VIRTUAL TABLE memory_search USING fts5(
    memory_id UNINDEXED,
    canonical_key,
    searchable_text,
    tokenize = 'unicode61'
);
