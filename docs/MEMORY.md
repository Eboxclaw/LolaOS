# LOLA Memory — Storage, Knowledge Graph, Retrieval (v2)

Companion to `docs/HARNESS.md`. Crate: `lola-memory`. Schema of record:
`migrations/0001_init.sql` + `migrations/0002_omni_sessions.sql`.

The most important subsystem in LOLA. Everything else exists to produce
decisions worth remembering and to reuse them without paying for them twice.

## 1. Philosophy (ADR-009)

**Immutable core + TTL'd bulk**, **hybrid** update semantics:

| Data class | Semantics | Lifetime |
|---|---|---|
| Decisions, outcomes, orders, fills, risk reports | Append-only, immutable | Forever |
| Canonical facts (memory items, instruments, regimes) | Upsert — one row per `(scope, canonical_key)` | Current + timestamps |
| Graph edges | Versioned: `valid_from_ns`→`valid_until_ns`, never deleted | History queryable |
| Model payloads, session bulk, snapshot bodies, media | Content-addressed blobs, TTL-compacted | Rows + hashes forever; payloads per TTL |

"Over-storing" = **storing a payload twice, or keeping bulk in a hot row
past its TTL.** Compaction moves payloads, never facts.

## 2. Schema map

```
 DURABLE SESSIONS               APPEND-ONLY LEDGER (immutable core)
 ┌───────────────────┐          ┌───────────────────────────┐
 │ sessions          │          │ decision_runs             │◄─ risk_checks
 │  memory_snapshot_ │          └────────────┬──────────────┘   paper_orders
 │  hash ────────────┼─► L0    ┌─────────────▼─────────────┐    └─ paper_fills
 └─────────┬─────────┘  frozen │ decision_outcomes         │
           │ 1:N               │ (horizons 1/3/6/12/24)    │
 ┌─────────▼─────────────────┐ └───────────────────────────┘
 │ session_events (Pi tree)  │        CANONICAL FACTS / GRAPH / SEARCH
 │  id · parent_id · type:   │ ┌──────────────┐ ┌──────────────────────┐
 │  message · model_change · │ │ memory_items │ │ graph_nodes/edges    │
 │  compaction · ctx_edit ·  │ │ (scope,key)  │ │ provenance+validity  │
 │  custom · media           │ │ UNIQUE key   │ │ hops ≤2, ≤20 neigh.  │
 │  payload_uri+hash ────────┼─┼──► derived: session_search / memory_search (FTS5)
 │  media_uri · absorbed_ids │ └──────────────┘ └──────────────────────┘
 └───────────────────────────┘
        blobs: data/blobs/<sha256[:2]>/<hash>.zst  ← TTL compaction target
```

## 3. Session tree & compaction lifecycle (ADR-020/022)

```
 session start ──► append events (tree grows, leaf = active branch)
        │
        │ context near limit
        ▼
 ┌─ compaction (one transaction) ─────────────────────────────┐
 │ 1. summarize absorbed range (summarizer = d1, slot A)      │
 │ 2. insert compaction row: summary + first_kept_event_id    │
 │    + absorbed_event_ids[] (witnesses)                      │
 │ 3. original rows stay (archived), still FTS-searchable     │
 │ 4. KV twin: llama_memory_seq_rm(evicted range)             │
 └────────────────────────────────────────────────────────────┘
        │ Android process death
        ▼
 suspend: KV state_seq_save + ledger row naming it → resume replays tail
```

### KV ↔ ledger twin operations

| Ledger operation (SQLite) | KV operation (engine) |
|---|---|
| append turn events | prefix reuse — no re-prefill |
| compaction + `absorbed_ids` | `seq_rm` evicted range |
| context-shift record | `seq_add` position shift |
| suspend/resume | `state_seq_save`/`load` (best-effort) |
| `model_change` entry | slot swap under residency policy |

## 4. Memory tiers (Hermes L0/L1/L2 pattern, in one DB)

| Tier | Contents | Size | Injection |
|---|---|---|---|
| **L0 active** | curated memory snapshot (preferences, active strategy versions, instrument universe) | ~100–800 tok | frozen into system prompt at session start (law 1) |
| **L1 working** | graph facts + recent decisions pulled by retrieval | ~2k tok budget | per-turn, retrieval-decided |
| **L2 full** | entire ledger + blobs + FTS corpus | unbounded | never injected wholesale; query-only |

Writes to L0 go through the memory tool's **add/patch/remove** verbs only
(no read — it's auto-injected; no replace — models wiping multi-fact
entries is a documented Hermes failure mode).

## 5. Retrieval pipeline (ADR-010/011)

Budget **<10 ms** per decision cycle; hit/miss per step is a first-class
metric; all steps deterministic (no vectors in v0.1).

```
 1. decision cache ──hit──► serve (never bypasses risk)
        │ miss          [9 exact dependencies must match:
        ▼                snapshot·feature-schema·question-schema·strategy·
 2. signature lookup      checkpoint·adapter·policy·instrument·timeframe]
        │ miss
        ▼
 3. instrument+strategy history (indexed SQL, recent N)
        ▼
 4. similar states: bucket signature narrows (idx) → bounded numeric
    kNN ranks top-8 of ≤512 candidates (normalized features)
        ▼
 5. graph traversal: ≤2 hops, ≤20 neighbors, validity-filtered
        ▼
 FTS5 (session/memory search) — only when explicitly asked
```

## 6. Blob store & TTL

`data/blobs/<sha256[:2]>/<hash>.zst`. Rows keep `payload_uri`+`payload_hash`
forever; blob presence == compacted. Unreferenced blobs are GC'd by hash
scan after compaction. Media (omni) blobs referenced by `media_uri` on
session events follow the same scheme with their own retention class.

| Class | TTL | Action |
|---|---|---|
| `model_payloads` | 30 d | payload → blob |
| `session_bulk` | 90 d | payload → blob + summary |
| `feature_snapshots` | 180 d | `features_json` → blob |
| `media` | policy TBD (S4) | keep while any decision references |
| `decision_runs`, `decision_outcomes`, `paper_orders`, `paper_fills`, `risk_checks`, `graph_nodes` | never | — |

## 7. Optimization considerations — the review list

Numbered for our next decision round; each needs a measurement plan before
a choice is made:

1. **d1 transient-KV sizing** — decision slot prefill peak = f(state tokens
   + 16-question set). Measure peak RSS at 1k/5k/20k-token states; decide
   whether prefill scratch is pooled with slot B's KV allocator.
2. **Question-set packing** — d1 answers up to 16 questions per pass;
   larger sets are faster but slightly less accurate. Benchmark accuracy
   vs latency at 4/8/16 and fix the default.
3. **WAL checkpoint cadence** — default passive checkpoints vs explicit
   `wal_checkpoint(TRUNCATE)` every N decisions; Android flash wear vs
   read-latency trade.
4. **Android durability dial** — `synchronous=FULL` vs `NORMAL` on mobile
   (power + flash wear vs at-most-one-checkpoint loss). Profile-gated.
5. **FTS5 tokenizer** — `unicode61` default vs `trigram` (only if CJK
   research queries matter); index size on-device.
6. **kNN placement** — in-SQL candidate scan vs loading ≤512 normalized
   vectors into memory per query; crossover point measurement.
7. **Graph edge proliferation bound** — auto-SIMILAR_TO edges can explode
   O(n²); cap auto-edge creation per snapshot and add edge-TTL.
8. **Quantized KV for slot B** — q8_0 KV halves agent KV (~0.4→0.2 GB at
   32K) at some quality cost; conformance-gate it.
9. **Blob mmap vs heap** — mmap on laptop, copy-on-open on Android (page
   cache pressure); measure cold-blob fetch latency both ways.
10. **Ledger page layout** — `session_events` grows fastest; consider
    monthly partition tables if scan latency degrades (measure at 1M rows).

## 8. Integrity rules (unchanged, still true)

Decisions never overwritten · outcomes append when measured · strategy/
policy/feature changes create versions, never mutations · unique idempotency
keys on orders · money as canonical decimal strings, never REAL · every
backtest names its exact dataset · every graph edge carries provenance.
