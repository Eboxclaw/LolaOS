# LOLA Decision Log (ADRs)

Decisions locked with the owner on 2026-11-10 after five interrogation
rounds. Format: context → decision → rejected alternatives.

---

**ADR-001 — Identity: everything LOLA.** The product, repo, package and
docs are LOLA ("LOLA OS"). The workspace folder `EdgeTOS` is an artifact of
history, not a brand. *Rejected:* EdgeTOS branding; split naming.

**ADR-002 — Rust core, single process, no sidecars.** Mobile constraints
apply from day one: no fork assumptions, bounded memory, single-writer
SQLite, GGUF-compatible weights. The user's direction: "multiplatform from
start with Rust and Compose or Kotlin". *Rejected:* Python-first (RFC-001's
own recommendation — dies with the mobile requirement); any Python
sidecar; TS anywhere.

**ADR-003 — Own nano-RIPL inference layer.** Inspired by llama.cpp
techniques (mmap, quantization), RIPL economics (~1 GB/model @ 4-bit,
sub-50 ms ambition), Hermes memory patterns and the Pi harness — never
forked or copied. A custom engine optimized for LFM3-family constructions
(hybrid conv + attention). **Honest risk note:** this reverses RFC-001's
"no custom inference engine in the first POC" and is the highest-risk item
in the roadmap. Mitigation: an in-process `reference-llamacpp` oracle
backend serves while the custom engine is built; the custom engine ships
only after kernel-by-kernel golden-test parity. *Rejected:* llama.cpp
bindings as the permanent answer; candle (pure Rust) — weaker model
coverage; Python inference sidecar.

**ADR-004 — NautilusTrader dropped from v0.1.** Its supported surface is
Python; embedding its Rust crates directly is unsupported. Its
conservative-fill philosophy survives in `lola-replay` defaults.
*Rejected:* Rust core + Nautilus Python sidecar (two runtimes forever,
Android harder); embedding nautilus Rust crates (unsupported path).

**ADR-005 — Hyperliquid first; Nado designed-for, built later.** Perps +
spot, leverage 1–10x in the schema, via the official
`hyperliquid-rust-sdk`. Nado (Inchain exchange) gets a venue adapter behind
the same `Venue` trait when its API surface is validated. *Rejected:*
NautilusTrader's first-party HL adapter (good, but Python-bound — recorded
as the revisited-alternative if LOLA ever wants a desktop-only backtest
beast); CEX adapters early.

**ADR-006 — Model set.** Active: d1-2B (decision) + LFM3-1.7B (agent),
Q4_K_M. The other five LFM3 checkpoints are registry metadata only.
d1 probabilities are model probabilities, never trade-profit probabilities.

**ADR-007 — Profile-based residency.** Laptop: both models resident.
Constrained/Android: d1 resident, LFM3 on-demand via warm page cache.
Decision models are never downgraded; a plan without a resident decision
model refuses to boot. *Rejected:* always-both-resident (Android budget);
d1-only-resident everywhere (wastes laptop RAM).

**ADR-008 — Leverage: testnet free, mainnet ladder.** Schema supports 10x.
Testnet may run any leverage (it exists to gather liquidation/funding/
calibration data). Mainnet starts 1x; rungs 2x→5x→10x unlock only via
versioned policy changes backed by measured evidence, approved by humans.
The risk engine hard-clamps; models can never raise caps. *Rejected:*
10x mainnet day one (unmeasured 2B-model calibration holding 10x is how
accounts die); 1x everywhere including testnet (starves the data).

**ADR-009 — Memory: immutable core + TTL'd bulk; hybrid semantics.**
Decisions/outcomes/orders/fills/risk immutable forever; facts canonical
(upsert per `(scope, canonical_key)`); graph edges versioned with validity;
bulky payloads compact to content-addressed blobs on TTL. "Over-storing" =
storing a payload twice or keeping bulk in hot rows past TTL.
*Rejected:* keep-everything-forever (Android storage); aggressive lossy
compaction (kills debuggability); full event sourcing (replay cost grows
with history); current-state-only (kills decision lineage — LOLA's entire
reason to exist).

**ADR-010 — No vectors in v0.1.** Deterministic signatures + SQL buckets +
graph + FTS5 only. Reconsidered only if measured retrieval quality after
S5 justifies a third model competing for edge RAM. *Rejected:* local
embedding model from day one; external embedding API in any path.

**ADR-011 — Similarity = bucket signature + bounded numeric kNN.** Coarse
signature narrows via index; weighted normalized-feature distance ranks
top-k. Deterministic, explainable, fast. *Rejected:* exact-signature-only
(too blunt); deferred similarity (A2 experiment impossible without it).

**ADR-012 — Risk engine is ring 0.** Deterministic, independent of both
models, fail-closed. Stable check names fixed in code. Cache hits never
bypass it.

**ADR-013 — Ruthless cut; sequence-only roadmap.** v0.1 = harness + one
venue + Experiment A. Equities, strategy discovery (B ladder),
specialists: out. No dates; exit criteria per slice. RFC-001's 12-epic
backlog contradicted its own "prove it deserves to exist first" principle.

**ADR-014 — Key management & UI/UX carved out.** Separate discussions,
zero assumptions encoded. The only trace in v0.1 is
`venue.yaml: credentials.mode: none` and this doc's question list.
See `KEY_MANAGEMENT.md`, `UI_UX.md`.

**ADR-015 — d1 shadow mode first.** d1 classifies real candles and its
calibration (ECE/Brier per regime) is measured before it may accept or
veto live signals. Go/no-go recorded here before A1 runs with d1 active.

**ADR-016 — Venue trait is the only venue boundary.** All venue knowledge
lives behind `lola-venue::Venue`. Nado drops in later without kernel
changes.
