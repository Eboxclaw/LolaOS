//! The dual Decision + Streaming pipeline (v0).
//!
//! ARCHITECTURE.md §2 mandates two peer pipelines with separate state
//! machines: Decision reads immutable context snapshots, runs the model
//! PREFILL-ONLY (ADR-024: zero generated tokens, transient KV) and owns
//! the deterministic pass-forward policy; Streaming is the only generator,
//! owns session state and ordered output. They communicate through typed
//! events and immutable snapshots (ADR-043 §6) — a Decision run never
//! holds Streaming's lock and never mutates an in-flight prompt.
//!
//! This module is a port-based orchestrator: the model-execution ports are
//! defined here and implemented by `ace-inference` adapters (or test
//! doubles). Host-owned budgets fail closed (ARCHITECTURE §5).

use crate::budget::{Budget, BudgetError, BudgetUsage};
use crate::pipeline::{CancellationToken, RunId};

/// An immutable context handed to a pipeline phase. Entries are shared via
/// `Arc` so a snapshot can never be mutated after construction; a new
/// phase (e.g. escalation into Streaming) must build a NEW snapshot.
#[derive(Debug, Clone)]
pub struct ContextSnapshot {
    snapshot_id: u64,
    created_unix_ms: u64,
    entries: std::sync::Arc<[ContextEntry]>,
    estimated_input_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct ContextEntry {
    pub role: ContextRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextRole {
    System,
    User,
    Assistant,
    Tool,
}

impl ContextSnapshot {
    pub fn new(snapshot_id: u64, created_unix_ms: u64, entries: Vec<ContextEntry>) -> Self {
        // Rough estimator: ~4 chars/token, good enough for budget gating;
        // real token counts come from the backend report.
        let chars: usize = entries.iter().map(|e| e.content.len()).sum();
        let estimated_input_tokens = (chars as u64 / 4).max(entries.len() as u64);
        Self {
            snapshot_id,
            created_unix_ms,
            entries: entries.into(),
            estimated_input_tokens,
        }
    }

    pub fn snapshot_id(&self) -> u64 {
        self.snapshot_id
    }

    pub fn created_unix_ms(&self) -> u64 {
        self.created_unix_ms
    }

    pub fn entries(&self) -> &[ContextEntry] {
        &self.entries
    }

    pub fn estimated_input_tokens(&self) -> u64 {
        self.estimated_input_tokens
    }
}

/// Typed events crossing the Decision/Streaming boundary, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineEvent {
    DecisionStarted {
        run_id: RunId,
        snapshot_id: u64,
    },
    DecisionScored {
        run_id: RunId,
        route: RouteDecision,
    },
    DecisionFailed {
        run_id: RunId,
        reason: FailureReason,
    },
    StreamingStarted {
        run_id: RunId,
        snapshot_id: u64,
    },
    StreamingChunk {
        run_id: RunId,
        sequence: u32,
        text: String,
    },
    StreamingCompleted {
        run_id: RunId,
        output_tokens: u64,
    },
    StreamingCancelled {
        run_id: RunId,
    },
    StreamingFailed {
        run_id: RunId,
        reason: FailureReason,
    },
}

/// Ordered, append-only event ledger for one dual-pipeline execution.
#[derive(Debug, Default)]
pub struct EventLedger {
    events: Vec<PipelineEvent>,
}

impl EventLedger {
    pub fn record(&mut self, event: PipelineEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[PipelineEvent] {
        &self.events
    }
}

/// Deterministic pass-forward policy (ADR-036): the model proposes scores,
/// the policy disposes. Thresholds are PROVISIONAL (ctx ≥0.70, escalate
/// ≥0.60) until S5 calibration. Missing data fails closed.
#[derive(Debug, Clone)]
pub struct RoutePolicy {
    pub direct_threshold: f32,
    pub escalate_threshold: f32,
    /// Content-free prior per choice (contextual calibration,
    /// arXiv:2102.09690). When present, raw scores are divided by the
    /// matching prior before threshold comparison.
    pub content_free_prior: Option<Vec<f32>>,
}

impl Default for RoutePolicy {
    fn default() -> Self {
        Self {
            direct_threshold: 0.70,
            escalate_threshold: 0.60,
            content_free_prior: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouteDecision {
    /// Deterministic answer known without generation.
    Direct,
    /// Escalate to the Streaming model (the only generator).
    Escalate,
    /// Scores missing or uninterpretable — no generation, fail closed.
    FailClosed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredChoice {
    pub name: String,
    pub raw_score: f32,
}

impl RoutePolicy {
    /// Normalize against the content-free prior (when configured) and
    /// route. Calibration mismatches fail closed rather than guess.
    pub fn route(&self, scores: &[ScoredChoice]) -> RouteDecision {
        if scores.is_empty() {
            return RouteDecision::FailClosed;
        }
        let best = match &self.content_free_prior {
            None => scores.iter().map(|s| s.raw_score).fold(0.0, f32::max),
            Some(prior) => {
                if prior.len() != scores.len() {
                    return RouteDecision::FailClosed;
                }
                scores
                    .iter()
                    .zip(prior)
                    .map(|(s, p)| {
                        if *p <= 0.0 {
                            return 0.0;
                        }
                        s.raw_score / p
                    })
                    .fold(0.0, f32::max)
            }
        };
        if !best.is_finite() {
            return RouteDecision::FailClosed;
        }
        if best >= self.direct_threshold {
            RouteDecision::Direct
        } else if best >= self.escalate_threshold {
            RouteDecision::Escalate
        } else {
            RouteDecision::FailClosed
        }
    }
}

/// Port: the decision model. Contractually PREFILL-ONLY — implementors
/// score a fixed choice set off the prefill state and generate zero
/// tokens; any KV built for the readout is transient and dropped on
/// return (ADR-024). The returned `input_tokens` feeds the budget.
pub trait DecisionReadout {
    fn score(
        &self,
        snapshot: &ContextSnapshot,
        choices: &[&str],
        run_id: RunId,
    ) -> Result<DecisionReadoutReport, FailureReason>;
}

#[derive(Debug, Clone)]
pub struct DecisionReadoutReport {
    pub scores: Vec<ScoredChoice>,
    pub input_tokens: u64,
    /// Always true by contract: the readout leaves no resident KV behind.
    pub kv_dropped: bool,
}

/// Port: the streaming engine — the only generator. Ordered chunks are
/// delivered through the callback; cancellation is cooperative and
/// checked between chunks.
pub trait StreamingEngine {
    fn generate(
        &self,
        snapshot: &ContextSnapshot,
        options: &GenerationOptions,
        run_id: RunId,
        cancellation: &CancellationToken,
        on_chunk: &mut dyn FnMut(u32, &str),
    ) -> Result<StreamingReport, FailureReason>;
}

#[derive(Debug, Clone)]
pub struct GenerationOptions {
    pub max_output_tokens: u64,
    /// Deterministic default for policy-relevant generation.
    pub temperature: f32,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            max_output_tokens: 256,
            temperature: 0.1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StreamingReport {
    pub output_tokens: u64,
    pub input_tokens: u64,
    pub ttft_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    BudgetExceeded(BudgetError),
    BackendError,
    Cancelled,
}

/// Result of one dual-pipeline execution.
#[derive(Debug)]
pub enum RunOutcome {
    /// Direct route: deterministic answer from scores, no generation.
    Direct {
        scores: Vec<ScoredChoice>,
        route: RouteDecision,
    },
    /// Escalated: the streaming engine generated the reply.
    Escalated {
        route: RouteDecision,
        scores: Vec<ScoredChoice>,
        reply: String,
        report: StreamingReport,
    },
    /// Fail-closed: no generation performed.
    Failed(FailureReason),
}

/// The orchestrator. Owns the budget, the two ports and the policy.
pub struct DualPipeline<'a> {
    budget: Budget,
    decision: &'a dyn DecisionReadout,
    streaming: &'a dyn StreamingEngine,
    policy: RoutePolicy,
    next_run_id: u128,
    next_snapshot_id: u64,
}

impl<'a> DualPipeline<'a> {
    pub fn new(
        budget: Budget,
        decision: &'a dyn DecisionReadout,
        streaming: &'a dyn StreamingEngine,
        policy: RoutePolicy,
    ) -> Self {
        Self {
            budget,
            decision,
            streaming,
            policy,
            next_run_id: 1,
            next_snapshot_id: 1,
        }
    }

    /// Execute one user turn: decision readout → policy route →
    /// (escalate) streaming generation with a NEW snapshot.
    pub fn execute(
        &mut self,
        entries: Vec<ContextEntry>,
        choices: &[&str],
        cancellation: &CancellationToken,
        on_chunk: &mut dyn FnMut(u32, &str),
    ) -> (RunOutcome, EventLedger) {
        let mut ledger = EventLedger::default();
        let mut usage = BudgetUsage::default();

        let decision_run = RunId(self.next_run_id);
        self.next_run_id += 1;
        let snapshot_id = self.next_snapshot_id;
        self.next_snapshot_id += 1;
        let snapshot = ContextSnapshot::new(snapshot_id, unix_now(), entries);

        // Decision phase.
        if usage.record_transition(&self.budget).is_err() {
            ledger.record(PipelineEvent::DecisionFailed {
                run_id: decision_run,
                reason: FailureReason::BudgetExceeded(BudgetError::TransitionsExceeded),
            });
            return (
                RunOutcome::Failed(FailureReason::BudgetExceeded(
                    BudgetError::TransitionsExceeded,
                )),
                ledger,
            );
        }
        if cancellation.is_cancelled() {
            ledger.record(PipelineEvent::DecisionFailed {
                run_id: decision_run,
                reason: FailureReason::Cancelled,
            });
            return (RunOutcome::Failed(FailureReason::Cancelled), ledger);
        }
        ledger.record(PipelineEvent::DecisionStarted {
            run_id: decision_run,
            snapshot_id: snapshot.snapshot_id(),
        });

        if let Err(error) =
            usage.record_input_tokens(snapshot.estimated_input_tokens(), &self.budget)
        {
            let reason = FailureReason::BudgetExceeded(error);
            ledger.record(PipelineEvent::DecisionFailed {
                run_id: decision_run,
                reason,
            });
            return (RunOutcome::Failed(reason), ledger);
        }
        if let Err(error) = usage.record_model_call(&self.budget) {
            let reason = FailureReason::BudgetExceeded(error);
            ledger.record(PipelineEvent::DecisionFailed {
                run_id: decision_run,
                reason,
            });
            return (RunOutcome::Failed(reason), ledger);
        }

        let readout = match self.decision.score(&snapshot, choices, decision_run) {
            Ok(readout) => readout,
            Err(reason) => {
                ledger.record(PipelineEvent::DecisionFailed {
                    run_id: decision_run,
                    reason,
                });
                return (RunOutcome::Failed(reason), ledger);
            }
        };
        if let Err(error) = usage.record_input_tokens(readout.input_tokens, &self.budget) {
            let reason = FailureReason::BudgetExceeded(error);
            ledger.record(PipelineEvent::DecisionFailed {
                run_id: decision_run,
                reason,
            });
            return (RunOutcome::Failed(reason), ledger);
        }

        let route = self.policy.route(&readout.scores);
        ledger.record(PipelineEvent::DecisionScored {
            run_id: decision_run,
            route,
        });

        match route {
            RouteDecision::Direct => (
                RunOutcome::Direct {
                    scores: readout.scores,
                    route,
                },
                ledger,
            ),
            RouteDecision::FailClosed => (RunOutcome::Failed(FailureReason::BackendError), ledger),
            RouteDecision::Escalate => {
                // Escalation builds a NEW immutable snapshot (ADR-043 §6):
                // the decision readout's transient view is never reused.
                let streaming_run = RunId(self.next_run_id);
                self.next_run_id += 1;
                self.next_snapshot_id += 1;
                let streaming_snapshot = ContextSnapshot::new(
                    self.next_snapshot_id - 1,
                    unix_now(),
                    snapshot.entries().to_vec(),
                );
                ledger.record(PipelineEvent::StreamingStarted {
                    run_id: streaming_run,
                    snapshot_id: streaming_snapshot.snapshot_id(),
                });
                if let Err(error) = usage.record_model_call(&self.budget) {
                    let reason = FailureReason::BudgetExceeded(error);
                    ledger.record(PipelineEvent::StreamingFailed {
                        run_id: streaming_run,
                        reason,
                    });
                    return (RunOutcome::Failed(reason), ledger);
                }
                let options = GenerationOptions::default();
                if let Err(error) = usage
                    .record_input_tokens(streaming_snapshot.estimated_input_tokens(), &self.budget)
                {
                    let reason = FailureReason::BudgetExceeded(error);
                    ledger.record(PipelineEvent::StreamingFailed {
                        run_id: streaming_run,
                        reason,
                    });
                    return (RunOutcome::Failed(reason), ledger);
                }

                let mut sequence = 0u32;
                let mut reply = String::new();
                match self.streaming.generate(
                    &streaming_snapshot,
                    &options,
                    streaming_run,
                    cancellation,
                    &mut |seq, text| {
                        sequence = seq;
                        reply.push_str(text);
                        on_chunk(seq, text);
                    },
                ) {
                    Ok(report) => {
                        let _ = sequence;
                        if let Err(error) =
                            usage.record_output_tokens(report.output_tokens, &self.budget)
                        {
                            let reason = FailureReason::BudgetExceeded(error);
                            ledger.record(PipelineEvent::StreamingFailed {
                                run_id: streaming_run,
                                reason,
                            });
                            return (RunOutcome::Failed(reason), ledger);
                        }
                        ledger.record(PipelineEvent::StreamingCompleted {
                            run_id: streaming_run,
                            output_tokens: report.output_tokens,
                        });
                        (
                            RunOutcome::Escalated {
                                route,
                                scores: readout.scores.clone(),
                                reply,
                                report,
                            },
                            ledger,
                        )
                    }
                    Err(FailureReason::Cancelled) => {
                        ledger.record(PipelineEvent::StreamingCancelled {
                            run_id: streaming_run,
                        });
                        (RunOutcome::Failed(FailureReason::Cancelled), ledger)
                    }
                    Err(reason) => {
                        ledger.record(PipelineEvent::StreamingFailed {
                            run_id: streaming_run,
                            reason,
                        });
                        (RunOutcome::Failed(reason), ledger)
                    }
                }
            }
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
