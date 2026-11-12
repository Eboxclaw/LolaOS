//! Deterministic strategy stubs (S1). `momentum-v1` is the A0 baseline:
//! EMA-alignment entry with a volume confirmation, symmetric exit.
//! No model in the path — this is what A0 is measured against.

use crate::contracts::{CandidateSignal, FeatureVector, Side};

pub const MOMENTUM_V1: &str = "momentum-v1";

/// Entry volume-confirmation threshold.
pub const ENTRY_VOLUME_RATIO: f64 = 1.2;

/// Momentum-v1 candidate from a computed feature state.
pub fn momentum_candidate(features: &FeatureVector, has_position: bool) -> Option<CandidateSignal> {
    use crate::contracts::{LiquidityState, TrendState, VolatilityState};
    let tradable_state = matches!(
        (features.trend_state, features.volatility_state, features.liquidity_state),
        (TrendState::Positive | TrendState::Neutral, _, LiquidityState::Healthy)
            if features.volatility_state != VolatilityState::Expanding
    );

    if !has_position && tradable_state && features.volume_ratio >= ENTRY_VOLUME_RATIO {
        Some(CandidateSignal {
            strategy_id: MOMENTUM_V1.into(),
            strategy_version: 1,
            action: Side::Buy,
        })
    } else if has_position && features.trend_state == TrendState::Negative {
        // `Negative` already encodes ema_fast < ema_slow AND return_5 < 0
        // (see features::compute), so no extra EMA test is needed here.
        Some(CandidateSignal {
            strategy_id: MOMENTUM_V1.into(),
            strategy_version: 1,
            action: Side::Sell,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::{compute, Bar};

    fn bars(direction: f64, last_volume: f64) -> Vec<Bar> {
        (0..40)
            .map(|i| {
                let close = 100.0 + direction * (i as f64).max(0.0);
                Bar {
                    open: close,
                    high: close + 0.1,
                    low: close - 0.1,
                    close,
                    volume: if i == 39 { last_volume } else { 1000.0 },
                    close_time_ns: i as i64 * 300_000_000_000,
                }
            })
            .collect()
    }

    #[test]
    fn rising_volume_confirmed_state_produces_a_buy_candidate() {
        let features = compute(&bars(1.0, 1500.0), 2.0).unwrap();
        let candidate = momentum_candidate(&features, false).expect("buy candidate");
        assert_eq!(candidate.action, Side::Buy);
        assert_eq!(candidate.strategy_id, MOMENTUM_V1);
    }

    #[test]
    fn thin_liquidity_blocks_entry() {
        let features = compute(&bars(1.0, 1500.0), 25.0).unwrap(); // wide spread ⇒ thin
        assert!(momentum_candidate(&features, false).is_none());
    }

    #[test]
    fn no_entry_without_volume_confirmation() {
        let features = compute(&bars(1.0, 1000.0), 2.0).unwrap(); // ratio 1.0 < 1.2
        assert!(momentum_candidate(&features, false).is_none());
    }

    #[test]
    fn falling_trend_with_position_produces_a_sell_candidate() {
        let features = compute(&bars(-1.0, 1000.0), 2.0).unwrap();
        assert_eq!(features.trend_state, crate::contracts::TrendState::Negative);
        let candidate = momentum_candidate(&features, true).expect("sell candidate");
        assert_eq!(candidate.action, Side::Sell);
    }
}
