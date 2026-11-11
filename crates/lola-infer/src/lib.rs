//! LOLA nano-RIPL — the inference layer.
//!
//! A nano-scale take on Liquid AI's RIPL economics for edge devices
//! (`docs/NANO_RIPL.md`): two ~2B models at 4-bit (~1 GB each), fast
//! in-process swaps, and a strict output contract. Three rules govern it:
//!
//! 1. The kernel only ever sees [`infer::Infer`] implementations — the
//!    reference backend (S2 oracle) and the custom LFM3-optimized engine
//!    (S3) are interchangeable behind the trait.
//! 2. The custom engine is validated kernel-by-kernel against the oracle's
//!    golden outputs before it may serve anything.
//! 3. No model is ever on the candle-critical path by default; escalation is
//!    a measured decision, not a habit.

pub mod config;
pub mod engine;
pub mod infer;
pub mod residency;
