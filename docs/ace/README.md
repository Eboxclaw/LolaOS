# LOLA ACE

**Adaptive Cognitive Execution** — Android-first, local-first AI application and execution runtime.

This directory is the canonical home for the LOLA ACE architecture. The existing root-level documentation describes the legacy LOLA trading product and remains preserved until a separately reviewed migration is approved.

## Product objective

Build an Android application with a chat-first experience and a model library. The user can discover, download, verify, install, select and remove local models, then converse with them offline with streamed output and persistent conversation history.

The first vertical slice is **LFM3-1.7B text streaming** on a Google Pixel 8 Pro (Tensor G3). This is a proof of concept, not a claim that the current repository already runs this model on Android.

## Canonical documents

- [Architecture](ARCHITECTURE.md) — runtime components, ownership and execution paths.
- [Inference contract](INFERENCE_CONTRACT.md) — backend-neutral API, streaming, state and cancellation.
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

## Current status

Architecture baseline only. Implementation readiness must be established by build, device tests, golden outputs and measured performance. A document or scaffold is not evidence that a runtime path works.
