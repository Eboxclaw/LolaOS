# UI/UX — Requirements (carve-out partially lifted 2026-11-11)

Status: the **requirements below are owner-stated and locked**; visual
design, layout craft and motion remain a future discussion. Stack is
already decided independently (ADR-026/027): Compose Multiplatform over
UniFFI to the Rust core — the UI owns presentation only, never logic.

## Owner requirements (recorded verbatim in intent)

- **Chat-friendly main window** — conversational surface with the agent
  (text now, voice later via omni slot).
- **Session history** — durable, searchable (FTS5 via the ledger), with
  branching visible (the session tree is real; the drawer shows it).
- **FAB + quick buttons** — always-reachable actions.
- **Decision cards** — every decision surfaces its typed answers,
  probabilities, calibration-adjusted scores, and eventual outcome.
- **Voice input** (omni) when the omni pair is resident.

## Quick actions (initial set)

| Action | What it does | Context cost |
|---|---|---|
| Explain decision | opens the decision card + lineage (evidence, retrieval steps, case-bank cases) | zero — reads the ledger |
| Escalate to research | creates an agent session seeded with the decision id *as an identifier* | minimal — agent pulls via tools |
| Kill switch | risk engine halt; persists; requires human to clear | zero |
| Run support pass | d1 prefill over the current view's question set | burst only |

## The hard rule (ADR-032)

**The UI never dumps content into the agent's context.** Quick actions
trigger tools and retrieval; payloads enter context only through the
support-pass gate (`docs/CONTEXT.md`). A chat message may reference a
decision/session/graph node *by id*; the agent fetches what it needs.

## Open (future visual-design discussion)

Layout, theming, motion, information density, tablet/desktop shells,
widgets/notifications for decision events, accessibility.
