# LOLA Router — Pass-Forward Reference Tables

ADR-036. Code: `lola-harness/src/router.rs` · config: `config/router.yaml`.

## 1. The critique this design survives

The owner asked: is the decision model "almost like an encoder or a router"?

- **As a router — half right.** d1 does route (which content, schemas and
  escalations go forward — Adaptive-RAG / Tool Search territory), but a
  router is content-blind path selection. d1 also *scores payload quality*
  (sufficiency, conflict) and *is* the payload (regime labels,
  probabilities). "Router" hides the judgment that must be calibrated and
  audited.
- **As an encoder — mostly wrong.** Encoders preserve information for a
  decoder. d1 is lossy by design, its "codebook" (question schemas) is
  authored and versioned *by us* — schema drift silently stales every
  cached answer — and its probabilities need recalibration before any
  threshold comparison. An "encoder" gets trusted; a scoring head gets
  recalibrated.

**The frame we build on: d1 = a calibrated scoring head; routing = a
deterministic table owned by the harness.** *Model proposes scores; policy
disposes.*

## 2. Two tables, two owners

| Table | Owner | Shown to the model? | Purpose |
|---|---|---|---|
| **Question schemas** (`config/router.yaml: schemas`) | harness, versioned | **yes** — this is what d1 answers against | named questions + criteria/rubrics ⇒ consistent, cacheable, comparable answers |
| **Pass-forward policies** (`config/router.yaml: policies`) | harness, versioned | **never** — not in any prompt or state | destinations, thresholds (on calibrated scores), hard budgets |

The model cannot game limits it cannot see. If it knew the forwarding
thresholds, the predictable failure is confidence inflation sitting just
above them.

## 3. Architecture

```
              ┌────────── reference tables (ours, versioned) ──────────┐
              │ QUESTION SCHEMAS          │  PASS-FORWARD POLICY       │
              │ tick: regime(choice)      │  answer → destination      │
              │       candidate(noul)     │  threshold (calibrated)    │
              │       sufficiency(noul)   │  budgets: ctx tokens/turn, │
              │ gate: relevance(score)    │  escalations/hour, ≤16 Q   │
              │ escalation: novelty…      │  fail_closed: always       │
              └───────┬────────────────────┴───────────┬───────────────┘
                      │ what to answer                 │ applied after
 state + case bank    ▼                                ▼
 ────────────► d1 scoring head ──► raw scores ──► calibrator (S5) ──► ROUTER (deterministic)
                (one pass, ≤16 Q)                                        pass_to_risk · pass_to_agent_ctx
                                                                         (≤ budget) · escalate_agent
                                                                         (≤ budget) · ledger_only · discard
```

Destinations: `pass_to_risk` (decision path) · `pass_to_agent_context`
(counts against the per-turn token budget) · `escalate_agent` (counts
against the hourly budget) · `ledger_only` · `discard`.

## 4. Rules the code enforces (`route()`)

1. Thresholds compare **calibrated** scores only — never raw.
2. Missing answers → fail closed (`discard`, audited).
3. Policy references a question not in the schema → configuration bug →
   fail closed (`FailClosedUnknown`).
4. Budgets are hard caps evaluated in declared rule order; over-budget
   answers demote to `ledger_only` (`BudgetExhausted`).
5. ≤ **16 questions** per set (d1's trained size — larger is faster but
   less accurate; never larger).
6. Every decision is a ledger row: schema version · raw + calibrated ·
   threshold · policy version · destination · reason.

## 5. Provisional thresholds and the S5 unlock

Current numbers (`router.yaml`) are **PROVISIONAL** and conservative:
context pass ≥ 0.70, escalation ≥ 0.60, tick routing ≥ 0.55–0.60; budgets
2048 tokens/turn, 6 escalations/hour. They change **only** via versioned
policy bumps backed by S5's calibration reports (ECE per answer type per
policy version — ADR-031), never by ad-hoc tuning, never by the model.

## 6. Cache invalidation

`question_schema_version` is now a first-class decision-cache dependency
(`lola-memory::retrieval::CACHE_DEPENDENCIES`, 9 entries): change a
question set and every cached d1 answer for that decision point
invalidates. Schema drift can never silently serve.
