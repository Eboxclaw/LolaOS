# LOLA ACE

**Adaptive Cognitive Execution** — Android-first, local-first AI application and execution runtime.

This directory is the canonical home for the LOLA ACE architecture. The existing root-level documentation describes the legacy LOLA trading product and remains preserved until a separately reviewed migration is approved.

## Product objective

Build an Android application with a chat-first experience and a model library. The user can discover, download, verify, install, select and remove local models, then converse with them offline with streamed output and persistent conversation history.

The first vertical slice is **LFM3-1.7B text streaming** on a Google Pixel 8 Pro (Tensor G3). This is a proof of concept, not a claim that the current repository already runs this model on Android.

## Canonical documents

- [Device testing](DEVICE_TESTING.md) — install the rolling signed APK on a real phone and run the A5 protocol.
- [Brand identity](BRAND_IDENTITY.md) — official LOLA logo and mascot usage.
- [Architecture](ARCHITECTURE.md) — runtime components, ownership and execution paths.
- [Inference contract](INFERENCE_CONTRACT.md) — backend-neutral API, streaming, state and cancellation.
- [Native Inference](NATIVE_INFERENCE.md) — LOLA Native Inference engineering track: three-engine strategy, LFM3 targets, benchmark programme, acceptance criteria (ADR-044).
- [ADR-043](ADR-043-MEMORY-SKILLS-WORKFLOW-GRAPHS.md) — memory, embeddings/RAG, Agent Skills and typed workflow graphs.
- [Android POC](ANDROID_POC.md) — app modules, screens, offline acceptance.
- [Model lifecycle](MODEL_LIFECYCLE.md) — catalogue, downloads, verification, installation and deletion.

## Locked direction

1. Android is the first product surface; the phone is the execution device.
2. Decision and Streaming are separate pipelines from the beginning. They may share an event/state ledger, but neither blocks the other's lifecycle.
3. The execution paradigm is selectable by policy: direct/reflex, FSM, DAG, HTN, GOAP, bounded ReAct, search/deliberation and recovery/reflection.
4. Routing, permissions, budgets and side-effect authorization remain host-owned and deterministic.
5. The first visible feature is local chat, not a complex autonomous agent.
6. LFM3 is the in-scope model family for this project vertical.
7. GGUF and ONNX are accepted interchange/import paths; no proprietary weight format is required of model authors.
8. The native engine must earn adoption through correctness and same-device benchmarks against reference runtimes.

## Current implementation status

Implemented in Rust:
- `ace-core`: initial run lifecycle, cancellation and host-owned usage-budget primitives.
- `ace-inference`: backend-neutral model/session/generation contract and ordered streaming chunk types.
- `ace-models`: schema-versioned catalogue validation, immutable artifact revision checks, streaming SHA-256 verification and lifecycle state transitions.

Implemented Android application:
- Kotlin launcher activity and Jetpack Compose navigation for Conversas, Modelos and Diagnóstico;
- reads the canonical `models/catalog.v1.json` asset;
- resumable private model downloads, SHA-256 verification and atomic installation;
- backend-neutral Kotlin inference contract;
- pinned CPU-only ARM64 llama.cpp reference adapter with `llama-common` Jinja chat rendering;
- independent native context/sampler per session and Kotlin token streaming;
- chat UI connected to real inference calls, with host-enforced output limits and cancellation;
- private SQLite conversation storage, multiple conversations, local history selection and partial-response persistence.

Not yet validated:
- physical-device (Pixel 8 Pro) inference, PSS, thermal and low-memory behaviour — the emulator baseline ([BENCHMARK_RECORD_EMULATOR_V1](BENCHMARK_RECORD_EMULATOR_V1.md)) is not device truth;
- emulator-validated already (2026-11-22, see the record): in-app download with real network-loss resume, SHA-256 install, golden pt-BR outputs with seed-42 byte-reproducibility, native TTFT/prefill/decode readings, cancellation with partial persistence, airplane-mode chat, restart persistence;
- incremental KV-cache reuse, foreground download guarantees and Kotlin↔Rust bridge;
- physical-device context budgeting, history compaction and conversation deletion UI;
- LOLA Native Engine implementation.

See [Android reference backend](ANDROID_REFERENCE_BACKEND.md) and [Android implementation status](ANDROID_IMPLEMENTATION.md) for details.

Implementation readiness must be established by passing CI, device tests, golden outputs and measured performance. A document, type contract or state machine is not evidence that a model can already be downloaded or run.


## Foundational intelligence layer — proposed in ADR-043

ACE will keep memory, skills and workflows as separate, composable foundations:

- Memory owns provenance-aware storage, lifecycle, lexical/structured retrieval and optional vector retrieval.
- Skills follow the Agent Skills open standard and describe reusable capabilities; they do not grant permissions or execute arbitrary code.
- Workflow Graphs are typed, versioned plans. Decision validates and schedules them; Streaming runs generation nodes and streams output.
- TurboQuant and FlashMem remain research candidates behind replaceable interfaces and evidence gates.

This ADR is proposed; these capabilities are not yet claimed as implemented in the ACE runtime.
