//! LOLA memory — the crown jewel (`docs/MEMORY.md`).
//!
//! Hybrid semantics (ADR-009): canonical facts are upserted; decisions,
//! outcomes, orders and fills are append-only and immutable; graph edges are
//! validity-versioned; bulky payloads compact to content-addressed blobs on
//! TTL. Nothing that made a decision is ever silently lost.
//!
//! Layout:
//! - [`persistence`] — WAL open + embedded, versioned migrations
//! - [`config`]      — `config/memory.yaml` (profiles, pairs, TTL)
//! - [`graph`]       — knowledge-graph node/edge types and traversal bounds
//! - [`retrieval`]   — the five-step pipeline and its budgets
//! - [`compaction`]  — archive-with-witnesses types (Hermes pattern)
//! - [`casebank`]    — outcome-weighted decision cases (ADR-031)
//! - [`blobs`]       — content-addressed payload store path scheme

pub mod blobs;
pub mod casebank;
pub mod compaction;
pub mod config;
pub mod error;
pub mod graph;
pub mod persistence;
pub mod retrieval;

pub use error::{LolaMemoryError, Result};
