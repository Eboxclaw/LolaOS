# LOLA Native Inference — engineering track

**Status:** committed roadmap track (ADR-044). **Scope:** LOLA ACE inference
architecture, LFM3 optimisation, Android. **Target device:** Google Pixel 8
Pro (Tensor G3). **Initial model:** Liquid AI LFM3-1.7B.

This document establishes LOLA Native Inference — our own execution engine —
as a first-class engineering track and a fixed architectural direction. It is
a planning commitment, not a claim of delivered performance: the long-term
objective is direct control over model execution, memory, hardware
scheduling and streaming, with LFM3 as the first deeply optimised family and
independent runtimes as measurable reference points.

No existing backend is removed or rewritten by this document. Implementation
follows the phases below through the normal review process.

## 1. Strategic decision

LOLA develops its own native inference engine. The objective is **not** to
wrap, fork, or permanently depend on llama.cpp or LiteRT-LM; both serve as
independent reference implementations and performance benchmarks against
which LOLA Native Inference is evaluated. The engine specialises first in
Liquid AI LFM3 models and Android hardware rather than aiming to be a
generic transformer runtime.

## 2. Three-engine strategy

| Engine | Role | Strategic position |
|---|---|---|
| **LOLA Native Inference** | our own engine | primary development target |
| **llama.cpp / GGML** | mature reference | benchmark + research baseline; the currently shipped app backend |
| **LiteRT-LM** | Android-oriented compiled runtime | benchmark and accelerator reference only |

- The existing llama.cpp implementation remains functional for the whole
  development period (it is the app's working backend and the recorded
  emulator baseline's subject).
- LiteRT-LM is investigated and integrated **as a benchmark backend only
  where useful** — never a mandatory dependency.
- Working implementations are never replaced prematurely; replacement is
  gated by the acceptance criteria in §8.

## 3. Technical direction

Design around LFM3's actual computational architecture — tri-hybrid
(gated short convolution, sliding-window attention, global attention) —
not around a generic transformer runtime first. Research, design and
progressively implement:

- **Execution graph** — model-aware execution planning and operator scheduling.
- **Short convolution** — dedicated kernels, efficient state handling.
- **Attention** — optimised execution and KV-cache management.
- **Quantisation** — native quantised ops, no unnecessary dequantisation
  (must climb the ADR-040 ladder and pass its SB gates).
- **Memory** — explicit allocation, reuse, bounded working sets,
  memory-pressure handling (Android 17 anon-RSS budget per MOBILE_FIRST).
- **ARM64 CPU** — NEON optimisations, hardware feature detection, runtime
  kernel dispatch.
- **GPU/NPU** — optional paths only on verified operator coverage and
  measured wins; no assumption that GPU/NPU beats a well-optimised CPU path.
- **Streaming** — low-latency token delivery, bounded buffering,
  backpressure, cancellation.
- **Sessions** — safe reuse of recurrent/convolutional and attention state.
- **Android integration** — lifecycle, memory ownership, thermal and
  battery awareness.

Every performance decision is supported by physical-device measurements.

## 4. Architecture boundaries

Preserve the ACE separation (ARCHITECTURE.md):

- **Kotlin** — app, UI, lifecycle, platform integration.
- **Rust** — ACE orchestration, execution policies, session coordination,
  budgets, backend abstraction.
- **C++** — native inference kernels and low-level execution components
  *where appropriate* (note: the engine lab today is Rust-first;
  C++ is admitted only where a measured need exists — reconciliation in
  ADR-044).

The engine integrates through the existing ACE inference contracts. The UI
and Decision pipeline never depend on a concrete backend. The
backend-independent interface covers model loading, sessions, generation,
streaming, cancellation, capabilities and metrics. LFM3-specific
optimisations live inside the native backend and its specialised components,
never in the general ACE contract.

## 5. Model strategy

- Initial target: **LFM3-1.7B** (pinned revision, SHA-256 — models/catalog.v1.json).
- Future research: other LFM3 variants; Audio/Vision variants where
  supported; **LFM3-Omni-2B** as a candidate, subject to verification of the
  exact architecture, checkpoint and modalities.
- Support for 1.7B never implies compatibility with other variants; model
  capability declarations stay explicit and validated (ADR-037 lineage).

## 6. Benchmark programme

Reproducible physical-device benchmark comparing all available engines on
the Pixel 8 Pro. Identical across engines: model revision/weights,
quantisation format and level, prompt templates and inputs, context lengths,
sampling parameters, generation limits, device conditions and thermal
starting state.

Measure: model-load time · TTFT · prefill tok/s · decode tok/s · peak
PSS · state/cache memory · thermal degradation · battery · cancellation
latency · session continuation correctness · streaming correctness. CPU and
GPU measured separately where supported; NPU only where the exact operators
and model path are supported.

**Emulator measurements remain classified as plumbing and functional
validation, never physical-device performance evidence** (the standing rule
of BENCHMARK_RECORD_EMULATOR_V1.md).

## 7. Development phases

| Phase | Content |
|---|---|
| **N0 — Architecture & research** | audit the existing implementations and contracts; map LFM3's operations; identify the minimum native execution path; study llama.cpp and LiteRT-LM for relevant optimisations; establish correctness requirements and benchmark methodology; document without disrupting current functionality |
| **N1 — Native engine foundation** | native runtime and backend boundary; model loading and metadata validation; tensor and memory management; operator interfaces and scheduling; minimal LFM3 inference path |
| **N2 — LFM3 performance engineering** | short-convolution and attention optimisation; quantised kernels; recurrent and KV state management; ARM64 kernel dispatch; profile and eliminate memory/scheduling bottlenecks |
| **N3 — ACE integration** | connect through the existing inference contract; sessions, streaming, cancellation; validate Decision/Streaming separation; lifecycle, concurrency, recovery |
| **N4 — Comparative optimisation** | identical workloads across LOLA Native / llama.cpp / LiteRT-LM; profiling-driven optimisation; reproducible physical-device records; publish benchmark records |

Dependencies and sequencing: N0 feeds the engine lab's existing assets
(own GGUF reader, K0–K4 kernels, S2 oracle harness, SB quality gates — see
ENGINE.md/BENCHMARKS.md); N3 depends on N1–N2 plus the ACE contract
stability; N4 depends on all three engines running the §6 protocol. The
Phase-A audit must reconcile with the lab's slice/fruit spine rather than
duplicating it.

## 8. Acceptance criteria

LOLA Native Inference does **not** replace the reference backend merely by
executing a model. Replacement requires all of:

1. Correct LFM3 model execution.
2. Validated output and streaming behaviour (golden set + SB gates).
3. Correct recurrent and attention state handling.
4. Reliable cancellation and session lifecycle.
5. Stable operation under Android memory pressure.
6. Reproducible physical-device benchmarks.
7. Documented performance comparisons against both reference engines.
8. No regressions in ACE contracts or application behaviour.

Performance superiority is a target, not an assumption; any claim names the
tested model, quantisation, hardware, workload and metrics.

## 9. Explicit constraints

- No llama.cpp fork as a substitute for building our engine.
- LiteRT-LM never a mandatory runtime dependency.
- No rewrite of the working backend before the native engine is validated.
- No UI/application coupling to a backend; no ACE coupling to LFM3 tensor
  operations.
- No correctness, memory-safety or session-consistency sacrifice for
  benchmark gains.
- No unsupported GPU/NPU assumptions.
- Existing ADRs, contracts, implementation history and working
  functionality are preserved; conflicts are flagged, not silently
  overridden (see ADR-044 for the reconciliation list).

## 10. Relationship to existing work

- The **engine lab** (`crates/lola-*`) is the research bench this track
  builds from; nothing there is deleted or bypassed.
- The **reference backend** (llama.cpp `b11960`, pinned) and its emulator
  baseline remain the app's runtime until §8 is fully satisfied.
- The **hardware truth run** (ROADMAP A5) provides the reference engine's
  device numbers that §6 comparisons require.
- Cross-references: ADR-042 (ACE-first policy this track executes),
  ADR-040 (quantisation ladder), ADR-027 (stack), ENGINE.md, LIQUID.md,
  MOBILE_FIRST.md.
