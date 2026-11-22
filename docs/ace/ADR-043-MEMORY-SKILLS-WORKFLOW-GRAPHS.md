# ADR-043 — ACE memory, skills and workflow graphs

- **Status:** Proposed
- **Date:** 2026-11-22
- **Scope:** LOLA ACE runtime; Android-first, local-first
- **Supersedes:** Nothing. Refines the ACE direction in ADR-042 and the existing skills/memory research in ADR-028/030/041 without importing the legacy trading vertical.
- **Research inputs:** Google TurboQuant (Zandieh et al., ICLR 2026); FlashMem (Findings of ACL 2026); FlashRAG; Agent Skills open standard; current ACE and engine-lab source.

## Context

LOLA ACE already has independent Decision and Streaming run lifecycles in `ace-core`, a backend-neutral inference contract in `ace-inference`, and an Android LFM3 reference path. The current Android app is emulator-validated end-to-end, but physical Pixel performance remains unproven. The Kotlin↔Rust bridge and LOLA Native Engine are not yet integrated.

The repository also contains substantial prior art in `crates/lola-memory` (SQLite ledger, FTS, graph, numeric kNN, compaction and retrieval) and `crates/lola-harness/src/skills.rs` (Agent Skills manifest parsing and progressive disclosure). These are in the legacy engine lab, whose trading-specific types and policy must not become ACE runtime dependencies. The ACE architecture currently describes a tool registry and a possible graph-like relational store, but it does not yet define a portable memory lifecycle, ACE skill runtime, or workflow-graph contract.

The project needs reusable capabilities that can be composed into deterministic, inspectable execution plans. It must support the dual pipelines without coupling memory, skills, workflows or tool policy to one model or one agent loop.

## Decision

Adopt a **three-part foundational layer** for ACE:

1. **Memory substrate** — model-independent, provenance-aware local memory with explicit write, retrieval, consolidation, supersession and forgetting lifecycle.
2. **Skills** — adopt the Agent Skills `SKILL.md` open standard as-is, using progressive disclosure. Skills describe reusable capabilities and constraints; they do not themselves become executable workflows.
3. **Workflow Graphs** — versioned, typed, declarative execution graphs that compose skills, retrieval, model calls, tools, transforms, gates and outputs. The Decision pipeline plans, validates and schedules graph execution; the Streaming pipeline executes model-generation nodes and streams output.

The three parts communicate through versioned, typed contracts and immutable references. They are capabilities of ACE, not a new model format and not a mandatory agent loop.

### 1. Memory substrate

Memory is a lifecycle, not a vector database and not a longer prompt. Use a hybrid local substrate with SQLite as the initial source of truth:

| Layer | Canonical role | Initial implementation |
|---|---|---|
| Event / episodic | What happened, when, in which run/session, with which outcome | Append-only event records; content-addressed payload references |
| Semantic | Validated facts, entities, preferences and relationships | Typed relational records + provenance |
| Procedural | Reusable skills and workflow versions | Versioned skill/workflow artifacts; evidence links |
| Working | Current request, run state and retrieved evidence | Bounded in-memory context snapshot |
| Evidence / document | Original source passages, files and citations | Private blob store + metadata + FTS5 |

Memory records must carry at least: stable ID, kind, content or payload reference, source/provenance, created/observed time, confidence where meaningful, validity interval, sensitivity/access scope, schema version and supersession/deletion state. Distinguish observed facts from model inferences and user assertions. Conflicts create explicit supersession or competing claims; do not silently overwrite provenance.

**Write path:** capture event → extract candidate records → validate schema and source → deduplicate / detect conflict → apply consent and retention policy → commit canonical record → update FTS/vector/graph indexes asynchronously → optionally consolidate during an idle window. Model-generated memories are proposals, not trusted facts. Do not write every token or every transient thought to long-term memory.

**Read path:** classify retrieval intent → exact/session lookup → lexical retrieval (FTS5/BM25) → metadata/time/access filters → optional vector candidate retrieval → bounded graph expansion → rerank → evidence pack with source references → context-budget selection. Retrieval results must preserve provenance and uncertainty. The system should return “no sufficient evidence” rather than invent a memory.

Use hybrid retrieval, but do not force every query through every retriever. Decision chooses retrieval paths based on query type, expected value, latency and resource budget. Start with lexical + structured/graph retrieval; enable vector retrieval only after embedding and recall benchmarks justify it. Keep raw source text retrievable and auditable.

**Memory tiers:** (a) working context, ephemeral and bounded; (b) session/episodic memory, persistent and append-only; (c) semantic/relational memory, validated and versioned; (d) procedural memory, skill/workflow artifacts. Background consolidation may distil episodes into semantic facts or procedural candidates, but must retain links to the source episodes and require validation before promotion.

Forgetting is explicit: support tombstones/deletion requests, index invalidation, blob retention policy and audit records that do not retain deleted content. Do not treat “never delete” rules from the trading ledger as universal ACE memory policy.

### 2. Embeddings and vector retrieval

Define an embedding-provider contract independent of the generation model:

- inspect model identity, dimensions, tokenizer/preprocessing and modality;
- embed one or many inputs with cancellation and bounded batches;
- report normalization, output dtype and version;
- store the embedding model ID + revision + preprocessing version with every vector;
- reject comparisons across incompatible embedding spaces unless an explicit migration exists.

The first on-device baseline should be **int8-quantized embedding storage with bounded candidate selection and SIMD dot-product reranking**, consistent with the existing ADR-041 research direction. Avoid a remote vector database, HNSW or a heavyweight service in the first mobile slice. Keep an exact/high-precision evaluation path so recall loss is measurable.

**TurboQuant is a research candidate, not an automatic dependency.** Google's TurboQuant combines PolarQuant-style rotation/scalar quantization with a 1-bit Quantized Johnson–Lindenstrauss residual correction. The work targets both KV-cache compression and high-dimensional nearest-neighbour search. It is promising for reducing memory and search cost, but the headline results are research results on tested model/hardware configurations, not proof of gains on LFM3 or Android ARM64. Evaluate it as a replaceable quantization backend against int8 and exact baselines. Do not block ACE on a TurboQuant port.

Separate two use cases:
- **Embedding/index quantization:** evaluate recall@k, MRR/NDCG, build time, bytes/vector, query latency and exact-rerank cost.
- **Inference KV-cache quantization:** evaluate token/logit or task-quality drift, memory, decode latency and session correctness on the actual LFM3 backend. This is a separate engine concern, not the RAG vector index.

No claim of “zero accuracy loss” is accepted without LOLA's own same-device and same-model evaluation. GPU H100 speedups must not be extrapolated to a Pixel.

### 3. FlashMem and latent memory

Treat FlashMem (Hou et al., Findings of ACL 2026) as an advanced research direction. It synthesizes latent memory from transient backbone states using a Shared-KV Consolidator and proposes an attention-entropy Cognitive Monitor to trigger consolidation when uncertainty is high.

This is not a drop-in replacement for external memory or RAG:
- it is coupled to model internals and the backbone's hidden/KV representations;
- it requires backend access to the relevant hidden states and cache lifecycle;
- latent state is not naturally portable across different model families, versions or quantizations;
- it is harder to inspect, cite, migrate and selectively delete than provenance-linked external records.

Therefore: do not make FlashMem a foundational ACE dependency and do not persist raw model KV as the canonical memory store. Preserve an optional future interface for model-specific latent-memory providers, behind capability negotiation and strict session/model/version scoping. First establish reliable external memory and retrieval.

### 4. Skills

Adopt the Agent Skills open standard unchanged:
- a skill is a directory containing `SKILL.md`;
- YAML frontmatter uses standard fields;
- LOLA-specific metadata belongs only under the sanctioned `metadata:` map;
- progressive disclosure: metadata/index first, skill body on activation, references/assets/scripts only when needed;
- validate name, description, compatibility, allowed tools and directory consistency at install/load time.

A skill is a **reusable capability package**, not an agent, model, tool or workflow graph. It may declare its purpose, input/output expectations, required capabilities, model preferences, pipeline affinity, resource envelope and allowed tool IDs through documented body conventions and namespaced `metadata` extensions. Tool permissions remain host-owned; `allowed-tools` is a constraint/request, never a grant. Skills cannot override budgets, privacy policy, model permissions or user confirmation.

Keep the skill index small in prompt context. Search metadata when the installed catalogue grows; load full instructions only after selection. Reuse the existing legacy parser only as implementation reference: port or extract generic validation after dependency review, but do not make ACE depend on `lola-harness` or import trading-specific skills/config.

### 5. Workflow Graphs

Introduce a declarative, versioned **Workflow Graph** distinct from skills. A graph is an execution plan composed from typed nodes and edges. The default graph is a DAG; bounded loops are explicit control-flow nodes with declared iteration, time, token and tool-call limits. Cycles are rejected unless represented by an approved bounded-loop construct.

Initial node families:
- `input` / `output` — typed workflow boundary;
- `skill` — activate a validated skill;
- `retrieve` — memory/document retrieval;
- `model` — structured decision or generation call;
- `stream` — user-facing generation with incremental events;
- `tool` — propose a host-registered tool call;
- `transform` — deterministic parsing, mapping or filtering;
- `condition` / `route` — deterministic branch on typed data;
- `parallel` / `join` — bounded fan-out and merge;
- `validate` / `approval` — schema, evidence, policy or user-confirmation gate;
- `memory.write` — submit a memory proposal to the governed write path;
- `terminal` — success, failure, cancellation or handoff.

Every node declares typed inputs/outputs, version, timeout, resource estimate, retry/idempotency policy and failure semantics. The graph declares ID/version, input/output schemas, required capabilities, pipeline affinity, execution budgets, privacy scope and provenance policy. Node outputs are immutable values or references; large documents and media travel by content reference, not copied into every event.

Graph validation happens before execution: schema compatibility, missing nodes, cycles, unbounded fan-out, unreachable terminals, unknown skills/tools/models, permission mismatch and budget overflow must fail closed. Graph execution is observable: every node emits start/result/failure/cancel events with timing, provenance and resource usage.

Do not turn this into an unrestricted visual programming language or arbitrary-code runner. Third-party scripts remain out of the initial scope; if later supported, they require a sandbox, capability allowlist and explicit resource limits.

### 6. Dual-pipeline orchestration

Decision and Streaming remain independent peers with separate lifecycle, budgets and cancellation.

**Decision owns:**
- request/task classification and plan selection;
- skill discovery and activation;
- workflow graph validation, scheduling and branching;
- memory retrieval policy and evidence selection;
- model/backend selection based on capability and device profile;
- tool proposal validation, permissions, approvals and dispatch;
- retries, recovery, evaluation and terminal state.

**Streaming owns:**
- model session and mutable inference state;
- prompt/context snapshot consumption;
- prefill, token decode and ordered output events;
- backpressure, cancellation and generation diagnostics;
- model-specific streaming/tool-call parsing as typed proposals only.

Communication is via typed events and immutable run snapshots. Decision must never hold a lock that blocks Streaming token delivery. A Streaming model's tool-call proposal returns control to Decision; Decision validates/executes the tool or retrieval node and starts a subsequent generation segment with a new immutable context snapshot. Do not mutate an in-flight prompt or silently splice newly retrieved text into a running decode.

The runtime may host multiple streaming model profiles/backends in future, but the initial ACE vertical remains LFM3-1.7B on the reference path until the physical-device acceptance gate passes. Model choice is capability- and budget-driven, not hardcoded into skills or graph definitions. The decision path may use deterministic policy first; a dedicated decision model is optional and must not be required for correctness or authorization.

### 7. Android and resource policy

All work is local-first and in-process on Android. Kotlin owns Android lifecycle, UI, permissions and platform APIs; Rust ACE owns orchestration, graph validation/execution, memory policy and budgets; inference backends own tensor/KV/session state. Avoid a per-token FFI crossing. Stream bounded batches/events.

Each workflow/run is subject to host-owned wall-clock, memory, model-residency, token, tool, branch, retry and cancellation budgets. Device capability includes available memory, thermal status, battery/power mode, CPU/GPU support and current model residency. Retrieval and consolidation are cancellable and may be deferred under pressure. No background “dreaming” work may compete with active decode; schedule it only in an explicit idle/maintenance window.

### 8. Proposed module boundaries

Create ACE-native boundaries only when implementation starts:
- `ace-memory`: schemas, provenance, lifecycle, retrieval contracts and SQLite implementation;
- `ace-skills`: Agent Skills parser, validator, index and activation policy;
- `ace-workflows`: graph schema, static validation and deterministic scheduler contracts.

These crates must not depend on `lola-harness`, `lola-venue`, trading policy or market types. They may initially depend on `ace-core` shared IDs/events/budgets and `ace-inference` capability contracts. Keep implementation minimal and avoid introducing a graph database or vector-store dependency before benchmarks justify it.

## Consequences

### Positive
- Skills, memory and workflows are reusable and model-independent.
- Decision can compose retrieval, tools and models without entangling Streaming.
- The system has a testable boundary for multi-model streaming and model-specific memory experiments.
- Existing SQLite/FTS/graph/kNN research can inform ACE without importing the legacy trading domain.
- Memory quality, retrieval quality and execution quality become measurable.

### Costs and risks
- More contracts and schema/version management are required.
- Hybrid retrieval adds index maintenance and evaluation work.
- Graph execution introduces validation, cancellation and observability requirements.
- FlashMem-like latent memory would be model/backend-specific and cannot be assumed portable.
- TurboQuant needs an independent implementation and mobile benchmark before adoption.

## Implementation sequence and acceptance gates

1. **ADR + schemas:** approve this boundary; publish example SKILL.md and workflow graph fixtures. No runtime feature claim.
2. **Skills foundation:** standard parser/validator, progressive disclosure, catalogue search, tool-ID validation. Tests for malformed manifests and prompt-budget behaviour.
3. **Memory v0:** SQLite event/episode records, FTS5, typed facts, provenance, supersession/deletion and bounded graph traversal. Tests for conflict, source lineage, deletion/index invalidation and restart durability.
4. **Retrieval baseline:** lexical + structured filters + graph; benchmark recall@k, MRR/NDCG, p50/p95 latency, bytes stored and context tokens. Establish no-memory and lexical-only baselines.
5. **Embedding path:** model-independent embedding contract, pinned embedding model metadata, int8 storage and exact rerank evaluation. Add TurboQuant only as an experimental backend after baseline.
6. **Workflow graph v0:** typed DAG validation, deterministic scheduler, bounded parallelism, explicit bounded-loop node, event trace and cancellation. No arbitrary scripts.
7. **Dual-pipeline integration:** Decision schedules graph nodes; Streaming runs generation nodes; typed tool-call handoff and immutable context snapshots. Prove Streaming remains responsive while Decision retrieves or validates.
8. **Consolidation research:** idle-window episode→semantic/procedural distillation with provenance and human/policy validation. FlashMem latent-memory spike only if backend state APIs and LFM3 support are demonstrated.

Required benchmark suite: retrieval (Recall@k, MRR, NDCG, answer faithfulness/source attribution), memory lifecycle (write correctness, conflict/supersession, deletion, stale-index prevention, cross-session recall), workflow (node correctness, deterministic replay, cancellation, bounded loops, failure recovery), and Android (TTFT, decode tok/s, PSS, thermal, battery, retrieval latency under concurrent decode). Always compare on the same device, model revision, prompt and thermal conditions.

## Research references

- Google Research, [TurboQuant: Redefining AI efficiency with extreme compression](https://research.google/blog/turboquant-redefining-ai-efficiency-with-extreme-compression/) (24 Mar 2026).
- Zandieh et al., [TurboQuant: Online Vector Quantization with Near-optimal Distortion Rate](https://arxiv.org/abs/2504.19874), ICLR 2026.
- Hou et al., [FlashMem: Distilling Intrinsic Latent Memory via Computation Reuse](https://aclanthology.org/2026.findings-acl.230/), Findings of ACL 2026.
- [FlashRAG](https://github.com/RUC-NLPIR/FlashRAG) — research toolkit and configurable RAG pipeline patterns; Python research reference, not an Android dependency.
- [Agent Skills specification](https://agentskills.io/specification.md) — portable skill packaging and progressive disclosure.
- Existing repository references: ADR-028, ADR-030, ADR-041; `docs/ace/ARCHITECTURE.md`, `docs/ace/INFERENCE_CONTRACT.md`, `crates/lola-memory`, `crates/lola-harness/src/skills.rs`.

## Open questions deliberately deferred

- Which embedding model and dimension fit Android memory/latency budgets?
- Whether vector search adds measurable value over FTS5 + structured/graph retrieval for actual ACE workloads.
- Whether TurboQuant can be implemented efficiently on ARM64 and preserve retrieval quality at LOLA's target dimensions.
- Whether the LFM3 backend exposes enough stable internal state for a FlashMem-inspired experiment.
- Whether workflow definitions should use YAML or JSON as the canonical serialized form; the schema and semantics matter more than the syntax.
- Whether a separate decision model is worthwhile after deterministic routing and LFM3 streaming are measured.
