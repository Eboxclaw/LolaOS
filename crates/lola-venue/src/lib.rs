//! Venue abstraction (ADR-005).
//!
//! v0.1 ships only the instrument model and the trait surface. The
//! Hyperliquid adapter lands in S6 on top of the official
//! `hyperliquid-rust-sdk`; a Nado (Inchain) adapter drops in later behind
//! this same trait. The full streaming/placement surface (async market-data
//! streams, order submission, reconciliation) is deliberately not designed
//! here — it gets shaped by what S6 actually needs, not by speculation.

pub mod config;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstrumentClass {
    Spot,
    Perp,
}

/// Everything the risk engine needs to validate an instrument, in kernel
/// terms — never venue-native structures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instrument {
    /// Canonical form: `VENUE:NATIVE_SYMBOL` (e.g. `HYPERLIQUID:BTC-PERP`).
    pub instrument_id: String,
    pub venue_id: String,
    pub native_symbol: String,
    pub instrument_class: InstrumentClass,
    pub base_currency: Option<String>,
    pub quote_currency: String,
    /// Canonical decimal strings; precision is venue-native.
    pub price_increment: String,
    pub quantity_increment: String,
    pub min_order_notional: Option<String>,
    /// Venue maximum; the risk engine may clamp below it, never above.
    pub max_leverage: Option<u32>,
}

/// Minimal trait surface for S0. Extended (not rewritten) in S6/S8.
pub trait Venue {
    fn venue_id(&self) -> &'static str;

    fn instrument(&self, native_symbol: &str) -> Option<Instrument>;
}
