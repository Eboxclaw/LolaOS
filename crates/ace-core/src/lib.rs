//! Domain-neutral execution primitives for LOLA ACE.
//!
//! This crate intentionally has no dependency on the legacy trading crates.
//! It owns run lifecycle, cancellation and host-enforced budgets. Model
//! execution and product-specific decision schemas live behind other APIs.

mod budget;
mod pipeline;

pub use budget::{Budget, BudgetError, BudgetUsage};
pub use pipeline::{
    CancellationToken, ExecutionParadigm, PipelineKind, PipelineRun, RunId, RunState,
    TransitionError,
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
}
