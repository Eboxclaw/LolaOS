# LOLA Harness — The OS Kernel

Companion to `docs/RFC-002.md`. Crates: `lola-harness` (kernel) on
`lola-memory` (storage, see `docs/MEMORY.md`).

LOLA is what you get when you take **Pi's** loop discipline, **llama.cpp's**
mechanics and **Hermes'** memory laws — and make it smaller, single-process,
and native to LFM3's decision + omni model pairs.

## 1. Stack

```
┌──────────────────── frontends (carved out: docs/UI_UX.md) ────────────────┐
│   lola CLI (Rust)         Compose MP app (later)          RPC/daemon      │
└───────────┬───────────────────────────────────────────────┬──────────────┘
┌───────────▼───────────────────────────────────────────────▼──────────────┐
│ lola-harness — kernel                                                     │
│   event bus · scheduler · session event-tree (Pi) · tools (≤10, no MCP)  │
│   decision loop (slot A: prefill-only) · agent loop (slot B: streaming)  │
│   escalation policy · risk guard (ring 0)                                │
├───────────────────────────────────────────────────────────────────────────┤
│ lola-memory — the crown jewel (docs/MEMORY.md)                           │
│   SQLite WAL ledger · session tree + compaction-as-archive               │
│   knowledge graph (validity-versioned) · curated memory L0/L1/L2         │
│   retrieval: cache → signature → bucket+kNN → graph → FTS5               │
│   content-addressed blobs + TTL                                           │
├───────────────────────────────────────────────────────────────────────────┤
│ lola-infer — nano-RIPL (docs/NANO_RIPL.md)                               │
│   slot A (decision) + slot B (agent) · modality router · residency       │
│   Infer trait ─ reference-llamacpp (oracle) ─ lola-custom (parity-gated) │
├───────────────────────────────────────────────────────────────────────────┤
│ compute tier-1: Rust CPU (AVX2/512·NEON) + Vulkan/wgpu                   │
│ tier-2 (later, dlopen C-ABI): Hexagon/QAIRT · LiteRT-Tensor              │
│ pinned to CPU: LiquidCodec + vision/audio towers (mmproj)                │
└───────────────────────────────────────────────────────────────────────────┘
```

## 2. Subsystem → inspiration map

| Subsystem | Inspired by | What we took | What we dropped |
|---|---|---|---|
| Agent/decision loop | **Pi** (`agentLoop`, one loop for all frontends, no step cap) | one loop core; steering/follow-up queues; abort-first design | multi-provider catalog, router models |
| Session durability | **Pi** (JSONL tree) + **Hermes** (state.db) | id/parent tree + entry taxonomy in SQLite; archive-don't-delete; durable markers | rotating session lineage; trigram shadows |
| Memory curation | **Hermes** | frozen session-start snapshots; add/patch/remove-only verbs; FTS5 recall; L0/L1/L2 tiering | cloud providers; per-turn micro-compaction |
| Compaction | **Hermes** + **Pi** | summary + first-kept + absorbed-ids witnesses; head/tail protection | in-place rewrite of sent history |
| Model residency | **llama.cpp** (router) + **Pi** (`model_change`) | two model+context pairs; LRU memory-margin; model swaps as ledger events | llama-server embedding; slot-similarity heuristics |
| KV management | **llama.cpp** | prefix reuse; `seq_rm`/`seq_add`/state-save twins of ledger ops | — |
| Tools | **Pi** | ≤10 native tools, schema'd, gated `tool_call` hook | MCP (context tax), 70-tool registries |

## 3. The three cache-stability laws

On-device, a rewritten prefix is real prefill compute. All three parent
systems converged on this independently; LOLA makes them invariants
(enforced in `session.rs`):

1. **No mid-session memory injection.** Curated memory is snapshotted at
   session start (`sessions.memory_snapshot_hash`) and frozen for the
   session's life. Changes land next session.
2. **No mid-turn model swap.** A slot swap ends the turn first
   (`ModelChange` entry).
3. **No history mutation.** Corrections are `ContextEdit` entries that
   reshape *future* context; original rows are never rewritten.

## 4. Session model (ADR-020/021/022)

Append-only event tree (`id` → `parent_id`); the leaf pins the active
branch; branching forks cost nothing.

| Entry | Purpose | KV twin |
|---|---|---|
| `message` | a turn; assistant entries record the serving model | prefix reuse |
| `model_change` | slot A/B swap (law 2) | slot swap under residency policy |
| `compaction` | summary + `first_kept` + `absorbed_ids` (ADR-022) | `seq_rm` evicted range |
| `context_edit` | append-only override for future context | — |
| `custom` | harness/extension state, never in model context | — |
| `media` | omni audio/image/video as `blob:<sha256>` refs | — |

## 5. The two loops

**Decision loop (slot A — d1 family).** d1 answers in **one forward pass,
zero generated tokens** ([d1 blog](https://www.liquid.ai/blog/d1-open)):
state + up to 16 packed named questions → typed answers + probabilities.
Prefill-only ⇒ **transient KV**: peak memory during the pass, evicted after
readout (ADR-024). Latency scales with state size, not output length.

**Agent loop (slot B — LFM3 family).** Streaming autoregressive decode,
ChatML + `<think>`, XML tool calls. Escalation-gated (below), never on the
candle-critical path. The d1 model doubles as the **summarizer** for slot
B's compaction (Hermes' aux-model pattern — a small non-reasoning model).

**Escalation policy** (`tools::EscalationCondition`, each measurable):
insufficient evidence · decision abstained · conflicts with history ·
research required · operator request · strategy hypothesis.

## 6. Tool policy (ADR-020)

≤10 native tools, JSON-schema'd, `prompt_snippet` progressive disclosure,
`tool_call` gate hook (block/mutate), idempotency keys on side-effecting
calls. No MCP — the context tax is precisely what Pi rejected and we're
smaller than Pi.

## 7. Mobile constraints (apply to every slice)

Single process, no fork assumptions · bounded memory with explicit budgets
(`memory.yaml` profiles) · GGUF weights, mmap-friendly · SQLite local-FS
only · everything builds for `aarch64-linux-android` · codec + encoder
towers pinned to CPU (LiquidCodec decodes ~87× realtime on one thread) ·
UI lives above the FFI line (Compose MP, `docs/UI_UX.md`).

## 8. Sources

[Pi](https://github.com/earendil-works/pi) · [Pi design essay](https://mariozechner.at/posts/2025-11-30-pi-coding-agent) ·
[Hermes Agent](https://github.com/NousResearch/hermes-agent) · [Hermes session storage](https://hermes-agent.nousresearch.com/docs/developer-guide/session-storage) ·
[Hermes memory](https://hermes-agent.nousresearch.com/docs/user-guide/features/memory) ·
[llama.h](https://github.com/ggml-org/llama.cpp/blob/master/include/llama.h) ·
[llama.cpp server (router/KV reference)](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)
