# UI/UX — CARVED OUT (no decisions here)

Status: **deliberately undecided** (ADR-014). Nothing about interfaces is
designed, sketched or implied by any slice.

Direction stated by the owner (recorded, not designed): multiplatform from
the start with Rust, and **Compose Multiplatform or Kotlin** named as the
UI direction. The kernel therefore keeps a strict separation: `lola-core`
and siblings expose data and contracts; every presentation concern belongs
to a future UI layer that consumes them.

Questions the future discussion must answer:

1. Compose Multiplatform vs Kotlin Multiplatform vs plain Compose on
   Android — and does the laptop UI share code with the mobile UI?
2. What does the operator actually need to watch (decision ledger, regime
   classifications, escalation queue, risk state, kill switch)?
3. How much surface is read-only dashboards vs interactive control?
4. Relationship to S8's kill switch and ADR-008's leverage-ladder approvals
   — which actions require a human in the loop?
