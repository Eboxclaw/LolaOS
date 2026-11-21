# LOLA ACE

**ACE — Adaptive Cognitive Execution.** An Android-first, local-first AI
application and execution runtime. The first vertical slice is LFM3-1.7B
text streaming on a phone, on-device, with honest measurements.

> A document, type contract or successful compile is not evidence that a
> model runs. Evidence is a golden output on hardware and a measured
> number. — [docs/ace](docs/ace/README.md)

**ACE documents:** [ACE README — direction & status](docs/ace/README.md) ·
[ARCHITECTURE](docs/ace/ARCHITECTURE.md) ·
[INFERENCE_CONTRACT](docs/ace/INFERENCE_CONTRACT.md) ·
[ANDROID_POC + acceptance checklist](docs/ace/ANDROID_POC.md) ·
[ANDROID_REFERENCE_BACKEND](docs/ace/ANDROID_REFERENCE_BACKEND.md) ·
[BRAND_IDENTITY](docs/ace/BRAND_IDENTITY.md)

## What exists now

- **`android/`** — Kotlin/Compose app (`ai.lola.ace`): Conversas, Modelos,
  Diagnóstico. Pinned model catalogue (`models/catalog.v1.json`), resumable
  verified downloads, private SQLite chat history, streaming local inference
  through a **backend-neutral Kotlin contract** currently served by a
  pinned llama.cpp CPU reference backend (`b11960`). No simulated output.
- **`crates/ace-*`** — Rust contracts: `ace-core` (execution budgets,
  Decision/Streaming pipelines, cooperative cancellation), `ace-inference`
  (backend-neutral model/session/generation contract), `ace-models`
  (catalogue validation, revision pinning, streaming SHA-256, lifecycle).
  Contracts only — not yet wired into the app (the Kotlin↔Rust bridge is a
  queued slice).
- **Golden suite + device harness** — `evals/ace/` (fixed pt-BR prompt set,
  capture helper) for the on-device validation sequence.

**Not yet validated:** physical-device golden outputs, TTFT/prefill/decode,
PSS/thermal. The reference backend is not the LOLA Native Engine; the native
engine must earn adoption through same-device benchmarks
([ANDROID_POC](docs/ace/ANDROID_POC.md) §5).

## Engine lab (legacy workspace)

The `crates/lola-*` Rust workspace — harness kernel, decision ledger,
lola-memory (SQLite/graph/kNN), and the **nano-RIPL LFM3 engine** (own GGUF
reader, NEON kernels, q8_0 KV, quality gates: ppl-Δ, NIA, token-identity) —
is retained in full as the **engine lab**: the bench where LOLA Native Engine
kernels and quality gates are built and proven before any adoption into ACE.
It is developed, tested and benchmarked on its own spine
([ROADMAP](docs/ROADMAP.md) S-slices, F-fruits); nothing in it is claimed as
a shipping product surface.

- [ENGINE — our LFM3-specialized engine](docs/ENGINE.md) ·
  [NANO_RIPL — model pairs, residency, compute tiers](docs/NANO_RIPL.md) ·
  [OPTIMIZATIONS](docs/OPTIMIZATIONS.md) ·
  [BENCHMARKS — recorded numbers vs budgets](docs/BENCHMARKS.md)
- Kernel & memory docs ([HARNESS](docs/HARNESS.md),
  [MEMORY](docs/MEMORY.md), [PIPELINE](docs/PIPELINE.md),
  [ROUTER](docs/ROUTER.md)) describe the lab's decision-loop lineage; the
  trading vertical (Hyperliquid) they target is **not** an active ACE
  surface.

## Quickstart

```bash
# Android app (CI parity: Gradle 9.5.0, NDK 29.0.13113456, CMake 3.31.6)
cd android && gradle :app:testDebugUnitTest :app:assembleDebug

# Engine lab
cargo test --all
```

## Process

[ROADMAP — slices & exit criteria](docs/ROADMAP.md) ·
[DECISIONS — ADR log](docs/DECISIONS.md) ·
[RESEARCH](docs/RESEARCH.md) · [LIQUID — positioning deep-dive](docs/LIQUID.md)
