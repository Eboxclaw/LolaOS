# LOLA Roadmap — Pizza Slices

Sequence-only: no dates, no estimates. A slice ships when its exit criteria
pass. S3 runs in parallel with S4–S7 (it never blocks them).

## Slice map

| # | Slice | Depends on |
|---|---|---|
| S0 | Foundation ✅ | — |
| S1 | Kernel harness, no models ✅ | S0 |
| S2 | Oracle bring-up (omni pair, golden set) | S0 |
| S2m | Mobile de-risk (P1–P5 prototypes) | S2 |
| S3 | **lola-custom engine K0–K8 — the product** | S2 (starts immediately after) |
| S4 | Memory & retrieval | S1 |
| S5 | d1 shadow mode on real candles | S2, S6 data optional for synthetic |
| S6 | Hyperliquid market data | S1 |
| S7 | Experiment ladder A0–A3 (case bank in A2) | S4, S5, S6 |
| S8 | HL testnet execution | S6, S7 |
| S9 | Agent escalation (LFM3) | S2, S5 |
| Sn | Native tool layer (categories + Kotlin bridge) | S1, Android app scaffold |

## S0 — Foundation ✅ (this session)

Docs (RFC-002, MEMORY, NANO_RIPL, DECISIONS, ROADMAP + carve-out stubs),
Cargo workspace with 5 crates, trimmed SQLite schema, config files, CI.

**Exit:** `cargo build && cargo test` green · `lola doctor` passes ·
migrations apply to an empty DB · clippy `-D warnings` clean.

## S1 — Kernel harness (no models, no venue) ✅ (2026-11-12)

*Shipped: features/signature/risk/strategy (949b590), ledger + loop +
crash-recovery idempotency (0b94a3a), criterion benchmarks
(docs/BENCHMARKS.md). All exit criteria met; numbers: full tick 169 µs,
risk 1.2 µs, retrieval kNN 8 ms @100k.*

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
recorded and versioned · conformance failures fail CI · mobile
instrumentation live (anon-vs-file-backed RSS, thermal headroom, tok/s
during slot-A bursts). *The oracle is a development-time dependency
only — nothing of it ships (ADR-003 rev.).*

## S2m — Mobile de-risk (P1–P5, `docs/MOBILE_FIRST.md` §5)

P1 CPU dual-resident harness (any device; concurrency math + anon RSS) ·
P2 Hexagon-in-APK smoke on a retail device · P3 LFM-on-Hexagon op
coverage + sustained decode · P4 mmap-eviction stress · P5 thermal
envelope (30–60 min realistic duty).

**Exit:** P1 numbers on a real phone (the go/no-go for on-device dual
residency) · P2 works-or-documented-blocker on retail hardware · memory +
thermal dashboards exist · **benchmarked on Pipette surfaces** (the
numbers Liquid reads — LIQUID.md §7). *P2–P5 may resolve after S3 starts; they gate
NPU promises, not the engine.*

## S3 — lola-custom engine (K0–K8) — **the product** (ADR-003 rev.)

Starts immediately after S2's golden set exists; full design in
`docs/ENGINE.md`. K0 weights container + GGUF import/repack → K1 conv
kernel → K2 SWA attention → K3 global attention + partial RoPE → K4
gate/norm fusion → K5 d1 readout port → K6 dual-slot scheduler → K7
wgpu/Vulkan (AMD laptops) → K8 NPU plugin ABI for our graph → **K9
LiquidCodec voice path** (encoder · GFSQ parallel prediction · spectral
decoder; reuses K1–K3).

**Exit:** each K-gate = oracle parity + SLO · e2e golden set passes on our
kernels alone (oracle unplugged) · dual-slot duty cycle within thermal
envelope on a reference phone.

## S4 — Memory & retrieval

Graph node/edge writers · bucket signatures + bounded numeric kNN ·
decision cache with full-dependency invalidation · TTL/compaction jobs ·
FTS5 sync · retrieval metrics.

**Exit:** retrieval < 10 ms at 100k decisions · similar-state results
reproducible · compaction moves payloads to blobs and reclaims space with
zero hash-loss · cache invalidates on every dependency change.

## S5 — d1 shadow mode

d1 classifies real (or S6-backfilled) HL candles; labels from deterministic
rules; calibration report (ECE/Brier per regime, abstention rate). The
router's pass-forward reference tables (`config/router.yaml`, ADR-036)
already scaffold the S1 loop; S5 is what makes their numbers real.

**Exit:** calibration report exists and is versioned · explicit go/no-go
decision recorded in `DECISIONS.md` before d1 may act in A1 · **router
thresholds unlocked: PROVISIONAL constants replaced via versioned policy
bumps backed by ECE per answer type** · d1 probabilities are never
displayed as trade-profit probabilities.

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

## Sn — Native tool layer (ADR-033/034)

UniFFI tool-call bridge (Rust core ⇄ Kotlin executor) · runtime-permission
flow + confirm-on-write FAB UI · first read-only categories live
(calendar / time_alarm / notifications / device) · `search_tools` once
the registry passes [`MAX_PROMPT_TOOLS`] · accessibility stays empty and
gated. Depends on the Android app scaffold (Later).

**Exit:** agent lists events / next alarm / recent notifications via
tools on a real device · every write shows exactly one confirmation tap ·
tool results enter context only through the support-pass gate ·
permissions requested lazily per tool.

## Fruits — benchmark-gated optimization slices (ADR-038, docs/OPTIMIZATIONS.md)

- **SB** — Benchmark-harness expansion (gates first): quality mode on the
  golden harness (token-identity, KL, ppl-Δ, needle-in-haystack @32K/64K),
  TTFT session scripts, p99 write-loop, energy wrapper (powermetrics/
  perfetto), engine `bench` subcommand for Android parity. **QAD gate:**
  QAD-Q4_0 vs Q4_K_M vs Q8_0 on the golden set before any slot-A
  quantization (LFM3 QAD has no published evals — ADR-040).
- **F1+F11** — global-attn accumulation restructure + KV SoA layout
  (est. 8–20× @64K against the measured 77 GB/s floor; KL + token-identity
  gates). *F1a shipped: NEON fma_axpy, −31% @64K (85.1 ms), quality gate
  passed; F1b = fused exp + head parallelism.*
- **F2** — q8_0 KV for the 6 global layers (−73% memory; NIAH + ppl gates
  from SB).
- **F4** — SQLite pack (NORMAL, prepare_cached, batched decision txns,
  checkpoint off-thread; p99 gate).
- **F3/F6** — prefix-KV session persistence + d1 state-prefix caching
  (TTFT gates; KV-hash correctness).
- **F5/F7** — NEON exp (Android-weighted) + prompt-lookup speculative
  decoding (accept-rate ≥ 1.5 gate before enabling).

## Later (unordered)

Nado (Inchain) adapter · strategy discovery ladder B0–B3 · specialist
models (VL/Audio/Base) · **Android app scaffold (Compose Multiplatform +
UniFFI + cargo-ndk)** · "dreaming" consolidation job + beliefs layer
(ADR-030) · **wasmi sandbox for untrusted third-party skills** · mainnet
leverage-ladder unlock process · equities/ETFs reconsidered ·
**Hexagon/QAIRT plugin for OUR graph (K8, gated on S2m P2/P3)** ·
**LiteRT-Tensor plugin (Pixel 10+)** · **d1-Omni Q4 quantization**
(unblocks omni pair on constrained devices) · DSpark-style drafter for
LFM3-Omni (watch Liquid releases) · voice/video frontends (media session
entries already exist) · LFM Open License v1.0 legal review (blocking
gate for any distribution).
