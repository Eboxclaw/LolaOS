//! Domain-neutral execution primitives for LOLA ACE.
//!
//! This crate intentionally has no dependency on the legacy trading crates.
//! It owns run lifecycle, cancellation and host-enforced budgets. Model
//! execution and product-specific decision schemas live behind other APIs.

mod budget;
mod pipeline;
mod runtime;

pub use budget::{Budget, BudgetError, BudgetUsage};
pub use pipeline::{
    CancellationToken, ExecutionParadigm, PipelineKind, PipelineRun, RunId, RunState,
    TransitionError,
};
pub use runtime::{
    ContextEntry, ContextRole, ContextSnapshot, DecisionReadout, DecisionReadoutReport,
    DualPipeline, EventLedger, FailureReason, GenerationOptions, PipelineEvent, RouteDecision,
    RoutePolicy, RunOutcome, ScoredChoice, StreamingEngine, StreamingReport,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_rejects_usage_beyond_host_limits() {
        let budget = Budget {
            max_transitions: 3,
            max_model_calls: 1,
            max_input_tokens: 128,
            max_output_tokens: 32,
            max_tool_calls: 0,
        };
        let mut usage = BudgetUsage::default();
        usage.record_transition(&budget).unwrap();
        usage.record_model_call(&budget).unwrap();
        assert_eq!(
            usage.record_model_call(&budget),
            Err(BudgetError::ModelCallsExceeded)
        );
        assert_eq!(
            usage.record_tool_call(&budget),
            Err(BudgetError::ToolCallsExceeded)
        );
    }

    #[test]
    fn decision_and_streaming_runs_have_independent_lifecycles() {
        let mut decision = PipelineRun::new(RunId(1), PipelineKind::Decision);
        let streaming = PipelineRun::new(RunId(2), PipelineKind::Streaming);
        assert_eq!(decision.state(), RunState::Created);
        assert_eq!(streaming.state(), RunState::Created);
        decision.start().unwrap();
        assert_eq!(decision.state(), RunState::Running);
        assert_eq!(streaming.state(), RunState::Created);
    }

    #[test]
    fn cancellation_is_idempotent_and_shared_by_clones() {
        let token = CancellationToken::new();
        let observer = token.clone();
        assert!(!observer.is_cancelled());
        token.cancel();
        token.cancel();
        assert!(observer.is_cancelled());
    }

    // ---- Dual pipeline (runtime.rs) ----

    use crate::pipeline::RunId;
    use crate::runtime::{
        ContextEntry, ContextRole, ContextSnapshot, DecisionReadout, DecisionReadoutReport,
        DualPipeline, FailureReason, RouteDecision, RoutePolicy, RunOutcome, ScoredChoice,
        StreamingEngine, StreamingReport,
    };

    struct PrefillOnlyReadout {
        scores: Vec<ScoredChoice>,
        input_tokens: u64,
        calls: std::rc::Rc<std::cell::Cell<u32>>,
    }

    impl DecisionReadout for PrefillOnlyReadout {
        fn score(
            &self,
            _snapshot: &ContextSnapshot,
            _choices: &[&str],
            _run_id: RunId,
        ) -> Result<DecisionReadoutReport, FailureReason> {
            self.calls.set(self.calls.get() + 1);
            Ok(DecisionReadoutReport {
                scores: self.scores.clone(),
                input_tokens: self.input_tokens,
                kv_dropped: true,
            })
        }
    }

    struct CountingStream {
        chunks: Vec<&'static str>,
        calls: std::rc::Rc<std::cell::Cell<u32>>,
    }

    impl StreamingEngine for CountingStream {
        fn generate(
            &self,
            _snapshot: &ContextSnapshot,
            _options: &crate::runtime::GenerationOptions,
            _run_id: RunId,
            cancellation: &CancellationToken,
            on_chunk: &mut dyn FnMut(u32, &str),
        ) -> Result<StreamingReport, FailureReason> {
            self.calls.set(self.calls.get() + 1);
            let mut sequence = 0;
            for text in &self.chunks {
                if cancellation.is_cancelled() {
                    return Err(FailureReason::Cancelled);
                }
                sequence += 1;
                on_chunk(sequence, text);
            }
            Ok(StreamingReport {
                output_tokens: 9,
                input_tokens: 40,
                ttft_ms: 5,
            })
        }
    }

    fn snapshot_entries() -> Vec<ContextEntry> {
        vec![ContextEntry {
            role: ContextRole::User,
            content: "state signature POSITIVE; candidate BUY".to_string(),
        }]
    }

    fn high_scores() -> Vec<ScoredChoice> {
        vec![ScoredChoice {
            name: "PROCEED".to_string(),
            raw_score: 0.9,
        }]
    }

    #[test]
    fn direct_route_never_invokes_the_generator() {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let decision = PrefillOnlyReadout {
            scores: high_scores(),
            input_tokens: 32,
            calls: calls.clone(),
        };
        let stream_calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let streaming = CountingStreamProbe {
            counter: stream_calls.clone(),
        };
        let mut pipeline =
            DualPipeline::new(test_budget(), &decision, &streaming, RoutePolicy::default());
        let token = CancellationToken::new();
        let (outcome, ledger) =
            pipeline.execute(snapshot_entries(), &["PROCEED"], &token, &mut |_, _| {});
        match outcome {
            RunOutcome::Direct { route, .. } => assert_eq!(route, RouteDecision::Direct),
            other => panic!("expected direct, got {other:?}"),
        }
        assert_eq!(calls.get(), 1, "readout called exactly once");
        assert!(ledger
            .events()
            .iter()
            .all(|e| !matches!(e, crate::runtime::PipelineEvent::StreamingStarted { .. })));
    }

    struct CountingStreamProbe {
        counter: std::rc::Rc<std::cell::Cell<u32>>,
    }

    impl StreamingEngine for CountingStreamProbe {
        fn generate(
            &self,
            _snapshot: &ContextSnapshot,
            _options: &crate::runtime::GenerationOptions,
            _run_id: RunId,
            _cancellation: &CancellationToken,
            _on_chunk: &mut dyn FnMut(u32, &str),
        ) -> Result<StreamingReport, FailureReason> {
            self.counter.set(self.counter.get() + 1);
            panic!("generator must not run on the direct route");
        }
    }

    #[test]
    fn escalate_route_generates_with_ordered_chunks_and_fresh_snapshot() {
        let decision = PrefillOnlyReadout {
            scores: vec![ScoredChoice {
                name: "PROCEED".to_string(),
                raw_score: 0.65, // between escalate 0.60 and direct 0.70
            }],
            input_tokens: 32,
            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let streaming = CountingStream {
            chunks: vec!["He", "llo", "!"],
            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let mut pipeline =
            DualPipeline::new(test_budget(), &decision, &streaming, RoutePolicy::default());
        let token = CancellationToken::new();
        let mut seen: Vec<(u32, String)> = Vec::new();
        let (outcome, ledger) = pipeline.execute(
            snapshot_entries(),
            &["PROCEED"],
            &token,
            &mut |seq, text| {
                seen.push((seq, text.to_string()));
            },
        );
        match outcome {
            RunOutcome::Escalated { reply, report, .. } => {
                assert_eq!(reply, "Hello!");
                assert_eq!(report.output_tokens, 9);
            }
            other => panic!("expected escalated, got {other:?}"),
        }
        assert_eq!(
            seen,
            vec![(1, "He".into()), (2, "llo".into()), (3, "!".into())]
        );
        assert!(matches!(
            ledger.events().last(),
            Some(crate::runtime::PipelineEvent::StreamingCompleted { .. })
        ));
    }

    #[test]
    fn low_scores_fail_closed_without_generation() {
        let decision = PrefillOnlyReadout {
            scores: vec![ScoredChoice {
                name: "PROCEED".to_string(),
                raw_score: 0.2,
            }],
            input_tokens: 32,
            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let streaming = CountingStreamProbe {
            counter: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let mut pipeline =
            DualPipeline::new(test_budget(), &decision, &streaming, RoutePolicy::default());
        let token = CancellationToken::new();
        let (outcome, _) =
            pipeline.execute(snapshot_entries(), &["PROCEED"], &token, &mut |_, _| {});
        assert!(matches!(outcome, RunOutcome::Failed(_)));
    }

    #[test]
    fn content_free_prior_calibrates_scores() {
        // Raw 0.55 against a content-free prior of 0.5 → calibrated 1.1 ≥ direct.
        let policy = RoutePolicy {
            content_free_prior: Some(vec![0.5]),
            ..RoutePolicy::default()
        };
        let scores = vec![ScoredChoice {
            name: "PROCEED".to_string(),
            raw_score: 0.55,
        }];
        assert_eq!(policy.route(&scores), RouteDecision::Direct);
        // Prior-size mismatch fails closed.
        let policy = RoutePolicy {
            content_free_prior: Some(vec![0.5, 0.5]),
            ..RoutePolicy::default()
        };
        assert_eq!(policy.route(&scores), RouteDecision::FailClosed);
    }

    #[test]
    fn budget_exhaustion_fails_closed_before_generation() {
        // Escalate-band scores (0.60–0.70) so the exhausted model-call
        // budget trips before the generator runs.
        let decision = PrefillOnlyReadout {
            scores: vec![ScoredChoice {
                name: "PROCEED".to_string(),
                raw_score: 0.65,
            }],
            input_tokens: 32,
            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let streaming = CountingStreamProbe {
            counter: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        // One model call only: the decision readout consumes it; escalation
        // must then fail closed instead of generating.
        let budget = Budget {
            max_transitions: 3,
            max_model_calls: 1,
            max_input_tokens: 4096,
            max_output_tokens: 256,
            max_tool_calls: 0,
        };
        let mut pipeline = DualPipeline::new(budget, &decision, &streaming, RoutePolicy::default());
        let token = CancellationToken::new();
        let (outcome, ledger) =
            pipeline.execute(snapshot_entries(), &["PROCEED"], &token, &mut |_, _| {});
        assert!(matches!(outcome, RunOutcome::Failed(_)));
        assert!(matches!(
            ledger.events().last(),
            Some(crate::runtime::PipelineEvent::StreamingFailed { .. })
        ));
    }

    #[test]
    fn cancellation_before_execution_fails_the_decision_phase() {
        let decision = PrefillOnlyReadout {
            scores: high_scores(),
            input_tokens: 32,
            calls: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let streaming = CountingStreamProbe {
            counter: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        let mut pipeline =
            DualPipeline::new(test_budget(), &decision, &streaming, RoutePolicy::default());
        let token = CancellationToken::new();
        token.cancel();
        let (outcome, ledger) =
            pipeline.execute(snapshot_entries(), &["PROCEED"], &token, &mut |_, _| {});
        assert!(matches!(
            outcome,
            RunOutcome::Failed(FailureReason::Cancelled)
        ));
        assert!(matches!(
            ledger.events().last(),
            Some(crate::runtime::PipelineEvent::DecisionFailed { .. })
        ));
    }

    fn test_budget() -> Budget {
        Budget {
            max_transitions: 8,
            max_model_calls: 4,
            max_input_tokens: 4096,
            max_output_tokens: 256,
            max_tool_calls: 0,
        }
    }
}
