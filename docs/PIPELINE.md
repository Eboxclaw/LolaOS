# LOLA Pipeline — the non-inference spine, planned and budgeted

Companion to RFC-002 §7/§10, MEMORY.md, HARNESS.md. This doc plans the
**recall side** of both loops — DB, parsing, graph, memory, and (new,
ADR-041) vectors — as pipeline stages with budgets and gates, not as
afterthoughts. The concept it honors: *llama.cpp-class efficiency for LFM
models + Pi-minimal harness + Hermes memory + vector-graph DB for tool
calls*.

## 1. The two loops, end to end, with every stage owned

```
DECISION LOOP (per tick)                                budget (p95)
─────────────────────────────────────────────────────────────────────
bars ──► features            83 ns   [K1 kernels]
      ──► signature+hash     ~1 µs   [features.rs]
      ──► ledger snapshot    ~30 µs  [F4 target]  ── WAL, batched txn
      ──► RETRIEVAL          <2 ms   [R1/R2/R3]
      │    1. RAM decision-cache map (hash-keyed)     ~1 µs  [R2]
      │    2. exact signature (SQLite, cached stmt)   ~5 µs  [F4]
      │    3. history (indexed SQL, top-N)            ~50 µs
      │    4. bucket → numeric kNN top-8 of ≤512      <300 µs [R1: numeric cols]
      │    5. graph hop ≤2, ≤20 neighbors             <200 µs [R3: edge hot-cache]
      │    6. vector rerank (int8 SIMD scan)          <1 ms  [R4, gated]
      ──► [d1 decide]        engine-side (K5/F6)
      ──► router             ~1 µs   [ADR-036 table]
      ──► risk 15 checks     1.2 µs  [measured]
      ──► order+fill+events  ~130 µs [F4 → <30 µs]
                                                    total non-model < 5 ms

AGENT LOOP (escalation / chat)                         budget (p95)
─────────────────────────────────────────────────────────────────────
message ──► context build    <1 ms
           persona verbatim + frozen L0 (RAM, session-cached)
           + skill index (≤10 or search_tools)
           + recalled evidence: SAME retrieval stages as above
       ──► [slot B prefill]  prefix-KV reuse          TTFT gate [F3]
       ──► decode stream     tool-call XML parsed INLINE
       ──► tool dispatch     UniFFI, once per turn     <1 ms  [R6: batch]
       ──► result → ledger append (ref + hash)
       ──► next turn / done
```

The loops share the retrieval spine (stages 1–6) — one implementation,
two callers. **Everything non-model stays off the inference budget**:
the tick's model prefill and the agent's decode never wait on us.

## 2. Stage designs (respecting the original concept)

### R1 — retrieval storage: numeric, not JSON (kills the 8 ms)
The pre-S4 bucket-kNN scan measured **7.99 ms** because candidates carry
`features_json` TEXT and get ranked on byte-length. Fix: a
`feature_vectors` side table (or generated columns) — one `REAL` column
per normalized feature + signature + event_time, all index-covered:
candidates load as compact rows, distance is a SIMD dot over ~9 floats.
Gate: bucket-kNN **< 300 µs** at 100k rows (criterion, existing bench).
Also the `dot`/`fma_axpy` NEON primitives from the engine are reused —
same math, different payload.

### R2 — decision-cache hot map (RAM, before SQLite)
A bounded LRU keyed by the 9-dependency cache key (ADR-036: snapshot
hash + schema versions + policy + …). Hits never touch SQLite.
Invalidation events (policy bump, schema change) evict by prefix — the
ledger already records those as versions. Gate: hit-path **< 5 µs**;
miss-rate reported per tick (retrieval hit/miss is already a metric).

### R3 — graph traversal: edge hot-cache + precompiled hops
`graph_edges` is source-sorted and indexed; the ≤2-hop/≤20-neighbor
queries get **precompiled statements** (rusqlite `prepare_cached`) and a
RAM hot-cache of the adjacency for the active instrument's neighborhood
(typically tens of edges — it is *our* subgraph, small by design; MEMORY
§4 keeps provenance + validity). Cold hops stay SQL. Gate: 2-hop query
**< 200 µs** cold, **< 20 µs** warm; add to criterion.

### R4 — vectors, the on-device way (ADR-041 amends ADR-010)
No vector DB service, no HNSW on day one. **int8-quantized embeddings
stored as BLOBs, brute-force SIMD scan** — the F1 lesson says
memory-bound scans are the thing CPUs do best (46–77 GB/s measured); at
768-d int8, 100k vectors ≈ 75 MB ≈ ~2 ms worst-case full scan, and we
only rerank the ≤512 candidates the buckets narrowed. Role: **rerank
stage 6**, gated by measurement — enable per-query only when lexical
recall measurably underperforms (recall@k harness, SB). Source of
embeddings: an LFM3-1.7B embedding head later (one more use of our own
engine), or off-device precomputed at first.

### R5 — parsing: parse less, later
Hot paths read ids/status/columns, not payloads: recall paths return
`decision_id` + score + small columns; full `decision_json` is parsed
only when a case is actually injected (≤5 per decision, ADR-031) or an
operator opens a decision card. Session events append canonical JSON but
recalls use the covering indexes. Gate: recall-path parse cost ≈ 0 (no
full-row JSON on the tick path); assert in criterion bench.

### R6 — UniFFI tool-call batching (Android)
The bridge is batched once per agent turn: a `ToolBatch { calls }` over
FFI, results returned as refs+hashes (ADR-032 unchanged: payloads gated
by the support pass). Removes per-call JNI/JNA transit. Gate: 10-call
turn dispatch **< 1 ms** (mock executor bench; device numbers in S2m).

### F4 — SQLite pack (already a fruit; restated here as pipeline spine)
`synchronous=NORMAL`, `prepare_cached` everywhere, batched tick writes in
one txn, `wal_autocheckpoint` moved off the decode thread. Gates already
defined: write < 30 µs, p99 < 100 µs under 10K-write loop with
concurrent decode.

## 3. Memory (Hermes semantics) in this pipeline

- **L0 frozen snapshot**: built at session start, RAM-cached, injected
  verbatim (persona law). Cost amortized to ~0 after the first turn.
- **Session events**: append-only, WAL-batched — never on the recall path.
- **`memory_search` (FTS5)**: the Hermes recall primitive; prepared
  statements + the same hot-map pattern for the top-k system prompts.
- **"Dreaming" (ADR-030)**: the consolidation job runs **between ticks**
  (WorkManager idle window), reading Parquet/ledger batches — it shares
  nothing with the hot path by construction.

## 4. Budget table (extends RFC-002 §10; all gates criterion-backed)

| Stage | Budget | Gate |
|---|---|---|
| features + signature | < 5 µs | measured (83 ns + µs) |
| ledger snapshot + decision writes | < 30 µs each | F4 |
| retrieval: cache hit | < 5 µs | R2 |
| retrieval: full miss path (stages 2–5) | < 2 ms | R1/R3 |
| vector rerank (when enabled) | < 1 ms @512 cands | R4 + recall@k |
| router + risk | < 5 µs | measured |
| tick non-model total | < 5 ms | composite bench |
| agent context build | < 1 ms | scripted session |
| tool batch dispatch | < 1 ms / turn | R6 mock bench |

## 5. Order of attack & where it lands

R1 → R2 → F4 (they share the SQLite work) → R3 → R5 (falls out of R1) →
R6 (with Sn) → R4 (after SB's recall@k harness proves/disproves the
vector need per-query class). Roadmap gains an **SP (spine)** fruit
series; every item ships with its gate, per ADR-038 — including the
non-speed gates (R4's is recall quality, not latency).
