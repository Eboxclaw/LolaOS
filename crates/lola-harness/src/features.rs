//! Feature engine — deterministic computation from closed bars (S1).
//!
//! Every feature is computable from bars that CLOSED at or before
//! `event_time_ns` (`availability_rule`: closed_bars_only). Identical
//! inputs produce identical outputs — the S7 lookahead audit and the
//! decision cache both depend on it.

use crate::contracts::FEATURE_SCHEMA_VERSION;
use crate::contracts::{FeatureVector, LiquidityState, TrendState, VolatilityState};

pub fn schema_version() -> &'static str {
    FEATURE_SCHEMA_VERSION
}

/// The single availability rule for every feature in every schema version:
/// computable from bars that CLOSED at or before `event_time_ns`. The S7
/// lookahead audit enforces it mechanically.
pub fn availability_rule() -> &'static str {
    "closed_bars_only"
}

/// Minimum bars required (rolling windows up to 26).
pub const MIN_BARS: usize = 26;

/// One completed OHLCV bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    /// Close time of this bar (inclusive boundary).
    pub close_time_ns: i64,
}

/// Deterministic feature vector from closed bars + current spread.
pub fn compute(bars: &[Bar], spread_bps: f64) -> Option<FeatureVector> {
    if bars.len() < MIN_BARS {
        return None;
    }
    let last = bars.last()?.close;
    let at = |n: usize| bars[bars.len() - n].close;
    // n-bar return: last close over the close n bars earlier.
    let ret = |n: usize| at(1) / at(n + 1) - 1.0;

    let return_1 = ret(1);
    let return_5 = ret(5);
    let return_20 = ret(20);

    // EMA trend: 12 vs 26 over the whole provided history.
    let ema = |period: usize| {
        let k = 2.0 / (period as f64 + 1.0);
        let mut e = bars[0].close;
        for bar in &bars[1..] {
            e = bar.close * k + e * (1.0 - k);
        }
        e
    };
    let (ema_fast, ema_slow) = (ema(12), ema(26));
    let slope_up = ema_fast > ema_slow && return_5 > 0.0;
    let trend_state = if slope_up {
        TrendState::Positive
    } else if ema_fast < ema_slow && return_5 < 0.0 {
        TrendState::Negative
    } else {
        TrendState::Neutral
    };

    // ATR% over the last 14 bars (true range vs previous close).
    let start = bars.len() - 14;
    let mut tr_sum = 0.0;
    for i in start..bars.len() {
        let prev_close = bars[i - 1].close;
        let bar = &bars[i];
        let tr = (bar.high - bar.low)
            .max((bar.high - prev_close).abs())
            .max((bar.low - prev_close).abs());
        tr_sum += tr;
    }
    let atr_pct = tr_sum / 14.0 / last;

    // Volatility expansion: mean bar-range rising over the last 3 vs 6 bars.
    let mean_range = |n: usize| {
        let slice = &bars[bars.len() - n..];
        slice.iter().map(|b| b.high - b.low).sum::<f64>() / n as f64
    };
    let expanding = mean_range(3) > mean_range(6) * 1.15;
    let volatility_state = if expanding {
        VolatilityState::Expanding
    } else if atr_pct < 0.01 {
        VolatilityState::Low
    } else if atr_pct < 0.025 {
        VolatilityState::Normal
    } else {
        VolatilityState::High
    };

    // Volume ratio vs its 20-bar mean.
    let vol_mean: f64 = bars[bars.len() - 20..]
        .iter()
        .map(|b| b.volume)
        .sum::<f64>()
        / 20.0;
    let volume_ratio = bars.last()?.volume / vol_mean;

    let liquidity_state = if spread_bps > 15.0 || volume_ratio < 0.25 {
        LiquidityState::Thin
    } else {
        LiquidityState::Healthy
    };

    Some(FeatureVector {
        return_1,
        return_5,
        return_20,
        atr_pct,
        volume_ratio,
        spread_bps,
        trend_state,
        volatility_state,
        liquidity_state,
    })
}

/// Coarse bucket signature used for retrieval narrowing (ADR-011).
/// Deterministic: same feature buckets ⇒ same signature.
pub fn state_signature(timeframe: &str, features: &FeatureVector, strategy_id: &str) -> String {
    format!(
        "{}|{:?}|{:?}|{:?}|{}",
        timeframe,
        features.trend_state,
        features.volatility_state,
        features.liquidity_state,
        strategy_id
    )
}

/// Content hash of the computed features for the decision cache.
/// FNV-1a over the canonical decimal form — deterministic and cheap; the
/// ledger's payload hashes use sha256 in lola-memory.
pub fn snapshot_hash(features: &FeatureVector) -> String {
    let text = format!(
        "{:.8e}|{:.8e}|{:.8e}|{:.8e}|{:.8e}|{:.8e}|{:?}|{:?}|{:?}",
        features.return_1,
        features.return_5,
        features.return_20,
        features.atr_pct,
        features.volume_ratio,
        features.spread_bps,
        features.trend_state,
        features.volatility_state,
        features.liquidity_state,
    );
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Compact numeric encoding for retrieval (SP1, ADR-041): the 6 floats
/// plus enum states as small ints. Order is fixed: r1, r5, r20, atr, vr,
/// spread, trend, vol, liq. This is the `feature_vectors` row shape.
pub fn to_vector9(f: &FeatureVector) -> [f32; 9] {
    let trend = match f.trend_state {
        TrendState::Negative => 0.0,
        TrendState::Neutral => 1.0,
        TrendState::Positive => 2.0,
    };
    let vol = match f.volatility_state {
        VolatilityState::Low => 0.0,
        VolatilityState::Normal => 1.0,
        VolatilityState::High => 2.0,
        VolatilityState::Expanding => 3.0,
    };
    let liq = match f.liquidity_state {
        LiquidityState::Stale => 0.0,
        LiquidityState::Thin => 1.0,
        LiquidityState::Healthy => 2.0,
    };
    // FeatureVector carries f64; retrieval vectors are f32 (halves the
    // row and doubles the scan rate — precision beyond f32 is noise here).
    [
        f.return_1 as f32,
        f.return_5 as f32,
        f.return_20 as f32,
        f.atr_pct as f32,
        f.volume_ratio as f32,
        f.spread_bps as f32,
        trend,
        vol,
        liq,
    ]
}

/// Reference to a persisted feature snapshot (row in `feature_snapshots`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureSnapshotRef {
    pub snapshot_id: String,
    pub instrument_id: String,
    pub timeframe: String,
    pub event_time_ns: i64,
    pub state_signature: String,
    pub snapshot_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 40 bars: flat at 100 for 30 bars, then rising 1/bar to 110.
    /// Volume flat at 1000, last bar 1500. Spread 2 bps.
    pub(crate) fn fixture_bars() -> Vec<Bar> {
        (0..40)
            .map(|i| {
                let close = if i < 30 {
                    100.0
                } else {
                    100.0 + (i - 29) as f64
                };
                Bar {
                    open: close - 0.5,
                    high: close + 0.5,
                    low: close - 0.5,
                    volume: if i == 39 { 1500.0 } else { 1000.0 },
                    close,
                    close_time_ns: 1_700_000_000_000_000_000 + (i as i64) * 300_000_000_000,
                }
            })
            .collect()
    }

    #[test]
    fn schema_version_is_pinned() {
        assert_eq!(schema_version(), "market-state-v1");
        assert_eq!(availability_rule(), "closed_bars_only");
    }

    #[test]
    fn computes_expected_features_on_the_fixture() {
        let features = compute(&fixture_bars(), 2.0).expect("enough bars");
        assert!((features.return_1 - 110.0 / 109.0 + 1.0).abs() < 1e-12);
        assert!((features.return_5 - 110.0 / 105.0 + 1.0).abs() < 1e-12);
        assert!((features.return_20 - 0.10).abs() < 1e-9, "20-bar return");
        // Last 20 volumes: 19×1000 + 1500 ⇒ mean 1025 ⇒ ratio 1500/1025.
        assert!((features.volume_ratio - 1500.0 / 1025.0).abs() < 1e-9);
        assert_eq!(features.trend_state, TrendState::Positive);
        // ATR ≈ 1.5/bar over a ~110 close ⇒ ~1.3–1.5% ⇒ normal band.
        assert!(
            (0.005..0.025).contains(&features.atr_pct),
            "atr_pct={}",
            features.atr_pct
        );
        assert_eq!(features.liquidity_state, LiquidityState::Healthy);
    }

    #[test]
    fn computation_is_deterministic() {
        let a = compute(&fixture_bars(), 2.0);
        let b = compute(&fixture_bars(), 2.0);
        assert_eq!(a, b);
        assert_eq!(snapshot_hash(&a.unwrap()), snapshot_hash(&b.unwrap()));
    }

    #[test]
    fn insufficient_history_returns_none() {
        assert!(compute(&fixture_bars()[..25], 2.0).is_none());
    }

    #[test]
    fn vector9_encoding_is_stable_and_total() {
        let features = compute(&fixture_bars(), 2.0).unwrap();
        let v = to_vector9(&features);
        assert_eq!(v.len(), 9);
        assert!((v[0] as f64 - features.return_1).abs() < 1e-6);
        assert_eq!(v[6], 2.0, "Positive trend encodes as 2");
        assert!(v[7] >= 0.0 && v[7] <= 3.0, "vol in 0..=3");
        // Every enum combination encodes without panic.
        let all = FeatureVector {
            return_1: 0.0,
            return_5: 0.0,
            return_20: 0.0,
            atr_pct: 0.0,
            volume_ratio: 0.0,
            spread_bps: 0.0,
            trend_state: TrendState::Negative,
            volatility_state: VolatilityState::Expanding,
            liquidity_state: LiquidityState::Stale,
        };
        let encoded = to_vector9(&all);
        assert_eq!((encoded[6], encoded[7], encoded[8]), (0.0, 3.0, 0.0));
    }

    #[test]
    fn signatures_are_stable_and_bucketed() {
        let features = compute(&fixture_bars(), 2.0).unwrap();
        let sig = state_signature("5m", &features, "momentum-v1");
        assert_eq!(sig, "5m|Positive|Normal|Healthy|momentum-v1");
        // A different strategy id changes the signature (cache isolation).
        assert_ne!(sig, state_signature("5m", &features, "mean-revert-v1"));
    }
}
