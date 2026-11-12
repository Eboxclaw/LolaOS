//! LOLA harness — the OS kernel (`docs/HARNESS.md`).
//!
//! Pi's loop discipline, Hermes' memory laws, llama.cpp's KV mechanics —
//! smaller. Subsystems:
//! - [`contracts`] — the syscalls: versioned, model-agnostic types
//! - [`events`]    — the scheduler's event stream
//! - [`session`]   — durable session event tree (Pi model, ADR-021)
//! - [`tools`]     — tool registry + escalation policy types
//! - [`lifecycle`] — Android-first loop lifecycle (ADR-025)
//! - [`skills`]    — Agent Skills standard adoption (ADR-028)
//! - [`persona`]   — prompt-as-ledger personality (ADR-029)
//! - [`router`]    — pass-forward reference tables (ADR-036)
//! - [`risk`]      — ring 0: deterministic, fail-closed (S1: full evaluate)
//! - [`strategy`]  — deterministic strategy stubs (A0 baseline)
//! - [`config`]    — `config/risk.yaml` + `config/tools.yaml`
//! - [`features`]  — feature engine (S1: computation from closed bars)

pub mod config;
pub mod contracts;
pub mod decision_loop;
pub mod error;
pub mod events;
pub mod features;
pub mod lifecycle;
pub mod metrics;
pub mod persona;
pub mod risk;
pub mod router;
pub mod session;
pub mod skills;
pub mod strategy;
pub mod tools;

pub use error::{LolaError, Result};
