# LOLA Context — Discipline, Skills, Personality, Improvement

Companion to `docs/HARNESS.md` + `docs/MEMORY.md`. ADR-028..032.
The owner's rule, verbatim in spirit: **do not load everything into the
agent — give it fast ways to search** (graph, DB, parsing, decision-model
support passes with accuracy + score).

## 1. Context discipline (the 9 principles)

Small-context (32K) mobile agent; every row is sourced.

| # | Do | Don't | Source |
|---|---|---|---|
| 1 | Budget tokens: ~1–2K persona/policy, ~2–4K hot state, skill index or `search_skills`, rest JIT, 4–8K output headroom | Preload "just in case" — context rot hits even SOTA models | [Chroma context rot](https://research.trychroma.com/context-rot) · [Anthropic](https://www.anthropic.com/research/effective-context-engineering-for-ai-agents) |
| 2 | Keep the prefix byte-stable, append-only; memory = frozen snapshot per session | Mutate mid-session context or remove tools mid-run | [Manus](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus) |
| 3 | Store state in the ledger; pass **identifiers**; pull content via tools JIT | Paste documents/tables/payloads wholesale | Anthropic JIT retrieval |
| 4 | Compact recall-first; clear old tool results first; persona carried **verbatim** | Summarize the persona; drop resolved decisions from summaries | Anthropic compaction + practitioner persona-drift reports |
| 5 | Inject a tiny memory top-layer (~200 lines / 25 KB); search below it | Store what the DB already shows; write memory indiscriminately | [Anthropic memory components](https://www.anthropic.com/research/components-of-agent-memory) |
| 6 | Small pass gates what the big model sees (schemas, docs, skills) | One giant prompt or one giant retrieval step | Anthropic [Tool Search](https://www.anthropic.com/research/advanced-tool-use) · [Adaptive-RAG](https://arxiv.org/abs/2403.14403) |
| 7 | Offload exploration to sub-passes returning 1–2K distillates (d1 support passes) | Let one thread accumulate 10K+ tokens of exploration | Anthropic sub-agents |
| 8 | Consolidate in the background ("dreaming"), never inline | Block the loop on memory maintenance | [Letta sleep-time](https://www.letta.com/blog/sleep-time-compute) · Anthropic |
| 9 | FTS5 + graph + parsers first; embeddings only where lexical measurably fails | Vector DB by default; vendor memory leaderboards | Letta "plain files = 74% LoCoMo" · Hermes FTS5 |

## 2. Skills ↔ tools linkage (ADR-033)

Skills declare their tool needs via the standard's `allowed-tools` field
(advisory in the spec, enforced by us: a skill's tools must exist in an
enabled category of `config/tools.yaml`). Skills never ship their own
tools; they compose existing categories. Past [`MAX_PROMPT_TOOLS`], the
agent sees `search_tools(query)` rather than a tool list — same pattern as
skills, and **native to the model**: LFM3's `defer_loading=True` keeps
tool schemas out of the prompt until a search returns them as
`tool_reference` (template injects `<additional_tools>`) — verified on the
[LFM3-1.7B card](https://huggingface.co/LiquidAI/LFM3-1.7B). Native tools
execute Kotlin-side via the UniFFI bridge (`docs/TOOLS.md` §3) with
read-only/confirm-on-write posture.

## 2b. Skills (ADR-028): adopt the Agent Skills standard

Format: [agentskills.io spec](https://agentskills.io/specification.md)
(open standard, Dec 2025; adopted by Codex/Cursor/Copilot/Gemini CLI/MS
Agent Framework). `SKILL.md` = YAML frontmatter + body; 3-level
progressive disclosure: **L1** metadata (~100 tok/skill, session-start
index) → **L2** body (<5K tok, on activation) → **L3** resources (on
demand). LOLA extensions live in the sanctioned `metadata:` map
(`lola-tier: support-pass` etc.). Past 40 skills, the index becomes a
`search_skills(query)` tool (Tool Search pattern). First-party skills run
native; untrusted third-party skills get the `wasmi` sandbox later.
Implementation: `lola-harness/src/skills.rs` (parse + spec validation).

## 3. Memory additions (ADR-030)

On top of MEMORY.md's L0/L1/L2 + validity-versioned graph (which already
matches Graphiti's bi-temporal core):

- **Episodes as provenance** — every derived fact/lesson traces to ledger
  rows ([Graphiti](https://github.com/getzep/graphiti)).
- **Evolving entity summaries** — denormalized, regenerable nodes.
- **Beliefs layer** — separated from facts, evidence-vs-inference split,
  each belief carries provenance + expiry ([Hindsight,
  arXiv:2512.12818](https://arxiv.org/abs/2512.12818)).
- **"Dreaming" job** — idle/background consolidation: reconcile
  invalidated edges, regenerate summaries, promote durable notes,
  extract lessons from settled decisions ([Anthropic memory components](
  https://www.anthropic.com/research/components-of-agent-memory): wiki+search
  61% vs 53%, ~20K tok/question saved).

## 4. Personality = prompt-as-ledger (ADR-029)

Persona is a **versioned ledger object**, injected verbatim at position 1,
carried **verbatim through compaction** (never summarized — compaction is
exactly where practitioner reports show personas die), and checksummed
into every decision for audit. Implementation: `lola-harness/src/persona.rs`.

## 5. Decision improvement (ADR-031) — the ledger pays rent

All offline/scheduled; persona and risk limits are **outside** the loop.

1. **Settlement job** — horizon completes → write realized outcome to the
   ledger (append-only).
2. **Calibration report + recalibration** — ECE/Brier per answer type per
   policy version; fit isotonic/temperature scaling on trailing settled
   decisions; apply inside the d1 support pass (verbalized probabilities
   are calibratable — [Tian et al.](https://openreview.net/forum?id=g3faCfrwm7)).
3. **Outcome-weighted case bank** — retrieve k≤5 settled cases similar to
   the current state, weight = recency-decay × outcome magnitude; inject
   into the d1 support pass ([Memento, arXiv:2508.16153](
   https://arxiv.org/abs/2508.16153)). Settled-only is an invariant
   (`lola-memory/src/casebank.rs`).
4. **Lesson extraction** — contrast realized winners vs losers (ExpeL
   pattern, [arXiv:2308.10144](https://arxiv.org/abs/2308.10144)) → ≤10
   natural-language market lessons into the beliefs layer with expiry;
   never auto-promoted to L0 without a review gate (Voyager's lesson:
   unverified skill material collapses without verification).
5. **Policy promotion** — ≤2 arms (incumbent + challenger), sequential
   test on the rolling settled window, kill-switch back to the previous
   version. This *is* the A2 experiment's engine.

## 6. The d1 support pass (novel — name it proudly)

A prefill-only structured decision model gates what the streaming agent
sees: skill/tool schema selection (Tool Search role), retrieved-doc
filtering (CRAG role), query-complexity routing (Adaptive-RAG role), and
context compression decisions (LLMLingua-2 role) — as typed answers with
probabilities instead of token classifiers. No prior art found for this
composition; worth an external write-up once S5 produces calibration data.

**Formal contract:** [ROUTER.md](ROUTER.md) (ADR-036) — question schemas
shown to the model; pass-forward policy (thresholds on calibrated scores,
destinations, budgets) harness-only. Model proposes scores; policy
disposes.
