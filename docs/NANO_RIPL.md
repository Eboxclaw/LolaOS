# LOLA nano-RIPL — Edge Inference Layer

Companion to `docs/RFC-002.md` §4/§10. Crate: `crates/lola-infer`.

## 1. Purpose and inspiration (never a fork)

A nano-scale answer to Liquid AI's RIPL for **edge** devices. What we take
from each inspiration — techniques and economics, not code:

| Inspiration | What we take |
|---|---|
| [RIPL](https://www.liquid.ai/blog/low-latency) | Sub-50 ms ambition; ~1 GB per 2B model at 4-bit; inference belongs *inside* the request path |
| llama.cpp | mmap-backed weights (warm page-cache == near-instant swap), block quantization, conservative kernels |
| Hermes | Durable, searchable, recoverable memory patterns |
| Pi harness | Lean agent loop discipline: the harness is the product, models are components |

The goal: a **super-light dual-model harness** — decision model + agent
model living together in one process, swapping at page-cache speed, with a
strict output contract the kernel can trust.

## 2. Device profiles (ADR-007)

| Profile | Budget | d1-2B | LFM3-1.7B |
|---|---|---|---|
| `laptop` | 8 GB | resident | resident |
| `constrained` | 3 GB | resident | on-demand |
| Android (later) | ~3–4 GB app budget | resident | on-demand |

Planning rule (`residency.rs`): decision models are **never** downgraded;
agent/specialist slots drop to on-demand when the budget minus active-model
footprint leaves < 1 GB kernel headroom. A plan without a resident decision
model refuses to boot.

## 3. Residency state machine

```
UNLOADED → LOADING → WARM ⇄ BUSY → EVICTION_PENDING → UNLOADED
                └──────────────► FAILED (recorded error + retry policy)
```

Never evict a model with an active request · never thrash under load
(hysteresis on swap decisions) · track weight memory separately from KV /
activation memory · record checkpoint + weight hashes with every load.

## 4. The `Infer` trait and the two backends

The kernel sees only `Infer` (decide / generate). Two implementations:

1. **`reference-llamacpp` (S2) — the oracle.** llama.cpp linked in-process
   as a library. Not a sidecar: one process, one language boundary, still
   mobile-clean. Its outputs on the decision-contract prompt set become the
   **golden tests**.
2. **`lola-custom` (S3) — the engine we actually want.** Our own inference
   optimized for LFM3-family constructions (hybrid conv + attention, GQA).
   GGUF-compatible weight container, 4-bit CPU kernels first, Metal later.
   It ships **only** after kernel-by-kernel parity with the golden tests.

S3 is the highest-risk slice in the roadmap (ADR-003 accepts this with open
eyes). Two protections: the harness never blocks on it — everything runs on
the oracle until parity — and every backend passes the same conformance
suite or is not a backend.

## 5. SLOs

| Metric | Budget | North star |
|---|---|---|
| d1 warm structured decision | < 200 ms | 50 ms |
| LFM3 warm generation start | < 500 ms | — |
| Cold load | < 5 s | — |
| Warm swap (page-cache reload) | < 1 s | — |
| Resident set, both models @4-bit | ≤ ~2.6 GB | — |

All measured by the S2 benchmark harness from day one; regressions against
budget are CI failures, not footnotes.

## 6. Conformance gates

A backend may serve the kernel only if: required named questions are all
answered · output types match the contract enums · probabilities are valid
and sum sensibly · unknown/malformed output fails closed to `ABSTAIN` ·
golden-set outputs match the oracle within tolerance. Quantized variants
(Q4_K_M first) are admitted only after passing the same gates.
