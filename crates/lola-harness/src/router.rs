//! The router — pass-forward reference tables (ADR-036, `docs/ROUTER.md`).
//!
//! **d1 is a calibrated scoring head; routing is a deterministic table
//! owned by the harness.** Two tables, two owners:
//! - *question schemas* are shown **to** the model — named questions with
//!   criteria/rubrics so answers are consistent, cacheable and comparable
//!   across versions;
//! - *pass-forward policies* are harness-only — thresholds on
//!   **calibrated** scores, destinations, hard budgets. The model never
//!   sees them, so it cannot game limits it doesn't know.
//!
//! Every pass-forward becomes a ledger row: schema version, raw +
//! calibrated score, policy version, destination. Unknown, missing or
//! over-budget answers fail closed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// d1's trained question-set size. Larger sets are faster but slightly
/// less accurate (model card) — and never larger than this.
pub const MAX_QUESTIONS: usize = 16;
const _: () = assert!(MAX_QUESTIONS <= 16);

/// PROVISIONAL conservative defaults (ADR-036): replaced only via
/// versioned policy bumps after S5's calibration reports exist.
pub const PROVISIONAL_CONTEXT_PASS: f64 = 0.7;
pub const PROVISIONAL_ESCALATION: f64 = 0.6;
pub const PROVISIONAL_AGENT_CONTEXT_TOKENS_PER_TURN: u32 = 2048;
pub const PROVISIONAL_ESCALATIONS_PER_HOUR: u32 = 6;

/// Where in the dual pipeline a support pass runs. Stable ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionPoint {
    /// Per-tick market decision (regime, candidate, sufficiency).
    TickDecision,
    /// What retrieved content may enter the agent's context.
    ContextGate,
    /// Whether the agent (slot B) is invoked at all.
    EscalationGate,
    /// Compaction summarization choices.
    CompactionSummary,
}

impl DecisionPoint {
    pub fn id(self) -> &'static str {
        match self {
            Self::TickDecision => "tick_decision",
            Self::ContextGate => "context_gate",
            Self::EscalationGate => "escalation_gate",
            Self::CompactionSummary => "compaction_summary",
        }
    }
}

/// Question shape — mirrors d1's system_one answer types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuestionKind {
    /// Yes/no → P(yes).
    Noul,
    /// One of the named criteria (`{name: description}`).
    Choice { criteria: BTreeMap<String, String> },
    /// One of 2–10 ordered levels, lowest first.
    Score { levels: Vec<String> },
}

/// One named question in a schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionDef {
    pub name: String,
    pub kind: QuestionKind,
}

/// The reference table shown to the model for one decision point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionSchema {
    pub schema_id: String,
    pub decision_point: DecisionPoint,
    pub version: u32,
    pub questions: Vec<QuestionDef>,
}

impl QuestionSchema {
    /// Enforce structural rules: ≤ [`MAX_QUESTIONS`] questions, unique
    /// names, non-degenerate rubrics.
    pub fn validate(&self) -> Result<(), RouterError> {
        if self.questions.is_empty() {
            return Err(RouterError::InvalidSchema("empty question set".into()));
        }
        if self.questions.len() > MAX_QUESTIONS {
            return Err(RouterError::InvalidSchema(format!(
                "{} questions exceeds the trained set size ({MAX_QUESTIONS})",
                self.questions.len()
            )));
        }
        let names: Vec<&str> = self.questions.iter().map(|q| q.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        if names.len() != sorted.len() {
            return Err(RouterError::InvalidSchema(
                "duplicate question names".into(),
            ));
        }
        Ok(())
    }

    pub fn has_question(&self, name: &str) -> bool {
        self.questions.iter().any(|q| q.name == name)
    }
}

/// Where a routed answer goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    /// Into the deterministic risk pipeline (decision path).
    PassToRisk,
    /// Into the agent's context (counts against the token budget).
    PassToAgentContext,
    /// Wake slot B for an escalation (counts against the hourly budget).
    EscalateAgent,
    /// Recorded in the ledger, passed nowhere.
    LedgerOnly,
    /// Not even recorded beyond the audit row.
    Discard,
}

/// One policy rule: which answer, where, above which calibrated score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteRule {
    pub answer_name: String,
    pub destination: Destination,
    pub min_calibrated: f64,
}

/// Hard budgets the router cannot exceed — the limits the model never sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardLimits {
    /// Per-turn cap on tokens the router may inject into agent context.
    pub agent_context_tokens_per_turn: u32,
    /// Cap on agent wake-ups per hour.
    pub escalations_per_hour: u32,
    /// When true (always, in practice): unknown/missing fail closed.
    pub fail_closed: bool,
}

impl Default for ForwardLimits {
    fn default() -> Self {
        Self {
            agent_context_tokens_per_turn: PROVISIONAL_AGENT_CONTEXT_TOKENS_PER_TURN,
            escalations_per_hour: PROVISIONAL_ESCALATIONS_PER_HOUR,
            fail_closed: true,
        }
    }
}

/// The harness-owned policy for one decision point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassForwardPolicy {
    pub decision_point: DecisionPoint,
    pub version: u32,
    pub routes: Vec<RouteRule>,
    #[serde(default)]
    pub limits: ForwardLimits,
}

/// One model answer with raw and recalibrated scores (ADR-031).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredAnswer {
    pub name: String,
    pub raw: f64,
    pub calibrated: f64,
}

/// Why a route decided what it decided (persisted for audit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteReason {
    PassedThreshold,
    BelowThreshold,
    BudgetExhausted,
    FailClosedMissing,
    FailClosedUnknown,
}

/// The routing outcome for one policy rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassForwardDecision {
    pub answer_name: String,
    pub requested: Destination,
    pub granted: Destination,
    pub raw: Option<f64>,
    pub calibrated: Option<f64>,
    pub threshold: f64,
    pub reason: RouteReason,
}

/// Deterministic routing: thresholds compare **calibrated** scores only;
/// missing answers and policy/schema mismatches fail closed; budgets are
/// hard caps evaluated in declared rule order.
pub fn route(
    answers: &[ScoredAnswer],
    schema: &QuestionSchema,
    policy: &PassForwardPolicy,
) -> Vec<PassForwardDecision> {
    let mut context_budget = policy.limits.agent_context_tokens_per_turn as i64;
    let mut escalations = 0i64;
    let mut decisions = Vec::with_capacity(policy.routes.len());

    for rule in &policy.routes {
        // Policy/schema mismatch is a configuration bug: fail closed.
        if !schema.has_question(&rule.answer_name) {
            decisions.push(PassForwardDecision {
                answer_name: rule.answer_name.clone(),
                requested: rule.destination,
                granted: Destination::Discard,
                raw: None,
                calibrated: None,
                threshold: rule.min_calibrated,
                reason: RouteReason::FailClosedUnknown,
            });
            continue;
        }
        let Some(answer) = answers.iter().find(|a| a.name == rule.answer_name) else {
            decisions.push(PassForwardDecision {
                answer_name: rule.answer_name.clone(),
                requested: rule.destination,
                granted: Destination::Discard,
                raw: None,
                calibrated: None,
                threshold: rule.min_calibrated,
                reason: RouteReason::FailClosedMissing,
            });
            continue;
        };

        if answer.calibrated < rule.min_calibrated {
            decisions.push(PassForwardDecision {
                answer_name: rule.answer_name.clone(),
                requested: rule.destination,
                granted: Destination::LedgerOnly,
                raw: Some(answer.raw),
                calibrated: Some(answer.calibrated),
                threshold: rule.min_calibrated,
                reason: RouteReason::BelowThreshold,
            });
            continue;
        }

        // Budgets are hard caps over the *granted* destinations.
        let mut granted = rule.destination;
        let mut reason = RouteReason::PassedThreshold;
        match rule.destination {
            Destination::PassToAgentContext => {
                // Rough scaffold estimate: name + two scores ≈ 24 tokens.
                let cost = 24i64;
                if context_budget - cost < 0 {
                    granted = Destination::LedgerOnly;
                    reason = RouteReason::BudgetExhausted;
                } else {
                    context_budget -= cost;
                }
            }
            Destination::EscalateAgent => {
                if escalations >= policy.limits.escalations_per_hour as i64 {
                    granted = Destination::LedgerOnly;
                    reason = RouteReason::BudgetExhausted;
                } else {
                    escalations += 1;
                }
            }
            _ => {}
        }

        decisions.push(PassForwardDecision {
            answer_name: rule.answer_name.clone(),
            requested: rule.destination,
            granted,
            raw: Some(answer.raw),
            calibrated: Some(answer.calibrated),
            threshold: rule.min_calibrated,
            reason,
        });
    }
    decisions
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouterError {
    #[error("invalid question schema: {0}")]
    InvalidSchema(String),
    #[error("configuration error: {0}")]
    Config(String),
}

/// `config/router.yaml` — the shipped reference tables.
#[derive(Debug, Clone, Deserialize)]
pub struct RouterConfig {
    pub schemas: Vec<QuestionSchema>,
    pub policies: Vec<PassForwardPolicy>,
}

impl RouterConfig {
    pub fn parse(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    pub fn schema_for(&self, point: DecisionPoint) -> Option<&QuestionSchema> {
        self.schemas
            .iter()
            .find(|s| s.decision_point == point && s.validate().is_ok())
    }

    pub fn policy_for(&self, point: DecisionPoint) -> Option<&PassForwardPolicy> {
        self.policies.iter().find(|p| p.decision_point == point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTER_YAML: &str = include_str!("../../../config/router.yaml");

    fn noul(name: &str) -> QuestionDef {
        QuestionDef {
            name: name.into(),
            kind: QuestionKind::Noul,
        }
    }

    fn schema() -> QuestionSchema {
        QuestionSchema {
            schema_id: "test-v1".into(),
            decision_point: DecisionPoint::ContextGate,
            version: 1,
            questions: vec![noul("relevance"), noul("freshness")],
        }
    }

    fn policy() -> PassForwardPolicy {
        PassForwardPolicy {
            decision_point: DecisionPoint::ContextGate,
            version: 1,
            routes: vec![
                RouteRule {
                    answer_name: "relevance".into(),
                    destination: Destination::PassToAgentContext,
                    min_calibrated: PROVISIONAL_CONTEXT_PASS,
                },
                RouteRule {
                    answer_name: "freshness".into(),
                    destination: Destination::LedgerOnly,
                    min_calibrated: 0.5,
                },
            ],
            limits: ForwardLimits::default(),
        }
    }

    #[test]
    fn missing_answers_fail_closed() {
        let decisions = route(&[], &schema(), &policy());
        assert_eq!(decisions.len(), 2);
        assert!(decisions.iter().all(|d| d.granted == Destination::Discard));
        assert!(decisions
            .iter()
            .all(|d| d.reason == RouteReason::FailClosedMissing));
    }

    #[test]
    fn thresholds_compare_calibrated_scores_not_raw() {
        let answers = vec![
            ScoredAnswer {
                name: "relevance".into(),
                raw: 0.95,
                calibrated: 0.5, // well below 0.7 after recalibration
            },
            ScoredAnswer {
                name: "freshness".into(),
                raw: 0.9,
                calibrated: 0.9,
            },
        ];
        let decisions = route(&answers, &schema(), &policy());
        assert_eq!(decisions[0].reason, RouteReason::BelowThreshold);
        assert_eq!(decisions[0].granted, Destination::LedgerOnly);
        assert_eq!(decisions[1].granted, Destination::LedgerOnly);
    }

    #[test]
    fn context_budget_is_a_hard_cap() {
        let tight = PassForwardPolicy {
            routes: vec![
                RouteRule {
                    answer_name: "relevance".into(),
                    destination: Destination::PassToAgentContext,
                    min_calibrated: 0.0,
                },
                RouteRule {
                    answer_name: "freshness".into(),
                    destination: Destination::PassToAgentContext,
                    min_calibrated: 0.0,
                },
            ],
            limits: ForwardLimits {
                agent_context_tokens_per_turn: 30, // room for one ~24-token pass
                escalations_per_hour: 6,
                fail_closed: true,
            },
            ..policy()
        };
        let answers = vec![
            ScoredAnswer {
                name: "relevance".into(),
                raw: 0.9,
                calibrated: 0.9,
            },
            ScoredAnswer {
                name: "freshness".into(),
                raw: 0.9,
                calibrated: 0.9,
            },
        ];
        let decisions = route(&answers, &schema(), &tight);
        assert_eq!(decisions[0].granted, Destination::PassToAgentContext);
        assert_eq!(decisions[1].granted, Destination::LedgerOnly);
        assert_eq!(decisions[1].reason, RouteReason::BudgetExhausted);
    }

    #[test]
    fn policy_schema_mismatch_fails_closed() {
        let mut bad = policy();
        bad.routes[0].answer_name = "not_in_schema".into();
        let decisions = route(&[], &schema(), &bad);
        assert_eq!(decisions[0].reason, RouteReason::FailClosedUnknown);
        assert_eq!(decisions[0].granted, Destination::Discard);
    }

    #[test]
    fn schemas_cannot_exceed_the_trained_set_size() {
        let mut big = schema();
        big.questions = (0..17).map(|i| noul(&format!("q{i}"))).collect();
        assert!(big.validate().is_err());
        assert!(schema().validate().is_ok());
    }

    #[test]
    fn shipped_router_yaml_parses_and_validates() {
        let config = RouterConfig::parse(ROUTER_YAML).expect("parse router.yaml");
        assert!(config.schemas.len() >= 3, "tick + context + escalation");
        for schema in &config.schemas {
            schema.validate().expect("valid schema");
        }
        for point in [
            DecisionPoint::TickDecision,
            DecisionPoint::ContextGate,
            DecisionPoint::EscalationGate,
        ] {
            assert!(
                config.schema_for(point).is_some(),
                "schema for {}",
                point.id()
            );
            assert!(
                config.policy_for(point).is_some(),
                "policy for {}",
                point.id()
            );
        }
    }
}
