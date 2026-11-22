# Dual-pipeline research digest — Decision readout & Streaming decode

*Research round 2026-11-22 (two web-research passes feeding the dual
pipeline slice, ADR-043 §6 / ARCHITECTURE §2). Every recommendation cites
its source. This is design input, not a performance claim.*

## A. Decision pipeline (d1 slot-A, prefill-only)

**What transfers from speculative/auxiliary decoding — and what doesn't.**
Medusa, EAGLE-2, prompt-lookup and lookahead are token-generation
accelerators (arXiv:2401.10774, 2406.16858, 2412.01447); their draft-verify
loops do not apply to a prefill-only scorer. What transfers: Medusa's proof
that extra heads read answers off the topmost-layer feature of the LAST
PREFILL position (d1's readout consumes the final hidden state after one
forward pass — never a decode loop), and prompt-lookup's insight that when
the answer is largely determined by prompt content (a fixed choice list),
the logits at the first answer position — available at end of prefill —
carry the decision.

Recommendations:

1. **Constrain the readout to the choice set, not full JSON.** Compile the
   schema (fixed key order + enum choices) into an FSM; mask logits over
   choice tokens at known positions. Grammar masking is sub-millisecond
   (XGrammar arXiv:2411.00901; Outlines arXiv:2501.10868; llama.cpp GBNF).
   Prefer enum masking over deep JSON grammars.
2. **Score by logprob, decide by argmax, temperature 0.** Verdict
   probability at a fixed answer position is a validated scoring mechanism
   (Generative Verifiers arXiv:2408.15240); surface-form choice scoring is
   standard but format-biased (Calibrate Before Use arXiv:2102.09690).
3. **Calibrate with content-free normalization, fit offline.** Divide each
   choice's probability by the model's prior on a content-free probe of the
   same schema (arXiv:2102.09690). No verbalized confidence — it needs
   generated tokens we don't have (arXiv:2305.14975, 2412.14737).
   Self-consistency contradicts the deterministic policy — offline
   validation only (arXiv:2203.07181).
4. **KV: persistent static prefix, transient suffix.** Cache the KV of the
   never-changing schema/system prefix (RadixAttention arXiv:2311.12349);
   drop suffix KV after each decision (ADR-024 holds). Chunked prefill and
   continuous batching protect concurrent decoders — irrelevant for
   single-user on-device decisions (Sarathi-Serve arXiv:2403.02310; Orca
   arXiv:2205.14814); the useful "batch" is scoring all choices inside one
   prefill.
5. **Specialize the model, freeze the schemas.** Task-tuned ~1B models beat
   general ones at structured decisions (TinyAgent arXiv:2409.00608;
   RouteLLM arXiv:2406.18665 — BERT-scale suffices for routing).
6. **Caution: forced format can degrade small models** (arXiv:2408.02442,
   2405.21047). Constrain only the choice-token position; A/B constrained
   vs free offline before locking a grammar.

## B. Streaming pipeline (LFM3 decode, N1–N2 input)

1. **Architecture (LFM line).** LFM1 framed generation as signal processing
   (Liquid blog); LFM2 is the published spec (arXiv:2511.23404):
   double-gated depthwise short conv, kernel k=3 (LFM2), GQA interleaved,
   hardware-in-the-loop layer mix. **LFM3-1.7B** (model card + HF config):
   24 layers = 13 gated short conv (kernel 7 → conv state = 6 past
   tokens/channel) + 5 SWA (window 256) + 6 global attention; RoPE θ=1e6
   partial (0.5) on global layers, θ=1e4 on SWA; 16Q/4KV heads, head-dim
   128, tied embeddings. Kernel priorities: W4A16 Q4_K_M dequant-GEMM with
   per-channel scales → gated conv k=7 streaming kernel → GQA decode
   attention with SWA ring + full KV → fused RMSNorm/QK-Norm/partial-RoPE.
2. **Memory/residency.** Decode is bandwidth/IOPS-bound: LLM in a flash
   (arXiv:2312.11514) windowed flash loading; Neuralink ASPLOS'25
   (arXiv:2410.19274) — layout co-activated neurons contiguously (1.49×);
   PowerInfer-2 (arXiv:2406.06282) phone scheduling; MNN-LLM
   (arXiv:2506.10443) DRAM/Flash hybrid + ISA-tuned rearrangement.
   1.7B Q4 ≈ 1 GB fits DRAM: mmap is for cold start + page-cache
   resilience, layer-major extents + prefetch layer N+1.
3. **State management.** Conv layers: fixed recurrent state (k−1 tokens) —
   exact. SWA: **StreamingLLM attention sinks (arXiv:2309.17453)** — a
   ring buffer collapses unless the first tokens are kept; keep sinks +
   256-token ring. Cross-turn reuse (PromptCache arXiv:2311.04934): persist
   the whole per-layer state blob per session; truncate only global-layer
   KV.
4. **Quantization.** Q4_K_M ≈ 4.9 bpw, ~+0.18 ppl (GGUF study
   arXiv:2601.14277); W4A16 favored for small models on mid-tier hardware
   (Arm blog); LFM3's QAD Q4_0 (quantization-aware distillation) recovers
   more; KleidiAI int4 (QSI4CX) micro-kernels ~20% faster on Cortex-A v9.
   Path: Q4_K_M + fp16 KV + fp32 logits first; q8_0 KV (our F2) and W8A8
   prefill later.
5. **Streaming UX/cancellation.** TTFT is the perceived-responsiveness
   metric (arXiv:2604.06183); no canonical cancellation paper — cooperative
   abort checked between decode steps; commit state atomically with each
   yielded token so cancel leaves consistent resumable state.

## C. How this shaped dual-pipeline v0 (ace-core `runtime.rs`)

- Decision port is **structurally prefill-only**: `DecisionReadout::score`
  returns scores over a fixed choice set; there is no way to express
  generation through it; reports carry `kv_dropped = true` by contract.
- `RoutePolicy` implements the deterministic pass-forward policy
  (ADR-036) with the PROVISIONAL thresholds (direct ≥0.70, escalate
  ≥0.60) and **content-free calibration** hook (§A3) — mismatched priors
  fail closed.
- Escalation builds a **new immutable snapshot** (never reuses the
  readout's view) per ADR-043 §6; events flow through a typed
  `PipelineEvent` ledger.
- All budgets stay host-owned and fail closed; the generator runs only on
  the escalate route, and budget exhaustion stops it before the first
  chunk.

## D. Execution path (dependency-ordered, traced 2026-11-22)

**Unblocked now, in actionable order:**

1. **Dual pipeline** (this slice, A10) → 2. **ace-inference adapters**:
   implement `DecisionReadout`/`StreamingEngine` over the llama.cpp
   backend; add `prefill()`/`diagnostics()` to the contract
   (INFERENCE_CONTRACT §2 names are illustrative) → 3. **A8** UniFFI
   bridge (no A5 dependency) — app served through `ace-inference` →
   4. **A6** incremental KV reuse + history compaction (emulator-gated,
   not device-gated) and **A7** WorkManager download guarantee
   (independent, parallelizable) → 5. **N0** native-inference audit
   (docs-only; consumes this research digest + engine-lab assets) →
   6. Engine-lab **S2** oracle bring-up (S0 ✅) → S3 kernels on its golden
   set; **S4/S6** open in parallel; **S2m P1** on any device.

**Blocked, by exactly what:**

- **A5** device truth run → physical Pixel 8 Pro (or any ARM64 phone).
- **N4** → A5 reference numbers (ADR-044 §6).
- **N1 → N0; N2 → N1; N3 → N1+N2 + contract stability; A9 folded into
  N3/N4** — the native chain cannot start ahead of its audit.
- Engine lab: S3 → S2 golden set; S5 → S2 (+S6 optional); S7 → S4+S5+S6;
  S8 → S6+S7; S9 → S2+S5; S2m P2–P5 → retail device; d1 slot-A quant →
  SB QAD gates (ADR-040); any distribution → LFM license review.

**Wire order for the dual pipeline specifically:** adapters (2) first on
the reference backend, so the same ports are later served by the native
engine at N3 without touching Decision/Streaming orchestration again.
