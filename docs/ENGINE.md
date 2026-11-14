# LOLA Engine — Our Own LFM3-Specialized Inference Implementation

Companion to `docs/NANO_RIPL.md`. Crate: `lola-infer` (`engine.rs` types
today; kernels land by milestone). ADR-003 (revised), ADR-026.

## 0. Doctrine

**The engine is the product.** We implement our own kernels for the LFM3
tri-hybrid, our own weight layout, and our own dual-slot scheduler.
llama.cpp serves exactly two external roles — **correctness oracle**
(golden outputs) and **GGUF import source** — and nothing of it ships
inside LOLA. Rationale: LFM3's shape (13/24 conv layers, SWA ring
buffers, partial RoPE, prefill-only decision models) is sufficiently
different from the Llama mainstream that a specialized engine is both
smaller and faster than a general one; Liquid AI's own deprecation of LEAP
in favor of llama.cpp defines the *interchange* convention, not the
*runtime ceiling* ([migration note](https://docs.liquid.ai/deployment/on-device/llama-cpp/migrating-from-leap-sdk)).

## 1. Kernel inventory (all ours)

Reference shape (verified against [LFM3 config](https://huggingface.co/LiquidAI/LFM3-1.7B) / [LFM3 blog](https://www.liquid.ai/blog/lfm3)):
24 layers = 13 gated short conv (k=7) + 5 SWA (window 256) + 6 global
attention; GQA 16Q/4KV × head_dim 128; partial RoPE on global layers
(50% of head_dim, θ=1e6), full RoPE on SWA (θ=1e4); head-wise sigmoid
gate; sandwich norm; 128K vocab; tied embeddings; MLP 7680.

| EngineOp | What we implement | Why specialized | Milestone |
|---|---|---|---|
| `GatedConv7` | **Depthwise** gated short-conv (per-channel k=7 value+gate), constant 7-slot state — 8.5 µs/token measured; dense variant kept as pointwise/parity utility | No KV at all; O(in_ch×7) per token — measured ~800× cheaper than the dense reading, which is exactly why LFM3 uses it for 13/24 layers | K1 ✅ |
| `SwaAttention` | Ring-buffer KV (fixed 256 slots), full-RoPE θ=1e4 | Bounded memory per head regardless of context; single flat buffer, no eviction logic | K2 |
| `GlobalAttention` | GQA 16/4, partial RoPE (rotate only the high-frequency half of each head) | Partial rotation changes the RoPE kernel's inner loop; generic kernels do wasted work | K3 |
| `HeadGate` | Per-head sigmoid gate fused into attention output | Near-free; fused avoids a whole tensor round-trip | K4 |
| `SandwichNorm` | Pre+post RMSNorm pair per block, fused with neighbors | Removes two global passes per block | K4 |
| `Q4Matmul` | Quantized matmul family, per-tensor mixes | Repacked layout lets tiles match our loop order (see §2) | K0+ |
| `TiedEmbedding` | 128K-vocab tied lookup + logits head | vocab is 2× LFM2's — embedding layout matters on 32K-cache devices | K0+ |
| `GatedMlp` | SwiGLU-style MLP, inter 7680 | Fused gate+up dequant | K0+ |
| `D1Readout` | Typed-answer readout (noul/choice/score + extraction/boxes/multiple) ported from `modeling_d1_lfm3_*.py` | Not implemented anywhere outside transformers today — including llama.cpp (only noul/choice/score via `/v1/systemone`) | K5 |

Platform order: NEON + AVX2/AVX-512 CPU first (works everywhere), then
wgpu/Vulkan (AMD laptops, Tensor Mali — **not** Adreno, which probes and
falls back to CPU per ADR-018). LiquidCodec + mmproj encoder towers stay
pinned to CPU (codec ≈ 87× realtime on one thread).

## 2. Weights pipeline

```
official GGUF (interchange, unmodified, checksummed)
        │  install-time repack (RepackPolicy::InstallTime — never in the
        │  decision path)
        ▼
lola-native layout vN (LOLA_WEIGHT_LAYOUT_VERSION)
  · per-kernel tile order (weights laid out in the exact order our loops
    read them — no gather at runtime)
  · per-tensor-class quant mix, measured not guessed
  · page-aligned sections so weights stay file-backed under mmap
    (Android 17's Memory Limiter meters anon RSS — file-backed doesn't
    count; ADR-026)
```

Quant-mix design space (decided by K0 benchmarks, not taste): attention
projections Q6k / conv Q8_0 / MLP Q4kM / embeddings Bf16 is the starting
hypothesis. Layout version bumps invalidate every repack; every import
records source sha256 + layout id in the ledger.

## 3. Dual-slot scheduler (K6)

| | Slot A — decision (d1 family) | Slot B — agent (LFM3 family) |
|---|---|---|
| Workload | prefill burst per tick (ms) | streaming decode, user bursts |
| KV | **transient**: peak during pass, evicted after readout (ADR-024) | persistent; SWA layers = ring buffers (256), only 6 layers carry long KV |
| Threads | pinned to 2–3 big cores, burst-priority | pinned to remaining big cores; yields to slot A bursts |
| Budget | counted in *peak* anon | counted in *steady* anon |

The scheduler owns: duty-cycle + thermal hooks (suspend before Android
suspends us — `lifecycle.rs`), anon accounting against the profile budget,
and slot-swap ordering under the cache-stability laws (a swap ends the
turn first).

## 4. Conformance: no kernel serves until its gate passes

1. **Per-kernel parity**: outputs vs the oracle on randomized shapes +
   edge cases (window boundaries, partial-RoPE split points, quant block
   edges). Tolerance per op; exact for norms/gates.
2. **E2E golden set**: the oracle's outputs on the decision-contract
   prompt set (recorded S2) — logits within tolerance, typed answers
   exact.
3. **Perf gate**: a kernel that passes parity but misses its SLO stays
   off the hot path (oracle fallback) until it doesn't.

## 5. Milestones K0–K9 (start immediately after S2's oracle)

`K0` weights container + GGUF import + repack → `K1` conv kernel → `K2`
SWA attention → `K3` global attention + partial RoPE → `K4` gate+norm
fusion → `K5` d1 readout port → `K6` dual-slot scheduler → `K7`
wgpu/Vulkan kernels → `K8` NPU plugin ABI **for our graph** (not ggml's;
Hexagon plugin targets our ops, gated on the P2/P3 de-risk prototypes in
`docs/MOBILE_FIRST.md`) → `K9` **LiquidCodec voice path**.

### K9 — LiquidCodec (ADR-035)

The ~74M streaming codec powering LFM3-Audio/Omni is itself built from
LFM3 blocks — encoder *and* decoder ([blog](https://www.liquid.ai/blog/liquidcodec)).
Consequences we exploit:

- **Our K1–K3 kernels cover the codec** — the voice path is entirely our
  engine: mic → causal encoder (streams; ~361× RT on one CPU thread) →
  backbone token space → **GFSQ parallel multi-token prediction** (finite
  scalar quantization over independent groups — speech-out is a few
  parallel refinement steps, not a long autoregressive stream) →
  direct spectral-prediction decoder (~87× RT one thread) → audio out.
- **Placement**: codec pins to a LITTLE core; GPU/NPU stay with the
  backbone. Decoder-only quantization-aware training makes the Q4_0
  decoder 3.5× smaller with minimal quality loss — that's our default
  (`config/models.yaml: codecs:`).
- **Import**: codec builds (Q4_0/Q4_K_M/Q8_0) ship inside the official
  GGUF releases → standard GGUF import + repack, checksummed.

## 6. Model notes — the checkpoints we run

Verified ✅ / verify-at-implementation ⚠️ (sources: Liquid docs + HF cards,
Nov 2026 research; re-verify against the cards when S2 pins revisions).

**d1-Omni-2B (slot A, decision)**
- ✅ Single forward pass, zero generated tokens; `system_one(state,
  questions, media)`; up to 16 questions per set (bigger = faster/word,
  slightly less accurate); answers are noul P(yes) / choice (+multiple) /
  score rubric 2–10 + extraction types (text/number/date spans, JSON
  pointers, boxes ≤16).
- ✅ Batch flags so state+media fit one batch: `-b 8192 -ub 8192`;
  image ≤ half of `-ub` (≤2048 tok/image); video 2 fps.
- ✅ Audio questions in, typed answers out — no transcript written.
- ⚠️ Extraction/boxes/multiple readout only in transformers today → K5
  ports from `modeling_d1_lfm3_omni.py`; llama.cpp covers noul/choice/
  score only.

**LFM3-Omni-2B (slot B, agent)** *(facts verified verbatim from the cards,
Nov 2026)*
- ✅ ChatML with `<|startoftext|>` BOS and `<|im_end|>` EOS; omni is
  **non-reasoning** (`enable_thinking=True` unsupported) and *every
  assistant turn opens with an empty `<think></think>`* — our prompt
  builder reproduces this exactly.
- ✅ **Tool calls (verbatim syntax)** — declared via the chat template
  (`<tools>…</tools>` in the system turn); the model emits:

  ```text
  <tool_call>
  <function=get_weather>
  <parameter=city>
  Paris
  </parameter>
  </function>
  </tool_call>
  ```

  Non-string values are written as JSON. Results go back as a `tool`
  message; keep the schemas attached on subsequent turns. The card's
  official parser regexes (`<tool_call>/<function=name>/<parameter=p>`)
  are the spec our parser implements. **Deferred tools**
  (`defer_loading=True` → search returns `tool_reference` → template
  injects `<additional_tools>`) — the model natively implements our
  Tool Search / `search_tools` design.
- ✅ Sampling (verbatim from cards): text output — greedy works well;
  speech/interleaved — `audio_temperature=0.4`,
  `audio_repetition_penalty=1.2`. ⚠️ `audio_top_k` is **not documented**
  for omni (the 128/rep-window values are Audio-2B's); the GGUF stores
  the card's audio params.
- ✅ Media markers: `<|vision_start|>/<|vision_end|>`, `<|audio_out|>`,
  `<|audio_start|>`, `<|mixed_start|>`; mmproj ships as **Q8_0**
  (`mmproj-LFM3-Omni-2B-Q8_0.gguf`); serve flags: `--reasoning off
  --temp 0`; ASR pattern `-sys "…" -p "<__media__>"`; realtime voice +
  tool calls on `/v1/realtime` (b11960+).
- ✅ 32K context ceiling; generation modes text/audio/interleaved;
  streaming causal audio encoder (~50 ms-class TTFT speech-in); video
  locked at 2 fps, 4–384 frames (`--video-fps` ignored).
- ⚠️ No DSpark speculative drafter for omni yet — watch Liquid releases.

## 7. Risks (owned here)

- d1 readout internals undocumented → port from modeling files; the K5
  gate is parity vs the d1 API on the same checkpoints.
- Quant-mix quality unknown until measured → K0 benchmarks decide; the
  oracle is always the correctness floor.
- Solo-kernel velocity: 10 milestones is real engineering; the oracle
  keeps the harness shippable while kernels land one by one (that is the
  entire point of the oracle discipline).
- Codec parity: K9's gate is ViSQOL/mel-distance vs the official codec
  builds on a fixed speech corpus.
