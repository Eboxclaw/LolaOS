# LOLA Memory — Storage, Knowledge Graph, Retrieval

Companion to `docs/RFC-002.md` §5–6. Schema of record:
`migrations/0001_init.sql`.

## 1. Philosophy (locked 2026-11-10)

**Immutable core + TTL'd bulk**, with **hybrid** update semantics:

| Data class | Semantics | Lifetime |
|---|---|---|
| Decisions, outcomes, orders, fills, risk reports | Append-only, immutable | Forever |
| Canonical facts (memory items, instruments, regimes) | Upsert — one row per `(scope, canonical_key)` | Current value + timestamps |
| Graph edges | Versioned: `valid_from_ns`/`valid_until_ns`, never deleted | History queryable |
| Model request/response payloads, session bulk, feature snapshot bodies | Content-addressed blobs, TTL-compacted | Rows + hashes forever; payloads per TTL |

"Over-storing" is defined as: **storing a payload twice, or storing a bulk
payload in a hot row after its TTL.** Rows are cheap; payloads move to
blobs on schedule and nothing is silently lost — hashes remain verifiable.

## 2. Storage layout

- **SQLite (WAL, single writer, `data/lola.db`)** — operational state:
  ledger, facts, graph, sessions, orders. Local filesystem only.
- **Parquet (`data/market/...`)** — historical bars, features, backtest
  outputs. Partitioned `venue/instrument/timeframe/year/month`.
- **Blob store (`data/blobs/<sha256[:2]>/<hash>.zst`)** — content-addressed,
  deduplicated payloads. `payload_uri`/`payload_hash` columns reference it.
- DuckDB, if ever needed, is an *offline research* tool on the Parquet
  files — never a mobile runtime dependency.

## 3. Retrieval pipeline (online, bounded, measurable)

Five ordered steps, all deterministic — no vectors in v0.1 (ADR-010):

1. **Exact decision-cache hit** — snapshot hash + feature schema + strategy
   version + model checkpoint + adapter + policy version must all match.
2. **Exact state-signature lookup** — same bucket signature.
3. **Instrument + strategy history** — indexed SQL, recent N decisions.
4. **Similar states** — bucket signature narrows candidates via index, then
   bounded numeric kNN over normalized features ranks top-k (ADR-011).
5. **Bounded graph traversal** — one hop default, two max, ≤ 20 neighbors,
   edges filtered by validity intervals.

Budget: < 10 ms per decision-cycle retrieval. Hit/miss per step is a
first-class metric. Embeddings are reconsidered only if measured retrieval
quality after S5 justifies a third model in RAM.

## 4. Knowledge graph rules

Node types: Instrument, Venue, MarketRegime, FeaturePattern, Strategy,
StrategyVersion, Decision, Order, Trade, Outcome, Policy, Evidence.
Every edge carries provenance (`source_type`, `source_ref`), optional
confidence and validity interval. An edge whose fact was wrong is closed
(`valid_until_ns = now`), never deleted — the history of what LOLA believed
is itself decision evidence.

## 5. TTL & compaction (S4 implements; policy fixed now)

| Class | TTL (default) | Compaction action |
|---|---|---|
| `model_payloads` | 30 d | payload → blob; row keeps hash |
| `session_bulk` | 90 d | payload → blob + summary row |
| `feature_snapshots` | 180 d | `features_json` → blob |
| `decision_runs`, `decision_outcomes`, `paper_orders`, `paper_fills`, `risk_checks`, `graph_nodes` | never | — |

Compaction is a job, not a trigger: single-writer friendly, restartable,
idempotent (blob presence == compacted). Deleting a blob requires that no
live row references it; blobs unreferenced after compaction are garbage-
collected by hash scan.

## 6. Integrity rules (unchanged from RFC-001, still true)

Decisions are never overwritten · outcomes append when measured · changed
strategy/policy/feature-calculation creates a new version, never a mutation ·
orders carry unique idempotency keys · money never travels as REAL —
canonical decimal strings only · every backtest names its exact dataset.
