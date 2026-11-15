-- LOLA OS migration 0003 — SP1: numeric feature vectors for retrieval
-- (ADR-041, docs/PIPELINE.md §2 R1). The bucket-kNN scan reads compact
-- numeric rows instead of parsing features_json TEXT (the 8 ms → sub-ms
-- fix). Enum states encode as small ints (trend/vol/liquidity order).

CREATE TABLE feature_vectors (
    snapshot_id      TEXT PRIMARY KEY REFERENCES feature_snapshots(snapshot_id),
    instrument_id    TEXT NOT NULL,
    timeframe        TEXT NOT NULL,
    event_time_ns    INTEGER NOT NULL,
    state_signature  TEXT NOT NULL,
    r1  REAL NOT NULL,
    r5  REAL NOT NULL,
    r20 REAL NOT NULL,
    atr REAL NOT NULL,
    vr  REAL NOT NULL,
    spread REAL NOT NULL,
    trend_s INTEGER NOT NULL,
    vol_s   INTEGER NOT NULL,
    liq_s   INTEGER NOT NULL
);

CREATE INDEX idx_feature_vectors_bucket
ON feature_vectors(state_signature, event_time_ns DESC);
