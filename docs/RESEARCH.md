# LOLA Research Ledger — filtered for the LFM vertical

ADR-037. Policy: external research is triaged here before adoption.
**The vertical stays LFM**: d1(-Omni) owns the decision slot, LFM3(-Omni)
owns the agent slot, our own engine runs both. Techniques are adoptable;
models and runtimes from other stacks are not.

Source: owner-supplied dual-model agentic-inference research digest
(Nov 2026), triaged 2026-11-15.

## 1. Adopted (techniques, with our twist)

| Technique | Source | How it lands in LOLA |
|---|---|---|
| **Schema-skeleton parallel argument decoding** (3.9× tool-calls, 2.1× energy, tested on-device) | SchemaWeaver, arXiv 2026-10-12 | **Superseded for slot A**: d1 emits zero output tokens — there is no sequential decoding to optimize. **Adopted later for slot B**: LFM3-Omni's XML tool calls (`<function=name>` scaffold fixed, `<parameter=x>` values independent) map directly onto the skeleton technique — roadmap Later, after K6. Their GPU/NPU-slower-than-CPU finding reinforces tier-1 CPU-first (ADR-018). |
| **Argument accuracy is the bottleneck** (tool selection 93–100% vs argument filling 63–84%, 0.6B–9B study, 30 MCP tools) | arXiv 2026-11-04 | Shapes measurement, not architecture: d1's typed answers (noul/choice/score) avoid free-form argument generation on the decision path entirely; the tool broker already validates agent arguments (ADR-033). **Argument accuracy becomes an explicit S5+/S9 metric.** Also validated: orchestrator-worker teams underperform one controller — our shape. |
| **Non-LLM components dominate agent latency** (5/10 apps; task-aware serving −29–40%; state offload 4.6× memory; tool-result caching −35% redundant calls) | AgentSysBench, arXiv 2026-08 | Confirms our thesis: the harness/ledger/retrieval *is* the inference pipeline. Adopt: tool-result caching added as an explicit extension of the decision cache + case bank (MEMORY.md consideration). Our S1 numbers (169 µs full tick) already show the non-LLM path is not the bottleneck. |
| **Agentic trajectory benchmarks** (multi-turn, context growth, KV reuse, end-to-end task progression) | MLCommons Agentic Inference, 2026-07 | Adopted as the measurement policy for S2m/S9 (BENCHMARKS.md): time-to-first-action, task completion, argument accuracy, repeated trials (30–50×). Raw tokens/s explicitly not the target. |
| **AppFunctions** (apps expose callable functions; Android 16, experimental) | Android Developers, 2026 | A fourth tool-interface class in TOOLS.md (native APIs · AppFunctions · MCP · accessibility), future, read-only first. |
| **Speculative tool-execution overlap** (16–37% of wall-clock is tool waiting; name prediction reliable, arguments not) | SPORK, 2026 | Later; strictly gated to read-only tools (consistent with ADR-033 tiers). |

## 2. Validated (we already built it — third-party confirmation)

- One deterministic controller + two specialized models; **not** two
  negotiating agents (the Nov-4 study's orchestrator-worker result).
- Decision model owns decisions; streaming model never executes tools
  (ADR-012/036); tools never execute from unvalidated output (ADR-033).
- Sequential inference with both models resident (ADR-007/017).
- Controller-side progress events instead of concurrent second-model
  inference for UI responsiveness.
- Dual-model ≠ automatically better: "do not assume two loaded models
  outperform one" — exactly why A0–A3 exists (S7 ladder).

## 3. Excluded (keeps the vertical LFM — ADR-037)

| Excluded | Reason |
|---|---|
| FunctionGemma 270M (as a model) | The *idea* — a small fine-tuned function specialist — is what d1 already is, natively typed, on the LFM3 architecture. Adopting the model would fork the vertical. |
| Qwen3/3.5, Gemma 4 E2B (as models) | Non-LFM. Our slots are d1-Omni + LFM3-Omni (ADR-017). |
| LiteRT-LM + ADK Kotlin (as the runtime) | Contradicts ADR-003 rev. (our own engine) and ADR-027 (Rust core + UniFFI + CMP). LiteRT remains a Tier-2 *plugin* target at most (ADR-018). |
| vLLM / SGLang serving | Server-side; LOLA is edge-first, single-process. |
| The paper's open question: "routing head inside a shared runtime?" | Already answered by our architecture: d1 *is* a structured head; routing stays a deterministic harness table (ADR-036) so the model can never game its own limits. |
