//! LOLA kernel.
//!
//! The OS metaphor, made literal:
//! - [`contracts`]  — the syscalls: versioned, model-agnostic types crossing
//!   every boundary (market state → decision → order intent).
//! - [`events`]     — the scheduler's event stream.
//! - [`persistence`]— the memory: SQLite WAL, single writer, versioned
//!   migrations, append-only ledger (see `docs/MEMORY.md`).
//! - [`risk`]       — the guard: deterministic, fail-closed, independent of
//!   both models.
//! - [`config`]     — kernel configuration types.
//! - [`features`]   — feature-engine types and availability rules.

pub mod config;
pub mod contracts;
pub mod error;
pub mod events;
pub mod features;
pub mod persistence;
pub mod risk;

pub use error::{LolaError, Result};
