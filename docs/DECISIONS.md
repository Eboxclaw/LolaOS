# LOLA Decision Log (ADRs)

Decisions locked with the owner on 2026-11-10 after five interrogation
rounds. Format: context → decision → rejected alternatives.

---

**ADR-001 — Identity: everything LOLA.** The product, repo, package and
docs are LOLA ("LOLA OS"). The workspace folder `EdgeTOS` is an artifact of
history, not a brand. *Rejected:* EdgeTOS branding; split naming.

**ADR-002 — Rust core, single process, no sidecars.** Mobile constraints
apply from day one: no fork assumptions, bounded memory, single-writer
SQLite, GGUF-compatible weights. The user's direction: "multiplatform from
start with Rust and Compose or Kotlin". *Rejected:* Python-first (RFC-001's
own recommendation — dies with the mobile requirement); any Python
sidecar; TS anywhere.

**ADR-003 — Own nano-RIPL inference layer.** *(Revised 2026-11-11, owner
direction: "we are making our own llama implementations specialized for
lfm3, weights and attention layers".)* The lola-custom engine — our own
kernels for the tri-hybrid (conv k=7, SWA-256, partial-RoPE global
attention, head gates, sandwich norm), our own repacked weight layout
(GGUF as interchange only), our own dual-slot scheduler — **is the
product**, not a parallel track. llama.cpp serves exactly two external
roles: correctness oracle (golden outputs, per-kernel parity gates) and
GGUF import source; nothing of it ships inside LOLA. Inspired by
llama.cpp techniques (mmap, quantization), RIPL economics (~1 GB/model @
4-bit, sub-50 ms ambition), Hermes memory patterns and the Pi harness —
never forked or copied. *Rejected:* llama.cpp as the shipped engine;
candle (weaker model coverage); Python inference sidecar.

**ADR-004 — NautilusTrader dropped from v0.1.** Its supported surface is
Python; embedding its Rust crates directly is unsupported. Its
conservative-fill philosophy survives in `lola-replay` defaults.
*Rejected:* Rust core + Nautilus Python sidecar (two runtimes forever,
Android harder); embedding nautilus Rust crates (unsupported path).

**ADR-005 — Hyperliquid first; Nado designed-for, built later.** Perps +
spot, leverage 1–10x in the schema, via the official
`hyperliquid-rust-sdk`. Nado (Inchain exchange) gets a venue adapter behind
the same `Venue` trait when its API surface is validated. *Rejected:*
NautilusTrader's first-party HL adapter (good, but Python-bound — recorded
as the revisited-alternative if LOLA ever wants a desktop-only backtest
beast); CEX adapters early.

**ADR-006 — Model set.** Active: d1-2B (decision) + LFM3-1.7B (agent),
Q4_K_M. The other five LFM3 checkpoints are registry metadata only.
d1 probabilities are model probabilities, never trade-profit probabilities.

**ADR-007 — Profile-based residency.** Laptop: both models resident.
Constrained/Android: d1 resident, LFM3 on-demand via warm page cache.
Decision models are never downgraded; a plan without a resident decision
model refuses to boot. *Rejected:* always-both-resident (Android budget);
d1-only-resident everywhere (wastes laptop RAM).

**ADR-008 — Leverage: testnet free, mainnet ladder.** Schema supports 10x.
Testnet may run any leverage (it exists to gather liquidation/funding/
calibration data). Mainnet starts 1x; rungs 2x→5x→10x unlock only via
versioned policy changes backed by measured evidence, approved by humans.
The risk engine hard-clamps; models can never raise caps. *Rejected:*
10x mainnet day one (unmeasured 2B-model calibration holding 10x is how
accounts die); 1x everywhere including testnet (starves the data).

**ADR-009 — Memory: immutable core + TTL'd bulk; hybrid semantics.**
Decisions/outcomes/orders/fills/risk immutable forever; facts canonical
(upsert per `(scope, canonical_key)`); graph edges versioned with validity;
bulky payloads compact to content-addressed blobs on TTL. "Over-storing" =
storing a payload twice or keeping bulk in hot rows past TTL.
*Rejected:* keep-everything-forever (Android storage); aggressive lossy
compaction (kills debuggability); full event sourcing (replay cost grows
with history); current-state-only (kills decision lineage — LOLA's entire
reason to exist).

**ADR-010 — No vectors in v0.1.** Deterministic signatures + SQL buckets +
graph + FTS5 only. Reconsidered only if measured retrieval quality after
S5 justifies a third model competing for edge RAM. *Rejected:* local
embedding model from day one; external embedding API in any path.

**ADR-011 — Similarity = bucket signature + bounded numeric kNN.** Coarse
signature narrows via index; weighted normalized-feature distance ranks
top-k. Deterministic, explainable, fast. *Rejected:* exact-signature-only
(too blunt); deferred similarity (A2 experiment impossible without it).

**ADR-012 — Risk engine is ring 0.** Deterministic, independent of both
models, fail-closed. Stable check names fixed in code. Cache hits never
bypass it.

**ADR-013 — Ruthless cut; sequence-only roadmap.** v0.1 = harness + one
venue + Experiment A. Equities, strategy discovery (B ladder),
specialists: out. No dates; exit criteria per slice. RFC-001's 12-epic
backlog contradicted its own "prove it deserves to exist first" principle.

**ADR-014 — Key management & UI/UX carved out.** Separate discussions,
zero assumptions encoded. The only trace in v0.1 is
`venue.yaml: credentials.mode: none` and this doc's question list.
See `KEY_MANAGEMENT.md`, `UI_UX.md`.

**ADR-015 — d1 shadow mode first.** d1 classifies real candles and its
calibration (ECE/Brier per regime) is measured before it may accept or
veto live signals. Go/no-go recorded here before A1 runs with d1 active.

**ADR-016 — Venue trait is the only venue boundary.** All venue knowledge
lives behind `lola-venue::Venue`. Nado drops in later without kernel
changes.

---

*Harness deep-dive round (researched 2026-11-10, locked same day; three
research agents: chip backends, Pi/Hermes/llama.cpp anatomy, LFM3
internals — sources in HARNESS/NANO_RIPL/MEMORY docs).*

**ADR-017 — Omni model pair from day one.** Laptop profile hosts
d1-Omni-2B (Q8_0, 1.85 GB — no 4-bit exists) + LFM3-Omni-2B (Q4_K_M,
~1.1 GB): spoken questions/answers and video understanding are first-class
(session `media` entries, modality router, mmproj sidecars). Pairs are
declared per profile; constrained devices fall back to the text pair
behind the same two slots. *Rejected:* text-pair-only (re-architecture
when voice arrives); omni-everywhere (Q8 floor + 32K ctx too heavy for
Android-class budgets today).

**ADR-018 — Compute: Tier-1 universal only in v0.1; vendor plugins via
dlopen C-ABI later.** Tier 1 = Rust CPU kernels (AVX2/512·NEON) +
Vulkan/wgpu (production on AMD, works on Tensor Mali, **probed with CPU
fallback on Adreno** where Vulkan is driver-broken). Hexagon/QAIRT and
LiteRT-Tensor are designed-for plugins, not v0.1 scope. *Rejected:*
Hexagon in v0.1 (packaging unverified, engine lacks parity); CPU-only
(throws away AMD/Tensor GPU); NNAPI (deprecated), SYCL (AMD support
removed), ROCm (loses decode to Vulkan).

**ADR-019 — Crate split: `lola-memory` + `lola-harness`.** The memory/DB/KG
subsystem — the most important part — gets its own crate with its own
tests and budgets; the kernel (loops, sessions, tools, risk types) sits on
top. `lola-core` dissolves. *Rejected:* monolithic core (crown jewel
shares a crate with trading concerns); full split with a `lola-types`
crate (ceremony over a young codebase).

**ADR-020 — Harness shape: Pi session-tree + ≤10 native tools, no MCP.**
Sessions are append-only event trees (id/parent_id; entry taxonomy
message/model_change/compaction/context_edit/custom/media) in SQLite; one
loop core serves every frontend; tool surface ≤10, schema'd, gated,
progressive disclosure. *Rejected:* Hermes' 70+ tools/28 toolsets and MCP
(the context tax); JSONL files (SQLite already won in ADR-009).

**ADR-021 — Frozen memory snapshots.** Curated memory (L0) is snapshotted
at session start and injected once; mid-session changes land next session.
Cache-stability law 1 — on-device, a rewritten prefix is real prefill
compute. *Rejected:* Hermes-style per-turn micro-compaction by default.

**ADR-022 — Compaction-as-archive with witnesses.** A compaction row
carries summary + first_kept_event_id + absorbed_event_ids[]; originals
are never deleted and stay FTS-searchable; the KV-side twin is
`seq_rm`. *Rejected:* in-place history rewrite.

**ADR-023 — GGUF is the single weight format.** Liquid's official GGUFs
(all 7 checkpoints, Q4_K_M~1.05–1.1 GB; mmproj sidecars for omni) drop in
unmodified; oracle floor pinned at llama.cpp b11960+; no conversion fork.
Custom engine reads the same files. *Rejected:* safetensors/ONNX pipelines
in the runtime.

**ADR-024 — Decision slot is prefill-only with transient KV.** d1 answers
in one forward pass with zero generated tokens; its KV is scratch, evicted
after readout. Residency accounting counts weights + agent KV + d1 prefill
peak. *Rejected:* treating both slots as streaming decoders (wastes RAM,
mis-models d1).

---

*Mobile-first rework round (researched 2026-11-11; three research agents —
stack validation, mobile dual-model inference, context/skills/memory SOTA.
Sources in MOBILE_FIRST/CONTEXT/ENGINE docs. Question round unanswered →
recommended options taken: phone runs the loop; Android+desktop only;
adopt skills standard; docs+types+diagram deliverable.)*

**ADR-025 — Mobile-first inversion: the phone runs the trading loop.**
Laptop/PC is second. Feasible because LOLA is decision-first: a d1
prefill burst per closed candle is thermally trivial (phones sustain
~5–10 *heavy* queries/hour; we use milliseconds). Loop = WorkManager
ticks + checkpointed FGS (6h-cap aware); agent chat/voice = user-initiated
bursts; checkpoint/resume is the normal path, never the error path.
*Rejected:* phone-as-cockpit-only (mobile-first becomes UI-first); device
handoff with ledger replication (added complexity now, revisit later).

**ADR-026 — Android 17 Memory Limiter is a design law.** The limiter
meters anon RSS+swap; file-backed mmap'd weights don't count. Design to
≤ ~2.5 GB anon on 12 GB+ devices; weights stay file-backed; `mlock` is
unavailable (64 KB RLIMIT) so reclaim-ability (evict + re-fault) is
designed for, not against. *Rejected:* heap-pinned weights; assuming
pre-Android-17 behavior.

**ADR-027 — Stack: Compose Multiplatform + UniFFI + in-process Rust;
WASM never for inference.** CMP's Android target *is* Jetpack Compose
(desktop door free); UniFFI Kotlin bindings serve ART and desktop JVM from
one generated set (batch calls; explicit `cancel()` across FFI); the
phantom-process killer forbids child processes, so the engine lives
in-process. WASM's only reserved role: a `wasmi` sandbox for untrusted
third-party skills, later. iOS not designed-for. *Rejected:* WASM
inference (~15–50% overhead, no NPU); out-of-process engine; iOS now.

**ADR-028 — Adopt the Agent Skills open standard as-is.** SKILL.md
frontmatter + 3-level progressive disclosure (metadata → body →
resources); LOLA extensions only via the sanctioned `metadata:` map;
`search_skills` past 40 skills; install-time validation. Free ecosystem
(skills.sh, multi-vendor adoption) instead of a bespoke format.
*Rejected:* custom skill format; deferring skills entirely.

**ADR-029 — Personality = prompt-as-ledger.** Persona is a versioned
ledger object, injected verbatim at position 1, carried **verbatim through
compaction** (never summarized — practitioner reports show persona rules
are what compaction drops), checksummed into every decision.
*Rejected:* prompt-as-config files outside the ledger.

**ADR-030 — Memory grows four things** (on top of MEMORY.md v2):
episodes-as-provenance; evolving entity summaries; a beliefs layer
separated from facts (evidence-vs-inference, provenance + expiry); a
background "dreaming" consolidation job. *Rejected:* vector DBs, vendor
memory platforms, MemOS-style abstraction layers.

**ADR-031 — Decision improvement = outcome-weighted case bank.** Retrieve
k≤5 **settled** past decisions similar to the current state, weighted by
recency × outcome magnitude, injected into the d1 support pass
(Memento pattern). Plus: calibration reports + isotonic/temperature
recalibration of d1 probabilities; ExpeL-style contrastive lessons into
the beliefs layer; ≤2-arm bandit policy promotion with kill-switch.
Settled-only is a code-level invariant. Persona and risk limits are
outside the loop, always.

**ADR-032 — The UI never dumps content into the agent's context.** Quick
actions (explain / escalate / kill switch / support pass) trigger tools
and retrieval; references travel as identifiers; payloads enter context
only through the support-pass gate. Requirements recorded in UI_UX.md;
visual design still future.

---

*Tool layer + LiquidCodec round (2026-11-12; one verification agent for
Android APIs/permissions + LFM3 tool-call syntax).*

**ADR-033 — Categorized tool registry; read-only default with
confirm-on-write.** Tools live in stable categories (`calendar,
time_alarm, notifications, device, files, media, apps, accessibility,
memory, market, system` — append-only ids); the registry may grow large
while the *prompt-injected* set stays ≤10 with `search_tools` beyond
(Tool Search). Each tool carries a `SafetyTier`: **read-only** by default;
**confirm-on-write** for every native write (agent proposes → human
confirms in the UI); **gated** for policy-grant tools; **disabled**.
`ToolSide` splits Core (Rust-resident: memory/market/system) from
AndroidBridge (Kotlin-side via UniFFI). Config: `config/tools.yaml`.
*Rejected:* full actuation; per-tool security theater prompts (the
confirm-on-write flow is one deliberate tap, not a dialog per call).

**ADR-034 — Accessibility category included, hard-gated, empty in v0.1.**
The taxonomy reserves `accessibility` (other-app screen reading /
actuation) because it is the most powerful native surface — but it ships
disabled with zero tools, requires explicit special-access grant + policy
version + per-action confirmation when it ever ships, and we accept the
Play-review scrutiny that comes with it. *Rejected:* excluding the
category slot entirely (would force a taxonomy break later).

**ADR-035 — LiquidCodec is a first-class engine component (K9).** The
~74M codec's encoder and decoder are built from LFM3 blocks, so our K1–K3
kernels cover it; GFSQ makes speech-out a few parallel refinement steps
(ideal for phones); the codec pins to a LITTLE core (361×/87× RT on one
thread); decoder-only QAT Q4_0 (3.5× smaller) is the default; codec
builds import through the standard GGUF pipeline. Registered in
`models.yaml: codecs:`; milestone K9 added.

---

*Router round (2026-11-12). Owner framing challenged and settled:
"encoder/router" → **calibrated scoring head + deterministic table**.*

**ADR-036 — d1 = calibrated scoring head; routing = deterministic
pass-forward policy owned by the harness.** Two tables, two owners:
*question schemas* (named questions + criteria/rubrics, versioned) are
shown to the model so answers stay consistent and cacheable;
*pass-forward policies* (destinations, thresholds on **calibrated**
scores, hard budgets: context tokens/turn, escalations/hour, ≤16
questions/set) are harness-only — the model never sees its limits, so it
cannot game them. Missing/unknown answers and over-budget passes fail
closed; every route is a ledger row. Thresholds are PROVISIONAL
conservative constants until S5 calibration reports exist, then change
only via versioned policy bumps. `question_schema_version` joins the
decision-cache invalidation set (9 dependencies) — schema drift
invalidates cached answers. *Rejected:* model-sees-its-own-rules
(confidence inflation around thresholds, unauditable routing); fully
configurable thresholds pre-calibration (tuning against noise); the
"encoder" framing (hides schema + calibration drift).

---

*Research-triage round (2026-11-15): owner-supplied dual-model agentic
research, filtered per instruction "keep our vertical always on LFM
models". Full ledger: docs/RESEARCH.md.*

**ADR-037 — Research adoption policy: the vertical stays LFM.** External
research is triaged into RESEARCH.md (adopted techniques / validated
already-built / excluded) before anything touches the roadmap. Adopted:
schema-skeleton decoding (slot B only — d1's zero-token readout supersedes
it for slot A), argument-accuracy as an S5+/S9 metric, tool-result
caching (extends decision cache + case bank), MLCommons agentic
trajectory metrics for S2m/S9, AppFunctions as a future tool-interface
class, SPORK read-only-gated later. Excluded: FunctionGemma/Qwen/Gemma as
models, LiteRT-LM + ADK Kotlin as the runtime, vLLM/SGLang serving — each
contradicts ADR-003 rev./017/018/027. The paper's open "routing head in a
shared runtime" question is already answered by d1 + ADR-036.

---

*Optimization-policy round (2026-11-15): fruits researched with measured
micro-benchmarks (Apple M4 floor: 77 GB/s single-thread axpy vs our ~1 GB/s
effective @64K); Liquid deep-dive informs strategy. Docs: OPTIMIZATIONS.md,
LIQUID.md, LORD.md.*

**ADR-038 — Benchmark-gated optimization policy.** Every optimization
lands behind its measured gate; gates are not always speed — quality
(golden token-identity, KL, ppl-Δ, needle-in-haystack), memory (RSS),
energy (µJ/token), TTFT, and tail latency (p99 under concurrent load) are
first-class gates. Anti-fruits are recorded to prevent wasted effort
(#12 layer-skip: no evidence for hybrid stacks). Quality harness
(gap #1) is built BEFORE the quantization fruits that depend on it.
Fruit order: F1+F11 (attn restructure, 8–20× @64K headroom) → F2 (q8 KV,
−73% memory) → F4 (SQLite pack) → F3/F6 (prefix-KV + d1 state-prefix) →
F5 (NEON exp, Android) → F7 (prompt-lookup spec decoding).

**ADR-039 — Liquid positioning.** LOLA builds the artifact Liquid's
strategy leaves open: the Android-first **dual-model runtime** (d1 gate +
LFM3 agent in one process) plus the **d1 calibration harness** nobody
ships. Kernels are generic config-parameterized operators (LFM4
insurance) positioned for upstream contribution (Vulkan/Adreno where
llama.cpp is weak), benchmarked on Pipette surfaces. We complement, never
fork: their models, their GGUFs, their fine-tune recipes; our runtime,
ledger, router, and calibration tooling.
