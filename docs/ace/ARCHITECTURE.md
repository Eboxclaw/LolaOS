# LOLA ACE runtime architecture

## 1. Design goals

- Run locally on Android with bounded memory, latency, thermal load and battery use.
- Provide a usable chat/model-management application before exposing advanced agent capabilities.
- Keep model execution independent of agent orchestration and product verticals.
- Support independent Decision and Streaming lifecycles, budgets and cancellation.
- Make the execution paradigm selectable at runtime without allowing the model to change host policy.
- Keep the system observable: every run records model/backend identity, timings, memory estimates and termination reason.

## 2. Component map

```text
Android app (Kotlin + Jetpack Compose)
  ├── Chat UI / conversation history
  ├── Model catalogue / download / install / delete
  ├── Permissions / lifecycle / diagnostics
  └── Kotlin ↔ Rust bridge (batched, versioned API)
          |
LOLA ACE host (Rust, in-process)
  ├── Session & event ledger
  ├── Scheduler / cancellation / budgets
  ├── Policy & permission enforcement
  ├── Paradigm selector
  ├── Decision pipeline (independent lifecycle)
  ├── Streaming pipeline (independent lifecycle)
  ├── Tool registry (proposals, validation, approval)
  ├── Model registry / residency manager
  └── Backend-neutral inference contract
          |
Inference backends
  ├── Reference backend(s) for correctness and baseline
  └── LOLA Native Engine
        ├── LFM3 model frontend / graph lowering
        ├── CPU ARM64 kernels (first target)
        ├── GPU backend (only after device/operator validation)
        └── vendor accelerator adapter (optional, later)
```

The runtime lives in the app process. Do not rely on child inference processes or a local daemon on Android. Keep Kotlin↔Rust calls coarse-grained; token streaming should use bounded batches/events rather than one FFI call per scalar token if that creates measurable overhead.

## 3. Decision and Streaming pipelines

These are peers with separate state machines.

### Decision pipeline

Purpose: produce bounded, typed assessments and choose the next execution action. It may use deterministic rules, a decision model, retrieval or a small number of bounded model calls.

Responsibilities:
- Read a versioned state snapshot.
- Select an execution paradigm using host policy and available capabilities.
- Evaluate evidence and produce typed proposals.
- Apply transition, call, token, tool, memory and wall-clock budgets.
- Emit a decision event with provenance and confidence semantics.
- Never authorize privileged side effects.

### Streaming pipeline

Purpose: generate user-facing text or other supported model output.

Responsibilities:
- Own prompt construction and generation state.
- Stream partial output to the UI.
- Support stop/cancel, backpressure and bounded output.
- Track TTFT, prefill, decode throughput and inter-token latency.
- Own the model's mutable inference state for that generation/session.

A Decision run must not hold the Streaming pipeline's lock or block token delivery. They can communicate through typed events and immutable snapshots. Shared state must have explicit ownership and concurrency semantics.

## 4. Paradigm selection

The runtime supports multiple execution strategies rather than treating ReAct as the universal loop:

| Paradigm | Suitable use |
|---|---|
| Direct / Reflex | Single-step response, low latency |
| FSM | Explicit states, predictable transitions |
| DAG | Independent tasks and bounded parallel work |
| HTN | Hierarchical decomposition with known task methods |
| GOAP | Goal-driven action selection under preconditions/costs |
| Bounded ReAct | Observe → reason → propose tool → observe, with strict caps |
| Search / Deliberation | Candidate generation and evaluation within a fixed budget |
| Reflection / Recovery | Validate, repair or retry after a detected failure |

Selection is performed by a deterministic host policy using task class, device capability, latency target, available model capabilities and budgets. Model-generated classifications may be advisory only.

## 5. Budgets and safety

Every execution has host-enforced limits:
- wall-clock deadline;
- state transitions;
- model calls;
- input and output tokens;
- tool calls and parallel branches;
- memory and model-residency budget;
- retry count;
- cancellation deadline.

The model cannot increase these limits. Tool calls emitted by a model are proposals. Validate tool ID, JSON schema, arguments, permissions, idempotency and policy before execution. Writes or sensitive actions require explicit confirmation according to the tool's safety tier.

## 6. State and persistence

Initial persistence uses SQLite WAL on local app storage. Store:
- conversation/session metadata;
- append-only event/message records;
- model selection and model artefact metadata;
- execution/run metadata;
- diagnostic summaries;
- versioned policy/config references.

Graph-like relations can initially use relational tables and recursive CTEs. Do not introduce a graph database or vector database without measured evidence. Keep large model artefacts and media outside the hot relational tables; store verified file references and metadata.

## 7. Model residency

Do not load all model slots by default. The residency manager must:
- estimate weights + context/KV + convolution state + scratch + runtime overhead;
- reserve memory for Android and the UI;
- load and unload at safe boundaries;
- support cancellation and checkpoint/reload where possible;
- avoid mid-turn model swaps;
- report measured memory rather than relying only on model-file size.

The advertised model context length is not the same as a feasible phone context. Initial context defaults should be conservative and configurable per device profile.

## 8. Platform implementation

- Kotlin/Compose owns UI, Android lifecycle, downloads, permissions, notifications and platform APIs.
- Rust owns scheduler, session/event state, policy, budgets, residency and orchestration.
- C++ is optional for specific kernels or platform libraries where benchmarks justify its maintenance and FFI cost. It is not a second orchestration layer.
- Use a narrow, versioned FFI boundary with explicit ownership, errors and cancellation. UniFFI is a candidate for Kotlin↔Rust; validate generated bindings and Android ABI packaging in the POC.
- Avoid unnecessary cross-language calls in token hot paths.

## 9. Performance policy

No backend is declared superior by file format alone. Compare on the same device, checkpoint, quantization class, prompt, context, generation length and thermal conditions.

Measure TTFT p50/p95, prefill tokens/s, decode tokens/s, inter-token latency, peak RSS/PSS, model load time, energy/temperature where measurable, cancellation latency, session interference and output correctness.

CPU ARM64 is the first native target. GPU and NPU paths are optional optimizations gated by exact operator coverage, stable drivers and measured end-to-end gains.
