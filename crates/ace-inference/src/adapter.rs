//! Bridges the `ace-core` dual-pipeline ports onto [`InferenceBackend`]
//! implementations (ADR-043 §6: Decision and Streaming communicate with
//! the model layer through the backend-neutral contract only).
//!
//! Each port owns its own session behind a mutex — the two pipelines are
//! peers and never share model state (ARCHITECTURE §2: a Decision run must
//! not hold the Streaming pipeline's lock). Tokenization is injected: the
//! adapter never invents token IDs.

use std::ops::ControlFlow;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ace_core::{
    CancellationToken, ContextSnapshot, DecisionReadout, DecisionReadoutReport, FailureReason,
    GenerationOptions as PipelineOptions, RunId as PipelineRunId, ScoredChoice, StreamingEngine,
    StreamingReport,
};

use crate::{
    GenerationOptions, InferenceBackend, InferenceError, InferenceSession, RunDiagnostics, RunId,
    StreamChunk,
};

/// Text → token IDs for one model. Implemented by the platform layer
/// (e.g. the llama.cpp tokenizer); tests use trivial mappings.
pub trait SnapshotTokenizer: Send + Sync {
    fn tokenize(&self, snapshot: &ContextSnapshot) -> Vec<u32>;
    /// Token IDs for one choice label, in choice order.
    fn tokenize_choice(&self, choice: &str) -> Vec<u32>;
}

/// Decision port over a backend that implements [`crate::InferenceBackend::score_choices`].
pub struct ContractDecisionReadout<'a> {
    backend: &'a dyn InferenceBackend,
    session: Mutex<InferenceSession>,
    tokenizer: Arc<dyn SnapshotTokenizer>,
}

impl<'a> ContractDecisionReadout<'a> {
    pub fn new(
        backend: &'a dyn InferenceBackend,
        session: InferenceSession,
        tokenizer: Arc<dyn SnapshotTokenizer>,
    ) -> Self {
        Self {
            backend,
            session: Mutex::new(session),
            tokenizer,
        }
    }
}

impl DecisionReadout for ContractDecisionReadout<'_> {
    fn score(
        &self,
        snapshot: &ContextSnapshot,
        choices: &[&str],
        run_id: PipelineRunId,
    ) -> Result<DecisionReadoutReport, FailureReason> {
        let input = self.tokenizer.tokenize(snapshot);
        let choice_tokens: Vec<Vec<u32>> = choices
            .iter()
            .map(|c| self.tokenizer.tokenize_choice(c))
            .collect();
        let mut session = self
            .session
            .lock()
            .map_err(|_| FailureReason::BackendError)?;
        let scores =
            self.backend
                .score_choices(&mut session, RunId(run_id.0), &input, &choice_tokens)?;
        if scores.scores.len() != choices.len() {
            return Err(FailureReason::BackendError);
        }
        Ok(DecisionReadoutReport {
            scores: choices
                .iter()
                .zip(&scores.scores)
                .map(|(name, raw)| ScoredChoice {
                    name: (*name).to_string(),
                    raw_score: *raw,
                })
                .collect(),
            input_tokens: input.len() as u64,
            kv_dropped: scores.kv_dropped,
        })
    }
}

/// Streaming port over any backend's [`crate::InferenceBackend::generate`].
pub struct ContractStreamingEngine<'a> {
    backend: &'a dyn InferenceBackend,
    session: Mutex<InferenceSession>,
    tokenizer: Arc<dyn SnapshotTokenizer>,
    options: GenerationOptions,
}

impl<'a> ContractStreamingEngine<'a> {
    pub fn new(
        backend: &'a dyn InferenceBackend,
        session: InferenceSession,
        tokenizer: Arc<dyn SnapshotTokenizer>,
        options: GenerationOptions,
    ) -> Self {
        Self {
            backend,
            session: Mutex::new(session),
            tokenizer,
            options,
        }
    }
}

impl StreamingEngine for ContractStreamingEngine<'_> {
    fn generate(
        &self,
        snapshot: &ContextSnapshot,
        options: &PipelineOptions,
        run_id: PipelineRunId,
        cancellation: &CancellationToken,
        on_chunk: &mut dyn FnMut(u32, &str),
    ) -> Result<StreamingReport, FailureReason> {
        let input = self.tokenizer.tokenize(snapshot);
        let contract_options = GenerationOptions {
            max_output_tokens: options.max_output_tokens.min(u32::MAX as u64) as u32,
            temperature: options.temperature,
            ..self.options.clone()
        };
        let started = Instant::now();
        let mut first_chunk_ms: Option<u64> = None;
        let mut output_tokens = 0u64;
        let mut cancelled_in_band = false;

        {
            let mut session = self
                .session
                .lock()
                .map_err(|_| FailureReason::BackendError)?;
            let result = self.backend.generate(
                &mut session,
                RunId(run_id.0),
                &input,
                &contract_options,
                &mut |chunk: StreamChunk| {
                    if first_chunk_ms.is_none() {
                        first_chunk_ms = Some(started.elapsed().as_millis() as u64);
                    }
                    output_tokens = output_tokens.max(chunk.sequence);
                    on_chunk(chunk.sequence as u32, &chunk.text);
                    if cancellation.is_cancelled() {
                        cancelled_in_band = true;
                        return ControlFlow::Break(());
                    }
                    ControlFlow::Continue(())
                },
            );
            match result {
                Ok(report) => {
                    if cancelled_in_band || report.finish_reason == crate::FinishReason::Cancelled {
                        return Err(FailureReason::Cancelled);
                    }
                    let ttft = self
                        .ttft_from_diagnostics(&session, run_id)
                        .or(first_chunk_ms)
                        .unwrap_or(0);
                    Ok(StreamingReport {
                        output_tokens: report.generated_tokens as u64,
                        input_tokens: input.len() as u64,
                        ttft_ms: ttft,
                    })
                }
                Err(InferenceError::Cancelled) => Err(FailureReason::Cancelled),
                Err(_) => Err(FailureReason::BackendError),
            }
        }
    }
}

impl ContractStreamingEngine<'_> {
    /// Prefer backend-reported TTFT (native monotonic metrics) over the
    /// adapter's wall-clock estimate.
    fn ttft_from_diagnostics(
        &self,
        session: &InferenceSession,
        run_id: PipelineRunId,
    ) -> Option<u64> {
        let diagnostics = self.backend.diagnostics(session, RunId(run_id.0)).ok()?;
        let RunDiagnostics { ttft_ms, .. } = diagnostics;
        ttft_ms
    }
}

impl From<InferenceError> for FailureReason {
    fn from(_: InferenceError) -> Self {
        FailureReason::BackendError
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ChoiceScores, FinishReason, GenerationReport, ModelDescriptor, ModelId, SessionId,
    };
    use ace_core::{Budget, ContextEntry, ContextRole, DualPipeline, RoutePolicy};
    use std::cell::Cell;
    

    const MODEL: ModelDescriptor = ModelDescriptor {
        id: ModelId(String::new()),
        family: String::new(),
        revision: String::new(),
        artifact_sha256: None,
        context_length: None,
    };
    const fn const_model_id() -> ModelId {
        ModelId(String::new())
    }

    /// Trivial tokenizer: byte-value + 1000 per entry; choice = first byte + index.
    struct ByteTokenizer;
    impl SnapshotTokenizer for ByteTokenizer {
        fn tokenize(&self, snapshot: &ContextSnapshot) -> Vec<u32> {
            let mut tokens = Vec::new();
            for (index, entry) in snapshot.entries().iter().enumerate() {
                for byte in entry.content.bytes().take(8) {
                    tokens.push(byte as u32 + 100 * (index as u32 + 1));
                }
            }
            tokens
        }
        fn tokenize_choice(&self, choice: &str) -> Vec<u32> {
            choice.bytes().take(4).map(|b| b as u32 + 7).collect()
        }
    }

    /// Backend whose generate() would corrupt the decision semantics if the
    /// adapter ever routed the readout through it.
    struct ScoringBackend {
        scores: Vec<f32>,
        prefill_calls: Cell<u32>,
        generate_calls: Cell<u32>,
    }

    impl InferenceBackend for ScoringBackend {
        fn id(&self) -> crate::BackendId {
            crate::BackendId("fake".into())
        }
        fn capabilities(&self, _: &ModelDescriptor) -> crate::BackendCapabilities {
            crate::BackendCapabilities::default()
        }
        fn load_model(&mut self, _: &ModelDescriptor) -> Result<(), InferenceError> {
            Ok(())
        }
        fn unload_model(&mut self, _: &ModelId) -> Result<(), InferenceError> {
            Ok(())
        }
        fn create_session(
            &self,
            _: &ModelDescriptor,
            session_id: SessionId,
        ) -> Result<InferenceSession, InferenceError> {
            Ok(InferenceSession::new(session_id, const_model_id(), ()))
        }
        fn generate(
            &self,
            _: &mut InferenceSession,
            run_id: RunId,
            _: &[u32],
            _: &GenerationOptions,
            on_chunk: &mut dyn FnMut(StreamChunk) -> ControlFlow<()>,
        ) -> Result<GenerationReport, InferenceError> {
            self.generate_calls.set(self.generate_calls.get() + 1);
            for (sequence, text) in ["He", "llo"].into_iter().enumerate() {
                if on_chunk(StreamChunk {
                    run_id,
                    sequence: sequence as u64 + 1,
                    token_ids: vec![],
                    text: text.into(),
                }) == ControlFlow::Break(())
                {
                    return Ok(GenerationReport {
                        run_id,
                        generated_tokens: sequence as u32 + 1,
                        finish_reason: FinishReason::Cancelled,
                    });
                }
            }
            Ok(GenerationReport {
                run_id,
                generated_tokens: 2,
                finish_reason: FinishReason::EndOfSequence,
            })
        }
        fn score_choices(
            &self,
            _: &mut InferenceSession,
            run_id: RunId,
            input_tokens: &[u32],
            choices: &[Vec<u32>],
        ) -> Result<ChoiceScores, InferenceError> {
            self.prefill_calls.set(self.prefill_calls.get() + 1);
            assert!(!input_tokens.is_empty(), "tokenizer must feed the backend");
            assert_eq!(choices.len(), self.scores.len());
            Ok(ChoiceScores {
                run_id,
                scores: self.scores.clone(),
                kv_dropped: true,
            })
        }
        fn diagnostics(
            &self,
            _: &InferenceSession,
            _: RunId,
        ) -> Result<RunDiagnostics, InferenceError> {
            Ok(RunDiagnostics {
                ttft_ms: Some(12),
                ..RunDiagnostics::default()
            })
        }
    }

    fn entries() -> Vec<ContextEntry> {
        vec![ContextEntry {
            role: ContextRole::User,
            content: "state POSITIVE candidate BUY".to_string(),
        }]
    }

    #[test]
    fn direct_route_scores_through_the_contract_without_generating() {
        let backend = ScoringBackend {
            scores: vec![0.9],
            prefill_calls: Cell::new(0),
            generate_calls: Cell::new(0),
        };
        let tokenizer = Arc::new(ByteTokenizer);
        let decision = ContractDecisionReadout::new(
            &backend,
            backend.create_session(&MODEL, SessionId(1)).unwrap(),
            tokenizer.clone(),
        );
        let streaming = ContractStreamingEngine::new(
            &backend,
            backend.create_session(&MODEL, SessionId(2)).unwrap(),
            tokenizer,
            GenerationOptions::default(),
        );
        let budget = Budget {
            max_transitions: 8,
            max_model_calls: 4,
            max_input_tokens: 4096,
            max_output_tokens: 256,
            max_tool_calls: 0,
        };
        let mut pipeline = DualPipeline::new(budget, &decision, &streaming, RoutePolicy::default());
        let token = CancellationToken::default();
        let (outcome, ledger) = pipeline.execute(entries(), &["PROCEED"], &token, &mut |_, _| {});
        assert!(matches!(
            outcome,
            ace_core::RunOutcome::Direct {
                route: ace_core::RouteDecision::Direct,
                ..
            }
        ));
        assert_eq!(backend.prefill_calls.get(), 1);
        assert_eq!(backend.generate_calls.get(), 0);
        assert!(matches!(
            ledger.events().last(),
            Some(ace_core::PipelineEvent::DecisionScored { .. })
        ));
    }

    #[test]
    fn escalate_route_generates_through_the_contract_with_backend_ttft() {
        let backend = ScoringBackend {
            scores: vec![0.65],
            prefill_calls: Cell::new(0),
            generate_calls: Cell::new(0),
        };
        let tokenizer = Arc::new(ByteTokenizer);
        let decision = ContractDecisionReadout::new(
            &backend,
            backend.create_session(&MODEL, SessionId(1)).unwrap(),
            tokenizer.clone(),
        );
        let streaming = ContractStreamingEngine::new(
            &backend,
            backend.create_session(&MODEL, SessionId(2)).unwrap(),
            tokenizer,
            GenerationOptions::default(),
        );
        let budget = Budget {
            max_transitions: 8,
            max_model_calls: 4,
            max_input_tokens: 4096,
            max_output_tokens: 256,
            max_tool_calls: 0,
        };
        let mut pipeline = DualPipeline::new(budget, &decision, &streaming, RoutePolicy::default());
        let token = CancellationToken::default();
        let mut chunks = Vec::new();
        let (outcome, ledger) =
            pipeline.execute(entries(), &["PROCEED"], &token, &mut |seq, text| {
                chunks.push((seq, text.to_string()));
            });
        match outcome {
            ace_core::RunOutcome::Escalated { reply, report, .. } => {
                assert_eq!(reply, "Hello");
                assert_eq!(report.ttft_ms, 12, "TTFT comes from backend diagnostics");
                assert_eq!(report.output_tokens, 2);
            }
            other => panic!("expected escalated, got {other:?}"),
        }
        assert_eq!(chunks, vec![(1, "He".into()), (2, "llo".into())]);
        assert!(matches!(
            ledger.events().last(),
            Some(ace_core::PipelineEvent::StreamingCompleted { .. })
        ));
    }

    #[test]
    fn in_band_cancellation_maps_to_cancelled_failure() {
        struct CancellingBackend(ScoringBackend);

        impl InferenceBackend for CancellingBackend {
            fn id(&self) -> crate::BackendId {
                self.0.id()
            }
            fn capabilities(&self, m: &ModelDescriptor) -> crate::BackendCapabilities {
                self.0.capabilities(m)
            }
            fn load_model(&mut self, m: &ModelDescriptor) -> Result<(), InferenceError> {
                self.0.load_model(m)
            }
            fn unload_model(&mut self, m: &ModelId) -> Result<(), InferenceError> {
                self.0.unload_model(m)
            }
            fn create_session(
                &self,
                m: &ModelDescriptor,
                s: SessionId,
            ) -> Result<InferenceSession, InferenceError> {
                self.0.create_session(m, s)
            }
            fn generate(
                &self,
                session: &mut InferenceSession,
                run_id: RunId,
                input: &[u32],
                options: &GenerationOptions,
                _on_chunk: &mut dyn FnMut(StreamChunk) -> ControlFlow<()>,
            ) -> Result<GenerationReport, InferenceError> {
                // Emit one chunk, then report cancellation.
                let _ = self.0.generate(session, run_id, input, options, &mut |_| {
                    ControlFlow::Break(())
                });
                Ok(GenerationReport {
                    run_id,
                    generated_tokens: 1,
                    finish_reason: FinishReason::Cancelled,
                })
            }
            fn score_choices(
                &self,
                s: &mut InferenceSession,
                r: RunId,
                i: &[u32],
                c: &[Vec<u32>],
            ) -> Result<ChoiceScores, InferenceError> {
                self.0.score_choices(s, r, i, c)
            }
        }

        let inner = ScoringBackend {
            scores: vec![0.65],
            prefill_calls: Cell::new(0),
            generate_calls: Cell::new(0),
        };
        let backend = CancellingBackend(inner);
        let tokenizer = Arc::new(ByteTokenizer);
        let decision = ContractDecisionReadout::new(
            &backend,
            backend.create_session(&MODEL, SessionId(1)).unwrap(),
            tokenizer.clone(),
        );
        let streaming = ContractStreamingEngine::new(
            &backend,
            backend.create_session(&MODEL, SessionId(2)).unwrap(),
            tokenizer,
            GenerationOptions::default(),
        );
        let budget = Budget {
            max_transitions: 8,
            max_model_calls: 4,
            max_input_tokens: 4096,
            max_output_tokens: 256,
            max_tool_calls: 0,
        };
        let mut pipeline = DualPipeline::new(budget, &decision, &streaming, RoutePolicy::default());
        let token = CancellationToken::default();
        let (outcome, ledger) = pipeline.execute(entries(), &["PROCEED"], &token, &mut |_, _| {});
        assert!(matches!(
            outcome,
            ace_core::RunOutcome::Failed(FailureReason::Cancelled)
        ));
        assert!(matches!(
            ledger.events().last(),
            Some(ace_core::PipelineEvent::StreamingCancelled { .. })
        ));
    }

    #[test]
    fn contract_defaults_are_unsupported_not_panicking() {
        struct MinimalBackend;
        impl InferenceBackend for MinimalBackend {
            fn id(&self) -> crate::BackendId {
                crate::BackendId("minimal".into())
            }
            fn capabilities(&self, _: &ModelDescriptor) -> crate::BackendCapabilities {
                crate::BackendCapabilities::default()
            }
            fn load_model(&mut self, _: &ModelDescriptor) -> Result<(), InferenceError> {
                Ok(())
            }
            fn unload_model(&mut self, _: &ModelId) -> Result<(), InferenceError> {
                Ok(())
            }
            fn create_session(
                &self,
                _: &ModelDescriptor,
                session_id: SessionId,
            ) -> Result<InferenceSession, InferenceError> {
                Ok(InferenceSession::new(session_id, const_model_id(), ()))
            }
            fn generate(
                &self,
                _: &mut InferenceSession,
                _: RunId,
                _: &[u32],
                _: &GenerationOptions,
                _: &mut dyn FnMut(StreamChunk) -> ControlFlow<()>,
            ) -> Result<GenerationReport, InferenceError> {
                Err(InferenceError::BackendFailure("unused".into()))
            }
        }

        let backend = MinimalBackend;
        let mut session = backend.create_session(&MODEL, SessionId(9)).unwrap();
        assert!(matches!(
            backend.prefill(&mut session, RunId(1), &[1]),
            Err(InferenceError::UnsupportedCapability(_))
        ));
        assert!(matches!(
            backend.score_choices(&mut session, RunId(1), &[1], &[vec![2]]),
            Err(InferenceError::UnsupportedCapability(_))
        ));
        assert!(matches!(
            backend.diagnostics(&session, RunId(1)),
            Err(InferenceError::UnsupportedCapability(_))
        ));
        assert!(matches!(
            backend.cancel(RunId(1)),
            Err(InferenceError::UnsupportedCapability(_))
        ));
    }
}
