//! The `Infer` boundary: everything the kernel may ask of a model, and
//! nothing more (ADR-003).
//!
//! Backends implementing this trait:
//! - `reference-llamacpp` (S2): llama.cpp linked **in-process** as the
//!   correctness oracle. Not a sidecar — one process, mobile-clean.
//! - `lola-custom` (S3): the LFM3-optimized engine. Ships only after
//!   golden-test parity with the oracle.

use lola_core::contracts::DecisionV1;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    /// Structured market-state decisions (d1 family).
    Decision,
    /// Research, analysis, tool orchestration (LFM3 family).
    Agent,
    /// Registered specialists (VL / Audio / Omni / Base).
    Specialist,
}

impl std::fmt::Display for ModelRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decision => write!(f, "decision"),
            Self::Agent => write!(f, "agent"),
            Self::Specialist => write!(f, "specialist"),
        }
    }
}

/// Named, typed questions the decision path may ask — answered in one call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    Regime,
    CandidateAssessment,
    DataSufficiency,
    RiskFlags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub name: String,
    pub kind: QuestionKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub state: lola_core::contracts::MarketStateV1,
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub prompt: String,
    pub max_tokens: u32,
    pub stop: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub text: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

#[derive(Debug, Error)]
pub enum InferError {
    /// The backend's output violated the decision contract. Fails closed —
    /// the kernel never repairs a malformed decision.
    #[error("contract violation: {0}")]
    Contract(String),
    #[error("model unavailable: {0}")]
    Unavailable(String),
    #[error("model abstained: {0}")]
    Abstained(String),
}

/// The single inference boundary. The kernel cannot tell which backend
/// implements it, and no backend gets capabilities beyond these two calls.
pub trait Infer: Send + Sync {
    fn model_id(&self) -> &str;

    /// Structured decision path (d1 family). Returns a contract-conforming
    /// [`DecisionV1`] or [`InferError::Contract`] — never a raw model string.
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionV1, InferError>;

    /// Generative path (LFM3 family). Never on the candle-critical path.
    fn generate(&self, request: &GenerateRequest) -> Result<GenerateResponse, InferError>;
}
