# LOLA nano-RIPL — Edge Inference Layer (v2)

Companion to `docs/HARNESS.md`. Crate: `lola-infer`. ADRs: 003, 007, 017,
018, 023, 024.

## 1. Purpose

A nano-scale answer to [RIPL](https://www.liquid.ai/blog/low-latency) for
edge devices, specialized for LFM3-family pairs (decision + agent, omni
capable). Techniques and economics from the inspirations — never forks:

| Inspiration | What we take |
|---|---|
| [RIPL](https://www.liquid.ai/blog/low-latency) | sub-50 ms ambition; ~1 GB per 2B model at 4-bit; inference inside the request path |
| llama.cpp | mmap weights (warm page-cache ≈ instant swap), GGUF block quants, conservative kernels |
| Hermes / Pi | the harness laws that make on-device caching actually pay |

## 2. Model pairs (ADR-017/019)

Profiles declare **pairs**; the planner verifies fit; the decision slot is
never downgraded (ADR-007).

| Profile | Slot A (decision) | Slot B (agent) | Notes |
|---|---|---|---|
| `laptop` | d1-Omni-2B · Q8_0 · 1.85 GB · ctx 128K | LFM3-Omni-2B · Q4_K_M · ~1.1 GB · ctx 32K | omni day one; mmproj sidecars; agent KV ~0.4 GB @32K |
| `constrained` | d1-2B · Q4_K_M · 1.1 GB | LFM3-1.7B · Q4_K_M · 1.05 GB · on-demand | text pair; omni when d1-Omni ships 4-bit |

**Why dual-model residency is viable**: LFM3's tri-hybrid (24 layers = 13
conv k=7 + 5 SWA-256 + 6 global attention) means only 6/24 layers carry
long KV — ~12 KB/token ⇒ ~0.4 GB @32K, ~1.5 GB @128K
([LFM3 blog](https://www.liquid.ai/blog/lfm3)). And the decision slot is
**prefill-only**: d1 answers in one forward pass with zero generated tokens
([d1 blog](https://www.liquid.ai/blog/d1-open)), so its KV is transient —
peak during the pass, evicted after readout (ADR-024).

## 3. Compute tiers (ADR-018)

**Tier 1 (ships in v0.1, universal):**
- Rust CPU kernels: quantized matmul (Q4_0/Q4_K/MXFP4), AVX2/AVX-512 +
  NEON. Reference points: 113–143 tok/s decode for 1.2B Q4 on current
  Ryzen; 8–25 tok/s on Android CPUs ([Liquid llama.cpp docs](https://docs.liquid.ai/deployment/on-device/llama-cpp),
  [ggml #23736](https://github.com/ggml-org/llama.cpp/discussions/23736)).
- Vulkan compute via wgpu: **production on AMD laptops** ([Phoronix:
  Vulkan beats ROCm on decode](https://www.phoronix.com/review/lemonade-vulkan-rocm)).
  *Correction (Nov 2026 research):* on **Adreno** Vulkan is driver-broken
  → probe → CPU fallback ([#16881](https://github.com/ggml-org/llama.cpp/issues/16881)),
  and on **Tensor phones Vulkan historically loses to CPU**
  ([#829](https://github.com/ggml-org/llama.cpp/issues/829)) — the K7
  Vulkan tier is an **AMD-laptop** tier; phones run our CPU kernels.

**Tier 2 (designed-for now, built later; dlopen C-ABI plugins):**

| Plugin | Device | Status (researched Nov 2026) |
|---|---|---|
| Hexagon/QAIRT (`ggml-hexagon`) | Snapdragon NPU | merged upstream, experimental: 51.5 tok/s (1B Q4) vs ~8 CPU; app-packaging unverified ([snapdragon README](https://github.com/ggml-org/llama.cpp/blob/master/docs/backend/snapdragon/README.md)); Liquid benchmarks it too ([Liquid4All](https://github.com/Liquid4All/benchmarks-llama.cpp)) |
| Adreno OpenCL | Snapdragon GPU | 21–31 tok/s (1.5B Q8); f16-KV-only caveats ([OPENCL.md](https://github.com/ggml-org/llama.cpp/blob/master/docs/backend/OPENCL.md)) |
| LiteRT-Tensor | Pixel 10+ TPU | Tensor SDK beta, custom models unverified at 2B scale ([Tensor SDK](https://developers.google.com/edge/litert/next/tensor-sdk)) |

Rejected: NNAPI (deprecated in Android 15), SYCL (AMD support removed),
ROCm (loses to Vulkan, no mobile story), embedding ExecuTorch/ONNX as core
deps (conversion projects without documented LFM3 paths).

## 4. Backends behind `Infer`

**The engine is the product (ADR-003 rev. — full design in
`docs/ENGINE.md`): our own kernels, our own repacked weight layout, our
own dual-slot scheduler.** llama.cpp is external to the shipped system:

1. **`reference-llamacpp` (S2) — the oracle, used at build/test time.**
   llama.cpp ≥ b11960 linked in-process during development. Serves
   noul/choice/score via the systemone-style path; its outputs on the
   decision-contract prompt set become the **golden tests**. Nothing of
   it ships inside LOLA.
2. **`lola-custom` (S3) — the engine we want.** Our own runtime for the
   tri-hybrid: gated short-conv k=7 streaming state, SWA ring buffer (256),
   global attention with partial RoPE (50% of head_dim, θ=1e6), head-wise
   sigmoid gate, sandwich norm, GQA 16/4 × 128. The **d1 readout** (typed
   answers incl. extraction/boxes/multiple — not yet in llama.cpp) is
   ported from `modeling_d1_lfm3_*.py`. Ships only after kernel-by-kernel
   golden parity.

**Omni hosting rules:** mmproj encoder towers + LiquidCodec **pinned to
CPU** (codec decodes ~87× realtime on one thread; keeps GPU/NPU for the
backbone); streaming causal audio encoder enables ~50 ms TTFT speech-in;
video slices at 2 fps.

## 5. SLOs (budgets, not promises; CI-enforced regressions)

| Metric | Budget | North star |
|---|---|---|
| d1 decision (warm, 1 question) | < 200 ms | 50 ms (RIPL-class) |
| d1 decision (5k-token state) | < 800 ms | — |
| Agent decode (Q4, laptop CPU) | ≥ 25 tok/s | — |
| Agent decode (Q4, Android CPU) | ≥ 8 tok/s | — |
| Speech TTFT (omni) | < 300 ms | 50 ms |
| Cold load (mmap warm) | < 1 s | — |
| Cold load (cold page cache) | < 5 s | — |
| Resident set, omni pair | ≤ ~3.5 GB | — |

## 6. Risk register

| Risk | Severity | Mitigation |
|---|---|---|
| d1-Omni has **no 4-bit** (Q8 floor 1.85 GB) | med | constrained profile uses text pair; d1-Omni Q4 quantization is a roadmap task (we own it) |
| Adreno Vulkan broken | med | probe → CPU fallback; Hexagon plugin later |
| d1 readout internals undocumented | med | port from modeling files; golden tests vs API |
| **LFM Open License v1.0 unread** (`license:other`) | high | legal review before any distribution |
| llama.cpp version floor churn (b11960+) | low | vendor-pin the oracle |
| Hexagon app-packaging unverified | med | device-lab spike before promising NPU |
| Tensor SDK beta / 2B-fit unverified | low | CPU+Vulkan is the shipped Tensor story |
