# LOLA OS

A decision-first, edge-first agentic operating system for trading. One Rust
process, no sidecars, mobile constraints respected from day one.

> Don't solve the same problem twice. Retrieve what is known, decide
> deterministically when possible, spend model inference only where it
> creates value, record everything, improve from outcomes.

- [RFC-002 — Architecture](docs/RFC-002.md)
- [MEMORY — Storage, knowledge graph, retrieval](docs/MEMORY.md)
- [NANO_RIPL — Inference layer](docs/NANO_RIPL.md)
- [ROADMAP — Pizza slices & exit criteria](docs/ROADMAP.md)
- [DECISIONS — ADR log](docs/DECISIONS.md)

## Status

**S0 — Foundation** (see [ROADMAP](docs/ROADMAP.md)). Docs + workspace
scaffold only; no business logic yet.

## Quickstart

```bash
cargo run -p lola-cli -- doctor        # validate config + migrations on a scratch DB
cargo run -p lola-cli -- migrate       # apply migrations to data/lola.db
cargo test --all
```

## Scope guardrails (v0.1)

Hyperliquid only (testnet). Paper → testnet execution; mainnet arrives via an
evidence-gated leverage ladder. d1-2B (decision) + LFM3-1.7B (agent) at 4-bit.
Key management and UI/UX are explicitly carved out to separate discussions —
see [docs/KEY_MANAGEMENT.md](docs/KEY_MANAGEMENT.md) and
[docs/UI_UX.md](docs/UI_UX.md).
