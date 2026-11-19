//! Independent run lifecycles and cooperative cancellation.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunId(pub u128);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineKind {
    Decision,
    Streaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionParadigm {
    Direct,
    FiniteStateMachine,
    DirectedAcyclicGraph,
    HierarchicalTaskNetwork,
    GoalOrientedActionPlanning,
    BoundedReAct,
    SearchDeliberation,
    ReflectionRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Created,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    InvalidTransition { from: RunState, to: RunState },
}

#[derive(Debug)]
pub struct PipelineRun {
    id: RunId,
    kind: PipelineKind,
    state: RunState,
}

impl PipelineRun {
    pub fn new(id: RunId, kind: PipelineKind) -> Self {
        Self {
            id,
            kind,
            state: RunState::Created,
        }
    }

    pub fn id(&self) -> RunId {
        self.id
    }

    pub fn kind(&self) -> PipelineKind {
        self.kind
    }

    pub fn state(&self) -> RunState {
        self.state
    }

    pub fn start(&mut self) -> Result<(), TransitionError> {
        self.transition(RunState::Running)
    }

    pub fn complete(&mut self) -> Result<(), TransitionError> {
        self.transition(RunState::Completed)
    }

    pub fn fail(&mut self) -> Result<(), TransitionError> {
        self.transition(RunState::Failed)
    }

    pub fn cancel(&mut self) -> Result<(), TransitionError> {
        self.transition(RunState::Cancelled)
    }

    fn transition(&mut self, to: RunState) -> Result<(), TransitionError> {
        let allowed = matches!(
            (self.state, to),
            (RunState::Created, RunState::Running)
                | (RunState::Created, RunState::Cancelled)
                | (RunState::Running, RunState::Completed)
                | (RunState::Running, RunState::Failed)
                | (RunState::Running, RunState::Cancelled)
        );
        if !allowed {
            return Err(TransitionError::InvalidTransition {
                from: self.state,
                to,
            });
        }
        self.state = to;
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
