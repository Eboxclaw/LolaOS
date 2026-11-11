# LOLA OS

A decision-first, edge-first agentic operating system for trading. One Rust
process, no sidecars, mobile constraints respected from day one.

> Don't solve the same problem twice. Retrieve what is known, decide
> deterministically when possible, spend model inference only where it
> creates value, record everything, improve from outcomes.

- [RFC-002 — Architecture](docs/RFC-002.md)
- [HARNESS — The OS kernel](docs/HARNESS.md)
- [MEMORY — Storage, knowledge graph, retrieval](docs/MEMORY.md)
- [NANO_RIPL — Inference layer](docs/NANO_RIPL.md)
- [ROADMAP — Pizza slices & exit criteria](docs/ROADMAP.md)
- [DECISIONS — ADR log](docs/DECISIONS.md)

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
