//! SB — real-data quality harness (ADR-038/041). The instruments that
//! gate the quantization fruits (F2 q8 KV, F5 NEON exp) and QAD claims:
//! a fixed agent-typical corpus, a needle-in-haystack probe (the failure
//! mode ppl misses), and greedy token-identity vs a frozen reference.
//!
//! These measure against real model outputs — they need the oracle
//! (S2.c) or `lola-custom` (K5+) to produce logits. What's here now is
//! the harness + corpus + probe logic, fully tested on synthetic
//! distributions; the model-gated numbers land once a backend exists.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// One corpus prompt: agent-typical shapes where quality regressions hide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusPrompt {
    pub id: String,
    pub prompt: String,
    pub expected_shape: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corpus {
    pub corpus_id: String,
    pub schema_version: u32,
    pub description: String,
    pub prompts: Vec<CorpusPrompt>,
}

impl Corpus {
    pub fn load(path: &Path) -> Result<Self, CorpusError> {
        let text = std::fs::read_to_string(path)?;
        let corpus: Corpus = serde_json::from_str(&text)?;
        if corpus.prompts.is_empty() {
            return Err(CorpusError::Invalid("corpus is empty".into()));
        }
        let mut ids: Vec<&str> = corpus.prompts.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() != corpus.prompts.len() {
            return Err(CorpusError::Invalid("duplicate prompt ids".into()));
        }
        Ok(corpus)
    }

    pub fn from_json(text: &str) -> Result<Self, CorpusError> {
        let corpus: Corpus = serde_json::from_str(text)?;
        if corpus.prompts.is_empty() {
            return Err(CorpusError::Invalid("corpus is empty".into()));
        }
        Ok(corpus)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid corpus: {0}")]
    Invalid(String),
}

/// Perplexity of a token stream: exp(mean(-log p(tok))). The ppl-Δ gate
/// compares candidate vs reference on the same corpus.
pub fn perplexity(log_probs: &[f64]) -> f64 {
    if log_probs.is_empty() {
        return 0.0;
    }
    let mean_neg_log = -log_probs.iter().sum::<f64>() / log_probs.len() as f64;
    mean_neg_log.exp()
}

/// ppl-Δ gate: the candidate's perplexity on the corpus vs the
/// reference's. Success per ADR-038: Δ ≤ +0.5%.
pub fn ppl_delta(reference: f64, candidate: f64) -> f64 {
    (candidate - reference) / reference
}

/// Needle-in-haystack probe: a known fact is buried at a depth fraction
/// of a long context; success = the answer is retrieved. ppl misses
/// retrieval collapse — this is the gate that catches it (F2's real
/// failure mode). Logic is deterministic on the provided
/// `retrieved: bool` — the harness drives it per depth once a backend
/// exists.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NiaResult {
    pub depth_fraction: f64,
    pub retrieved: bool,
}

pub fn nia_gate(results: &[NiaResult]) -> bool {
    !results.is_empty() && results.iter().all(|r| r.retrieved)
}

/// Greedy token-identity vs a frozen reference: argmax match fraction
/// (reuses `kernels::quality::token_identity`; re-exported here as the
/// SB gate entry point).
pub use crate::kernels::quality::token_identity;

/// The full SB quality gate set — what a model-gated change must pass
/// before it ships (the real numbers land with a backend; the harness is
/// what's tested here).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QualityGate {
    pub ppl_delta_max: f64,
    pub token_identity_min: f64,
    pub nia_required: bool,
}

impl Default for QualityGate {
    fn default() -> Self {
        Self {
            ppl_delta_max: 0.005,      // ≤ +0.5% ppl
            token_identity_min: 0.995, // ≥ 99.5% greedy argmax match
            nia_required: true,        // needle must be found at all depths
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perplexity_of_uniform_is_vocab_size() {
        // Uniform over 10 tokens: p=0.1 ⇒ ppl = 10.
        let log_probs: Vec<f64> = (0..10).map(|_| (0.1f64).ln()).collect();
        assert!((perplexity(&log_probs) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn ppl_delta_gate_bounds() {
        let reference = 8.0;
        let candidate_ok = 8.03; // +0.375%
        let candidate_bad = 8.06; // +0.75%
        assert!(ppl_delta(reference, candidate_ok) <= 0.005);
        assert!(ppl_delta(reference, candidate_bad) > 0.005);
    }

    #[test]
    fn nia_gate_requires_all_depths() {
        let all_retrieved = vec![
            NiaResult {
                depth_fraction: 0.1,
                retrieved: true,
            },
            NiaResult {
                depth_fraction: 0.5,
                retrieved: true,
            },
            NiaResult {
                depth_fraction: 0.9,
                retrieved: true,
            },
        ];
        assert!(nia_gate(&all_retrieved));
        let one_miss = vec![
            NiaResult {
                depth_fraction: 0.1,
                retrieved: true,
            },
            NiaResult {
                depth_fraction: 0.5,
                retrieved: false,
            },
        ];
        assert!(!nia_gate(&one_miss));
        assert!(!nia_gate(&[]));
    }

    #[test]
    fn shipped_corpus_loads() {
        let corpus = Corpus::from_json(include_str!("../../../evals/corpus/agent-tasks-v1.json"))
            .expect("shipped corpus must load");
        assert_eq!(corpus.corpus_id, "agent-tasks-v1");
        assert!(corpus.prompts.len() >= 5);
        // Shapes are agent-typical and non-trivial.
        assert!(corpus
            .prompts
            .iter()
            .any(|p| p.expected_shape == "xml-tool-call"));
        assert!(corpus
            .prompts
            .iter()
            .any(|p| p.expected_shape == "enumeration"));
    }

    #[test]
    fn quality_gate_defaults_are_the_contract() {
        let gate = QualityGate::default();
        assert_eq!(gate.ppl_delta_max, 0.005);
        assert_eq!(gate.token_identity_min, 0.995);
        assert!(gate.nia_required);
    }
}
