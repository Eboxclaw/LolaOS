# LOLA Roadmap — Pizza Slices

Sequence-only: no dates, no estimates. A slice ships when its exit criteria
pass. S3 runs in parallel with S4–S7 (it never blocks them).

## Slice map

| # | Slice | Depends on |
|---|---|---|
| S0 | Foundation *(this repo state)* | — |
| S1 | Kernel harness, no models | S0 |
| S2 | nano-RIPL v0 (oracle backend) | S0 |
| S3 | Custom LFM3-optimized engine | S2 (parallel to S4–S7) |
| S4 | Memory & retrieval | S1 |
| S5 | d1 shadow mode on real candles | S2, S6 data optional for synthetic |
| S6 | Hyperliquid market data | S1 |
| S7 | Experiment ladder A0–A3 | S4, S5, S6 |
| S8 | HL testnet execution | S6, S7 |
| S9 | Agent escalation (LFM3) | S2, S5 |

## S0 — Foundation ✅ (this session)

Docs (RFC-002, MEMORY, NANO_RIPL, DECISIONS, ROADMAP + carve-out stubs),
Cargo workspace with 5 crates, trimmed SQLite schema, config files, CI.

**Exit:** `cargo build && cargo test` green · `lola doctor` passes ·
migrations apply to an empty DB · clippy `-D warnings` clean.

## S1 — Kernel harness (no models, no venue)

Synthetic candle generator → feature engine → state signature →
deterministic strategy stub → risk checks → internal paper fill → decision
ledger write. Event bus, scheduler skeleton, loop metrics, restart
recovery.

**Exit:** full decision loop runs on synthetic bars without any model
loaded · restart mid-loop recovers with no duplicate orders (idempotency
keys) · per-stage latency metrics emitted · a `SELL` that would short or a
leverage request above policy is blocked and logged.

## S2 — nano-RIPL v0 (oracle, omni pair)

Model registry live · pair-based residency manager · `reference-llamacpp`
(llama.cpp ≥ b11960, vendor-pinned) linked in-process · **omni pair
bring-up** (d1-Omni-2B Q8 + LFM3-Omni-2B Q4, mmproj sidecars, media
session entries) · decision contract + conformance harness (noul/choice/
score via systemone-style readout) · golden outputs recorded ·
load/swap/latency benchmarks in CI. *Day-0 task: vendor-pin GGUF
revisions + checksums (official GGUFs verified to exist; d1-Omni is
Q8-only — see risk register).*

**Exit:** omni pair loads cold < 5 s / warm-swap < 1 s · d1 decision
warm < 200 ms at 1-question, < 800 ms at 5k-token states · golden set
recorded and versioned · conformance failures fail CI.

## S3 — Custom tri-hybrid engine (parallel track)

GGUF weight container → 4-bit CPU kernels → LFM3 tri-hybrid path: gated
short-conv k=7 streaming state, SWA-256 ring buffer, global attention
with partial RoPE (θ=1e6, 50% head_dim), head-wise sigmoid gate, sandwich
norm, GQA 16/4 → **d1 readout port** from `modeling_d1_lfm3_*.py`
(extraction/boxes/multiple — not in llama.cpp today) → golden parity →
then Vulkan/wgpu kernels (AMD + Tensor Mali; Adreno probe→CPU). Never
blocks other slices.

**Exit:** parity with oracle on the full golden set incl. d1 typed
answers · ≥ oracle speed on decision-contract prompts · resident set
within profile budget.

## S4 — Memory & retrieval

Graph node/edge writers · bucket signatures + bounded numeric kNN ·
decision cache with full-dependency invalidation · TTL/compaction jobs ·
FTS5 sync · retrieval metrics.

**Exit:** retrieval < 10 ms at 100k decisions · similar-state results
reproducible · compaction moves payloads to blobs and reclaims space with
zero hash-loss · cache invalidates on every dependency change.

## S5 — d1 shadow mode

d1 classifies real (or S6-backfilled) HL candles; labels from deterministic
rules; calibration report (ECE/Brier per regime, abstention rate).

**Exit:** calibration report exists and is versioned · explicit go/no-go
decision recorded in `DECISIONS.md` before d1 may act in A1 · d1
probabilities are never displayed as trade-profit probabilities.

## S6 — Hyperliquid market data

Public market data via `hyperliquid-rust-sdk`: candle backfill (BTC-PERP,
ETH-PERP, one spot pair), Parquet partitioning + manifests, gap/duplicate/
quality audit, dual timestamps preserved.

**Exit:** clean, gap-audited history for the watchlist · replay is
deterministic · dataset manifests hash exact inputs.

## S7 — Experiment ladder A0–A3

`lola-replay` conservative engine (next-bar fills, fees, spread penalty,
funding accrual, no limit-fill optimism) · A0 deterministic baseline →
A1 +d1 → A2 +retrieval → A3 +agent escalation · lookahead audit tooling ·
results separated into trading performance / decision quality /
latency-resource buckets.

**Exit:** all four configs run on identical datasets, reproducible from
manifests · lookahead audit passes · per-component value (or lack of it)
is quantified — this is the slice that answers whether LOLA deserves to
exist.

## S8 — Hyperliquid testnet execution

Throwaway API wallet (per `docs/KEY_MANAGEMENT.md`, once that discussion
happens) · risk-clamped signed orders · reconciliation vs local ledger ·
kill switch · leverage experiments up to 10x for calibration data.

**Exit:** stable multi-day testnet loop · zero instances of an order
bypassing ring 0 · reconciliation matches venue state · kill switch works
in a live drill.

## S9 — Agent escalation (LFM3)

Escalation conditions (novel state, d1 abstain, conflict with strong
historical pattern, operator request) · bounded tool registry · research
loop reading the ledger and graph.

**Exit:** agent invoked only on escalation-worthy states (measured) ·
escalations produce ledger-recorded investigations · loop p95 unaffected.

## Later (unordered)

Nado (Inchain) adapter · strategy discovery ladder B0–B3 · specialist
models (VL/Audio/Base) · Android target + Compose UI · mainnet
leverage-ladder unlock process · equities/ETFs reconsidered ·
**Hexagon/QAIRT plugin (Snapdragon NPU)** · **LiteRT-Tensor plugin
(Pixel 10+)** · **d1-Omni Q4 quantization** (unblocks omni pair on
constrained devices) · voice/video frontends (media session entries
already exist) · LFM Open License v1.0 legal review (blocking gate for
any distribution).
