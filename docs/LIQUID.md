# Liquid AI — deep dive & the LOLA angle

Research 2026-11-15 (primary sources: liquid.ai blog/docs, HF org, arXiv,
Liquid4All GitHub, partner announcements). Companion: RESEARCH.md (ADR-037).

## 1. The arc in one page

- **2024–H1 25** LFM1 → LFM-7B open weights. MIT spinout, $250M A-round
  (AMD Ventures), ~$2–2.4B valuation.
- **Jul 25 — LFM2**: hybrid thesis lands (10 conv k=3 + 6 GQA);
  **hardware-in-the-loop NAS ("STAR")** — scored against measured
  Snapdragon CPU latency/RAM, not proxy metrics. LEAP SDK era begins.
- **2026 H1 — LFM2.5**: agentic turn (RL-heavy post-training); 230M→24B;
  Mercedes-Benz partnership (MBUX, 2027 E-Class); **Antidoom/FTPO**
  open-sourced (agent doom-loops 10.2%→1.4%); QAD (Q4_0 distillation,
  ~97% of BF16); **DSpark** speculative drafting, upstreamed into
  llama.cpp + SGLang; **Pipette** on-device benchmarking (open, with
  Artificial Analysis) — *explicitly no-NPU, Android CPU-only*.
- **2026 H2**: **Liquid Context + Liquid Agent with Qualcomm**
  (Snapdragon Summit) — on-device personal context on Hexagon — **OEM-gated,
  no public SDK**. **d1** decision models (two launches in 10 days; d1-ultra
  API input-only pricing; Decision Index). **RIPL**: sub-50 ms enterprise
  inference — **NVIDIA-only, contact-sales**. **LFM3** (our target):
  tri-hybrid, six ~2B models, designed across 14 device/backend combos
  incl. Hexagon. **LiquidCodec**. **LEAP deprecated** → "use llama.cpp".
  No LFM3.5 as of today.

## 2. Strategy map: own / open / recommend / ignore

| Own (closed) | Own (open) | Recommend (3rd-party) | Ignored/deprecated |
|---|---|---|---|
| RIPL (NVIDIA enterprise serving) · Liquid API/console · Liquid Context + Agent (OEM-gated) | weights (GGUF/MLX/ONNX/NVFP4/FP8) · cookbook + `liquid-finetune` (MIT) · Pipette suite · antidoom · DSpark drafters (upstreamed via PRs) · LiquidCodec | llama.cpp (canonical on-device, incl. mobile C-API docs) · MLX · ONNX · vLLM/SGLang | **LEAP SDK** ("was a llama.cpp wrapper") · an official Android runtime · retail NPU (delegated to Nexa AI) |

Their revealed preference: **upstream kernels into llama.cpp/SGLang/MLX
and partner (MacPaw, Nexa, Qualcomm, Mercedes) rather than maintain
SDKs** [informed speculation from the pattern].

## 3. Fundamentals worth internalizing

- The thesis: on-device latency/memory on real hardware — not FLOPs — is
  the binding constraint; NAS scored against measured device latency
  (STAR). LOLA mirrors this: our benchmark-first rule is the same
  philosophy at kernel level.
- LFM3 published numbers we use: KV @128K = **1.6 GB** (vs 4.3 GB
  Qwen3.5-4B); ~1 GB total @4-bit; 1.8–2.8× faster decode @32K on
  llama.cpp Q4_0 across three devices.
- d1: calibrated probabilities in one pass; 475 packed states/s on RTX
  4090; **calibration is their asserted moat vs "Jev 1.14" — and no
  public tooling exists to calibrate d1 on a customer's own labels.**
- Fine-tuning: cookbook TRL recipes incl. **GRPO on a 108-tool
  phone-assistant dataset** (`LiquidAI/phone-assistant-tools`) — exactly
  our tool-calling vertical, trainable on one GPU.
- Card caveat: LFM3-1.7B "not recommended for agentic coding or complex
  multi-step reasoning" — our agent loop must compensate with routing,
  skills, and escalation discipline (it does).

## 4. Gap analysis — what nobody provides for LFM on-device

1. **Multi-model orchestration**: nothing (theirs or third-party) runs
   **d1 + an agent LFM in one local process** with a gating loop. Their
   d1 demos (Smart Filter, action picker, context compaction) are one-off
   blog demos. **LOLA sits exactly in this hole.**
2. **NPU-first public Android runtime**: Hexagon LFMs exist only inside
   the OEM Qualcomm deal; Pipette is CPU-only; community llama.cpp
   Vulkan/OpenCL on Snapdragon is painful.
3. **DSpark drafter for LFM3**: exists for LFM2.5 family only.
4. **Developer memory/context substrate**: Liquid Context is OEM-only;
   MacPaw built their own (Mnemos).
5. **d1 domain-calibration harnesses**: index + dataset public, tooling absent.
6. **Chat-template/tool-call reliability** on llama.cpp (`<think>`
   leakage, broken templates) — recurring community complaints.

## 5. What Liquid would value (ranked, with evidence)

1. **The Android-first dual-model runtime** (d1 gate + LFM3 agent, one
   process). They exited SDKs; the vacuum is real; they partner/buy
   rather than build SDKs (LEAP deprecation, MacPaw/Nexa/Qualcomm
   pattern). Risk: it's adjacent to what Liquid Agent is internally.
2. **d1-as-gate tooling + calibration harnesses** (our ROUTER + S5).
   Cheapest, zero overlap with anything they build, feeds their asserted
   calibration moat. **Best effort-to-value for us.**
3. **Tri-hybrid kernels as mobile-GPU (Vulkan/Adreno) contributions** —
   llama.cpp is weak there and RIPL doesn't reach mobile; position as
   upstream contributions/RIPL-edge feedstock, not a standalone product.
4. **Decision-ledger/memory substrate**: likely to *compete* with Liquid
   Context rather than be bought — keep it as our differentiation.

## 6. Risks & mitigations

| Risk | Mitigation |
|---|---|
| They ship a public mobile runtime (LEAP successor / Liquid Agent SDK) | Move fast on the dual-model orchestration niche; be the best *consumer* of their models, easy to recommend |
| Qualcomm/Nexa deliver official LFM3 NPU path | Our K8 plugin ABI targets *our* graph — we ride their runtime as a backend if it wins |
| llama.cpp upstream swallows the kernels | Benchmark on **Pipette** (their own surface); our advantage must show there |
| LFM4 re-mixes the architecture (k=3→7 in 15 months!) | Kernels stay **generic operators** (conv-k, SWA-w, partial-RoPE-GQA) parameterized by config, not frozen to LFM3 |
| LFM Open License >$10M-revenue clause | Enterprise customers may need Liquid paperwork; "the runtime Liquid recommends" partially depends on terms we don't control — track it |
| 2B-class not recommended for complex reasoning | Router + escalation + skills discipline is exactly our answer; watch the 24B-A2B line |

## 7. Actions adopted into the roadmap

- **Benchmark on Pipette surfaces** (their open apps define the numbers
  Liquid reads) — added to S2m.
- **d1 calibration harness** = S5 elevated: build the domain-calibration
  tooling nobody (including Liquid) ships — candidate open-source release
  ("the thing Liquid would buy" #2).
- **LFM3 drafter**: F7 (prompt-lookup first) then evaluate distilling a
  DSpark-style drafter for LFM3-1.7B with their MIT `liquid-finetune` +
  GRPO phone-assistant recipe — a contribution upstream could adopt.
- **Kernels as generic operators** (config-driven) — LFM4 insurance.
