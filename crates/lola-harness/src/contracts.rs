//! Canonical, versioned contracts exchanged between kernel components.
//!
//! These types are the stable boundary between the market-data side, the
//! inference layer, the risk engine and the execution path. They are
//! intentionally model-agnostic: no field may leak a specific backend's raw
//! response format. Money, prices and quantities always travel as canonical
//! decimal strings — never floats.

use serde::{Deserialize, Serialize};

pub const CONTRACT_VERSION: &str = "1.0";
pub const FEATURE_SCHEMA_VERSION: &str = "market-state-v1";

/// Input modality of a model or media payload (ADR-019: omni-native
/// harness). `Omni` marks a model that accepts every modality; media
/// entries never use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Audio,
    Image,
    Video,
    Omni,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketRegime {
    UpTrend,
    Range,
    DownTrend,
    HighVolatility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrendState {
    Positive,
    Neutral,
    Negative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VolatilityState {
    Low,
    Normal,
    High,
    Expanding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LiquidityState {
    Healthy,
    Thin,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskState {
    Normal,
    Degraded,
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentLabel {
    AcceptForRiskCheck,
    Reject,
    Abstain,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SufficiencyLabel {
    Sufficient,
    Insufficient,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskFlag {
    VolatilityExpansion,
    LiquidityThin,
    DataStale,
    SpreadWide,
    ConflictingHistory,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderType {
    Market,
    Limit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TimeInForce {
    Gtc,
    Ioc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionMode {
    Paper,
    Testnet,
    Mainnet,
}

/// Deterministic feature vector computed from closed bars only.
/// Every field has a documented definition, unit and availability rule
/// (`docs/RFC-002.md` §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureVector {
    pub return_1: f64,
    pub return_5: f64,
    pub return_20: f64,
    pub atr_pct: f64,
    pub volume_ratio: f64,
    pub spread_bps: f64,
    pub trend_state: TrendState,
    pub volatility_state: VolatilityState,
    pub liquidity_state: LiquidityState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSignal {
    pub strategy_id: String,
    pub strategy_version: u32,
    pub action: Side,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioState {
    pub has_position: bool,
    /// Canonical decimal string.
    pub cash_available: String,
    /// Canonical decimal string (notional exposure).
    pub exposure: String,
    pub risk_state: RiskState,
}

/// The input to every decision: deterministic state, not raw market streams.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketStateV1 {
    pub contract_version: String,
    pub instrument_id: String,
    pub timeframe: String,
    /// When the market event actually occurred.
    pub event_time_ns: i64,
    /// When LOLA received/stored it. Backtests must never conflate the two.
    pub ingestion_time_ns: i64,
    pub feature_schema_version: String,
    /// Coarse bucket signature used for retrieval narrowing (ADR-011).
    pub state_signature: String,
    /// sha256 of the canonical feature snapshot.
    pub snapshot_hash: String,
    pub features: FeatureVector,
    pub candidate: Option<CandidateSignal>,
    pub portfolio: PortfolioState,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LabelProbability<L> {
    pub label: L,
    pub probability: f64,
}

/// The output of the decision path, in LOLA's own schema — never a backend's
/// raw format. d1 probabilities are model probabilities, **not** probabilities
/// of profitable trades; that relationship is measured in S5 (shadow mode).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionV1 {
    pub contract_version: String,
    pub decision_id: String,
    pub model_id: String,
    pub policy_version: String,
    /// Ranked regime distribution; entry 0 is the argmax label.
    pub market_regime: Vec<LabelProbability<MarketRegime>>,
    pub candidate_assessment: LabelProbability<AssessmentLabel>,
    pub data_sufficiency: LabelProbability<SufficiencyLabel>,
    pub risk_flags: Vec<RiskFlag>,
    pub reason_codes: Vec<String>,
}

/// A proposed intent — not an authorised order. Nothing reaches a venue
/// (paper, testnet or mainnet) without the deterministic risk engine
/// approving it independently (`docs/RFC-002.md` §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderIntentV1 {
    pub contract_version: String,
    pub decision_id: String,
    pub instrument_id: String,
    pub action: Side,
    pub order_type: OrderType,
    /// Canonical decimal string.
    pub quantity: String,
    /// Canonical decimal string; `None` for market orders.
    pub limit_price: Option<String>,
    pub time_in_force: TimeInForce,
    /// Perps: can only reduce an existing position, never open/increase one.
    pub reduce_only: bool,
    /// Requested leverage; the risk engine may clamp it, never raise it.
    pub requested_leverage: Option<u32>,
    pub execution_mode: ExecutionMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> MarketStateV1 {
        MarketStateV1 {
            contract_version: CONTRACT_VERSION.into(),
            instrument_id: "HYPERLIQUID:BTC-PERP".into(),
            timeframe: "5m".into(),
            event_time_ns: 1_794_168_300_000_000_000,
            ingestion_time_ns: 1_794_168_300_100_000_000,
            feature_schema_version: FEATURE_SCHEMA_VERSION.into(),
            state_signature: "CRYPTO_PERP|5m|UpTrend|VolNormal|LiqHealthy|momentum-v1".into(),
            snapshot_hash: "deadbeef".into(),
            features: FeatureVector {
                return_1: 0.0012,
                return_5: 0.0041,
                return_20: 0.0102,
                atr_pct: 0.018,
                volume_ratio: 1.34,
                spread_bps: 1.8,
                trend_state: TrendState::Positive,
                volatility_state: VolatilityState::Normal,
                liquidity_state: LiquidityState::Healthy,
            },
            candidate: Some(CandidateSignal {
                strategy_id: "momentum-v1".into(),
                strategy_version: 1,
                action: Side::Buy,
            }),
            portfolio: PortfolioState {
                has_position: false,
                cash_available: "10000.00".into(),
                exposure: "0.00".into(),
                risk_state: RiskState::Normal,
            },
        }
    }

    #[test]
    fn market_state_roundtrips_through_json() {
        let state = sample_state();
        let json = serde_json::to_string(&state).expect("serialize");
        let back: MarketStateV1 = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, back);
    }

    #[test]
    fn serde_uses_screaming_snake_case() {
        let json = serde_json::to_string(&Side::Buy).expect("serialize");
        assert_eq!(json, "\"BUY\"");
    }
}
