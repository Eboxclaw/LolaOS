# LOLA OS

A decision-first, edge-first agentic operating system for trading. One Rust
process, no sidecars, mobile constraints respected from day one.

> Don't solve the same problem twice. Retrieve what is known, decide
> deterministically when possible, spend model inference only where it
> creates value, record everything, improve from outcomes.

**Architecture & kernel:** [RFC-002 — Architecture](docs/RFC-002.md) ·
[HARNESS — the OS kernel](docs/HARNESS.md) ·
[ROUTER — pass-forward reference tables](docs/ROUTER.md)

**Memory & agent:** [MEMORY — storage, knowledge graph, retrieval](docs/MEMORY.md) ·
[CONTEXT — discipline, skills, personality, improvement](docs/CONTEXT.md) ·
[TOOLS — categorized native tool layer](docs/TOOLS.md)

**Inference & mobile:** [ENGINE — our LFM3-specialized engine](docs/ENGINE.md) ·
[NANO_RIPL — model pairs, residency, compute tiers](docs/NANO_RIPL.md) ·
[MOBILE_FIRST — deployment, budgets, de-risking](docs/MOBILE_FIRST.md)

**Process:** [ROADMAP — slices & exit criteria](docs/ROADMAP.md) ·
[DECISIONS — ADR log 001–036](docs/DECISIONS.md)

## Status

**S0 — Foundation + harness deep-dive** (see [ROADMAP](docs/ROADMAP.md)):
docs, workspace with `lola-memory` / `lola-harness` / `lola-infer` /
`lola-venue` / `lola-replay` / `lola-cli`, schema (0001+0002), omni-pair
config. No engine code yet.

## Quickstart

```bash
cargo run -p lola-cli -- doctor        # validate config + migrations on a scratch DB
cargo run -p lola-cli -- migrate       # apply migrations to data/lola.db
cargo test --all
```

## Scope guardrails (v0.1)

Hyperliquid only (testnet). Paper → testnet execution; mainnet arrives via an
evidence-gated leverage ladder. Model **pairs per profile** (ADR-017):
laptop = omni pair (d1-Omni-2B + LFM3-Omni-2B), constrained = text pair —
same two slots. Tier-1 compute only (Rust CPU + Vulkan/wgpu). Key
management and UI/UX are explicitly carved out to separate discussions —
see [docs/KEY_MANAGEMENT.md](docs/KEY_MANAGEMENT.md) and
[docs/UI_UX.md](docs/UI_UX.md).
