# LOLA ACE migration audit — initial repository boundary

**Status:** Initial audit; documentation-only proposal  
**Audited repository:** https://github.com/Eboxclaw/LolaOS  
**Target direction:** Android-first LOLA ACE, LFM3-first local inference  
**Audit date:** 2026-11-19

## Executive summary

The current LolaOS repository is a Rust workspace whose documented product direction is an agentic operating system for trading. It contains potentially reusable runtime foundations, but it is not yet the Android chat and local-model-management proof of concept now being built under the LOLA ACE direction.

The safe migration strategy is **separation, not wholesale deletion**:

1. Preserve generic runtime, inference, memory and observability capabilities only where their code and contracts are genuinely domain-neutral.
2. Isolate trading-specific strategy, venue, portfolio, market-data and execution concepts behind a separate vertical/application boundary.
3. Build the Android application and model lifecycle as a first-class product surface.
4. Validate every claimed reusable component against source, tests and executable behaviour before moving or deleting it.

This PR deliberately adds documentation only. It does not remove trading code, rename crates, change dependencies or claim that the current engine can already execute LFM3 on Android.

## Repository shape observed

The root workspace declares these crates:

- `lola-memory`
- `lola-harness`
- `lola-infer`
- `lola-venue`
- `lola-replay`
- `lola-cli`

The workspace is Rust 2021 and declares Rust 1.85 as its minimum version. Root dependencies include SQLite (bundled), Serde, SHA-256, UUID, tracing, Criterion, Arrow and Parquet.

The README describes the product as an agentic operating system for trading, documents a synthetic-bars decision loop and provides a Cargo CLI quickstart. It states that key management and UI/UX are outside the current scope.

## Initial domain-boundary assessment

### Candidate generic foundations — inspect before reuse

These areas may contain useful foundations for LOLA ACE, but their names alone do not prove they are portable:

- **Harness / orchestration:** lifecycle, scheduling, cancellation, bounded execution, event propagation and error handling.
- **Inference:** model loading, tokenizer integration, tensor execution, sampling, streaming and cache management.
- **Memory:** SQLite persistence, event history, snapshots, retrieval interfaces and session isolation.
- **Replay:** deterministic event replay and test fixtures, if not coupled to market semantics.
- **CLI:** useful for developer diagnostics and benchmark tooling, but not a substitute for the Android application.

For each candidate, inspect public types, dependencies, tests and imports. A module is generic only if it does not depend on trading vocabulary or assumptions.

### Trading-specific vertical — isolate, do not silently delete

The `lola-venue` crate is strongly domain-oriented by its name and should be treated as trading-specific until proven otherwise. The README's synthetic market bars and trading decision loop are also domain-specific product examples.

Search the remaining crates for concepts such as orders, positions, fills, venues, exchanges, market data, symbols, candles, portfolio, risk limits and execution. These should not leak into LOLA ACE's generic runtime APIs.

## Target LOLA ACE boundary

The intended product is an Android application with a local chat experience and a model library:

- Browse, download, verify, install, select and delete model artefacts.
- Chat locally with LFM3-1.7B and stream generated tokens.
- Persist conversations and resume after app restart.
- Work offline after model installation.
- Expose diagnostics: time to first token, prefill/decode throughput, memory use, cache use, backend and thermal state where available.

The initial vertical slice is **LFM3-1.7B streaming**. Decision models and richer agent execution can follow without blocking the first app milestone.

Suggested ownership boundary:

- **Android / Kotlin:** application lifecycle, UI, permissions, downloads, notifications and platform integration.
- **LOLA ACE core:** scheduling, session state, cancellation, policy, event ledger and backend selection.
- **Native inference:** tensor kernels, model-specific operators, cache/state management and streaming.
- **Model artefacts:** accept supported upstream formats; do not require model authors to produce a proprietary `.lola` format.

## Migration rules

1. No destructive cleanup until a file-level dependency and ownership map exists.
2. Do not move trading code into generic crates merely to preserve it; keep it in a clearly named trading vertical.
3. Avoid a repository-wide rename in the first migration PR.
4. Do not claim Android, GPU or NPU acceleration until built and measured on the target device.
5. Keep llama.cpp, ONNX Runtime and LiteRT-LM as measurable reference backends where useful; do not confuse a model file format with an inference runtime.
6. Treat LFM3 cache/state isolation as a correctness requirement: independent sessions must not share mutable convolution or KV-cache state.
7. Keep model downloads verifiable (pinned source/revision, size and cryptographic hash where available).

## Work required before implementation PRs

- [ ] Inventory every file in each crate and classify it as generic, trading-specific, mixed or obsolete.
- [ ] Trace crate dependency edges and identify trading types crossing crate boundaries.
- [ ] Inspect inference code for actual LFM3 operator coverage and model-format assumptions.
- [ ] Inspect memory schema and session isolation behaviour.
- [ ] Run the existing test suite and record baseline results.
- [ ] Identify Android build/toolchain status; do not infer Android readiness from Rust alone.
- [ ] Propose explicit crate boundaries and migration sequence in a follow-up ADR.

## Proposed follow-up sequence

1. Complete file-by-file audit and run baseline tests.
2. Extract domain-neutral contracts only where tests prove independence.
3. Create Android app shell and model manager.
4. Integrate a reference LFM3 backend to prove the end-to-end offline chat workflow.
5. Implement and benchmark the LOLA native CPU inference path.
6. Add GPU/NPU paths only after operator coverage and device measurements justify them.

## Scope of this PR

- Adds this audit and migration boundary document only.
- No source-code behaviour changes.
- No trading code deleted or moved.
- No performance or compatibility claims are made.

This is intentionally a conservative first PR: it records the direction and prevents an unsafe cleanup before the source has been mapped.
