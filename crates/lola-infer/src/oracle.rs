//! Oracle conformance harness (S2.a, ADR-003/023).
//!
//! Everything the real oracle (llama.cpp ≥ b11960, linked in-process at
//! S2.b) will plug into — built and tested *without* weights:
//! - [`GoldenSet`]: versioned prompt→answer recordings with provenance.
//! - [`run_conformance`]: tolerance-checked replay of a backend against
//!   the set, over the existing [`crate::infer::Infer`] trait.
//! - [`GoldenOracle`]: a deterministic double that replays the set —
//!   proves the pipeline, doubles as the S3 parity baseline shape.
//!
//! The shipped `evals/golden/decision-contract-v1.json` is hand-authored
//! placeholder data until S2.c records real oracle outputs; its
//! `provenance` field says so explicitly, and `verify` refuses sets whose
//! provenance is missing.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::infer::{DecisionRequest, Infer, InferError};
use lola_harness::contracts::{AssessmentLabel, DecisionV1, MarketRegime};

/// Probability comparison tolerance for golden answers.
pub const PROBABILITY_TOLERANCE: f64 = 0.02;

/// One recorded case: the request and the answer the oracle produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldenCase {
    pub id: String,
    pub request: DecisionRequest,
    pub expected: DecisionV1,
}

/// Where a golden set's answers came from. `"oracle-llamacpp@<rev>"` is
/// the only provenance S3 kernels may be validated against; the shipped
/// placeholder is hand-authored and marked as such.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldenSet {
    pub set_id: String,
    pub schema_version: u32,
    /// e.g. `"hand-authored-placeholder"` or `"oracle-llamacpp@b11960"`.
    pub provenance: String,
    pub cases: Vec<GoldenCase>,
}

impl GoldenSet {
    pub fn load(path: &Path) -> Result<Self, GoldenError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json(&text)
    }

    /// Parse from an in-memory JSON document (tests embed sets via
    /// include_str! — no temp files, no races).
    pub fn from_json(text: &str) -> Result<Self, GoldenError> {
        let set: GoldenSet = serde_json::from_str(text)?;
        set.validate()?;
        Ok(set)
    }

    pub fn validate(&self) -> Result<(), GoldenError> {
        if self.provenance.trim().is_empty() {
            return Err(GoldenError::Invalid("golden set lacks provenance".into()));
        }
        if self.cases.is_empty() {
            return Err(GoldenError::Invalid("golden set is empty".into()));
        }
        let mut ids: Vec<&str> = self.cases.iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() != self.cases.len() {
            return Err(GoldenError::Invalid("duplicate case ids".into()));
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GoldenError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid golden set: {0}")]
    Invalid(String),
}

/// Why one case failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseFailure {
    InferError(String),
    RegimeMismatch {
        expected: MarketRegime,
        got: MarketRegime,
    },
    AssessmentMismatch {
        expected: AssessmentLabel,
        got: AssessmentLabel,
    },
    ProbabilityDrift {
        answer: String,
        expected: f64,
        got: f64,
        tolerance: f64,
    },
}

/// Full conformance report for one set replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceReport {
    pub set_id: String,
    pub provenance: String,
    pub total: usize,
    pub passed: usize,
    pub failures: Vec<(String, CaseFailure)>,
}

impl ConformanceReport {
    pub fn all_passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Replay `backend` against `set`. A case passes when the argmax regime,
/// the candidate assessment and every named probability match within
/// [`PROBABILITY_TOLERANCE`]; any backend error fails the case.
pub fn run_conformance(backend: &dyn Infer, set: &GoldenSet) -> ConformanceReport {
    let mut failures = Vec::new();
    let mut passed = 0usize;

    for case in &set.cases {
        let failure = match backend.decide(&case.request) {
            Err(error) => Some(CaseFailure::InferError(error.to_string())),
            Ok(got) => compare(&case.expected, &got),
        };
        match failure {
            None => passed += 1,
            Some(failure) => failures.push((case.id.clone(), failure)),
        }
    }

    ConformanceReport {
        set_id: set.set_id.clone(),
        provenance: set.provenance.clone(),
        total: set.cases.len(),
        passed,
        failures,
    }
}

fn compare(expected: &DecisionV1, got: &DecisionV1) -> Option<CaseFailure> {
    let expected_regime = expected
        .market_regime
        .first()
        .map(|p| p.label)
        .unwrap_or(MarketRegime::Range);
    let got_regime = got
        .market_regime
        .first()
        .map(|p| p.label)
        .unwrap_or(MarketRegime::Range);
    if expected_regime != got_regime {
        return Some(CaseFailure::RegimeMismatch {
            expected: expected_regime,
            got: got_regime,
        });
    }
    if expected.candidate_assessment.label != got.candidate_assessment.label {
        return Some(CaseFailure::AssessmentMismatch {
            expected: expected.candidate_assessment.label,
            got: got.candidate_assessment.label,
        });
    }
    if (expected.candidate_assessment.probability - got.candidate_assessment.probability).abs()
        > PROBABILITY_TOLERANCE
    {
        return Some(CaseFailure::ProbabilityDrift {
            answer: "candidate_assessment".into(),
            expected: expected.candidate_assessment.probability,
            got: got.candidate_assessment.probability,
            tolerance: PROBABILITY_TOLERANCE,
        });
    }
    let expected_probs: BTreeMap<_, _> = expected
        .market_regime
        .iter()
        .map(|p| (p.label, p.probability))
        .collect();
    for prob in &got.market_regime {
        if let Some(&expected_p) = expected_probs.get(&prob.label) {
            if (expected_p - prob.probability).abs() > PROBABILITY_TOLERANCE {
                return Some(CaseFailure::ProbabilityDrift {
                    answer: format!("regime_{:?}", prob.label),
                    expected: expected_p,
                    got: prob.probability,
                    tolerance: PROBABILITY_TOLERANCE,
                });
            }
        }
    }
    None
}

/// A deterministic double that replays a golden set through the
/// [`Infer`] trait — used to validate the conformance pipeline itself and
/// as the shape of the S2.b real backend.
pub struct GoldenOracle {
    answers: BTreeMap<String, DecisionV1>,
}

impl GoldenOracle {
    pub fn from_set(set: &GoldenSet) -> Self {
        // Keyed by the request's snapshot hash + first question name —
        // deterministic and stable across replays.
        Self {
            answers: set
                .cases
                .iter()
                .map(|c| (case_key(&c.request), c.expected.clone()))
                .collect(),
        }
    }
}

impl Infer for GoldenOracle {
    fn model_id(&self) -> &str {
        "golden-oracle"
    }

    fn decide(&self, request: &DecisionRequest) -> Result<DecisionV1, InferError> {
        self.answers
            .get(&case_key(request))
            .cloned()
            .ok_or_else(|| {
                InferError::Unavailable(format!(
                    "no golden answer for snapshot {}",
                    request.state.snapshot_hash
                ))
            })
    }

    fn generate(
        &self,
        _request: &crate::infer::GenerateRequest,
    ) -> Result<crate::infer::GenerateResponse, InferError> {
        Err(InferError::Unavailable(
            "the golden double serves decisions only".into(),
        ))
    }
}

fn case_key(request: &DecisionRequest) -> String {
    format!(
        "{}|{}",
        request.state.snapshot_hash,
        request
            .questions
            .first()
            .map(|q| q.name.clone())
            .unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infer::{Question, QuestionKind};
    use lola_harness::contracts::{
        CandidateSignal, FeatureVector, LabelProbability, LiquidityState, MarketStateV1,
        PortfolioState, RiskState, Side, SufficiencyLabel, TrendState, VolatilityState,
    };

    fn state(hash: &str) -> MarketStateV1 {
        MarketStateV1 {
            contract_version: "1.0".into(),
            instrument_id: "HYPERLIQUID:BTC-PERP".into(),
            timeframe: "5m".into(),
            event_time_ns: 1,
            ingestion_time_ns: 2,
            feature_schema_version: "market-state-v1".into(),
            state_signature: "5m|Positive|Normal|Healthy|momentum-v1".into(),
            snapshot_hash: hash.into(),
            features: FeatureVector {
                return_1: 0.001,
                return_5: 0.004,
                return_20: 0.01,
                atr_pct: 0.018,
                volume_ratio: 1.3,
                spread_bps: 2.0,
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

    fn request(hash: &str) -> DecisionRequest {
        DecisionRequest {
            state: state(hash),
            questions: vec![Question {
                name: "market_regime".into(),
                kind: QuestionKind::Regime,
            }],
        }
    }

    fn decision(prob: f64) -> DecisionV1 {
        DecisionV1 {
            contract_version: "1.0".into(),
            decision_id: "gd-1".into(),
            model_id: "d1-omni-2b".into(),
            policy_version: "risk-1".into(),
            market_regime: vec![
                LabelProbability {
                    label: MarketRegime::UpTrend,
                    probability: prob,
                },
                LabelProbability {
                    label: MarketRegime::Range,
                    probability: 1.0 - prob,
                },
            ],
            candidate_assessment: LabelProbability {
                label: AssessmentLabel::AcceptForRiskCheck,
                probability: 0.7,
            },
            data_sufficiency: LabelProbability {
                label: SufficiencyLabel::Sufficient,
                probability: 0.95,
            },
            risk_flags: vec![],
            reason_codes: vec!["TREND_ALIGNED".into()],
        }
    }

    fn make_set(expected: DecisionV1) -> GoldenSet {
        GoldenSet {
            set_id: "test-set".into(),
            schema_version: 1,
            provenance: "hand-authored-placeholder".into(),
            cases: vec![GoldenCase {
                id: "case-1".into(),
                request: request("hash-1"),
                expected,
            }],
        }
    }

    #[test]
    fn golden_double_passes_against_itself() {
        let set = make_set(decision(0.78));
        let report = run_conformance(&GoldenOracle::from_set(&set), &set);
        assert!(report.all_passed());
        assert_eq!(report.passed, 1);
    }

    #[test]
    fn probability_drift_beyond_tolerance_fails() {
        let set = make_set(decision(0.78));
        let mut drifted = set.clone();
        drifted.cases[0].expected = decision(0.90); // 0.12 drift ≫ 0.02
        let oracle = GoldenOracle::from_set(&set);
        let report = run_conformance(&oracle, &drifted);
        assert!(!report.all_passed());
        assert!(matches!(
            report.failures[0].1,
            CaseFailure::ProbabilityDrift { .. }
        ));
    }

    #[test]
    fn assessment_flip_fails() {
        let set = make_set(decision(0.78));
        let mut flipped = make_set(decision(0.78));
        flipped.cases[0].expected.candidate_assessment.label = AssessmentLabel::Reject;
        let oracle = GoldenOracle::from_set(&set);
        let report = run_conformance(&oracle, &flipped);
        assert!(matches!(
            report.failures[0].1,
            CaseFailure::AssessmentMismatch { .. }
        ));
    }

    #[test]
    fn unknown_request_fails_the_case_not_the_run() {
        let set = make_set(decision(0.78));
        let mut stranger = set.clone();
        stranger.cases[0].request = request("unknown-hash");
        let oracle = GoldenOracle::from_set(&set);
        let report = run_conformance(&oracle, &stranger);
        assert_eq!(report.total, 1);
        assert!(!report.all_passed());
    }

    #[test]
    fn sets_require_provenance() {
        let mut bad = make_set(decision(0.78));
        bad.provenance = "  ".into();
        assert!(bad.validate().is_err());
    }
}
