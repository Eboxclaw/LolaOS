//! Backend-neutral inference contracts for LOLA ACE.
//!
//! This API deliberately contains no trading or Android-specific types.
//! Implementations must keep mutable model state inside an inference session.

use std::{any::Any, ops::ControlFlow};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BackendId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub u128);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunId(pub u128);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub id: ModelId,
    pub family: String,
    pub revision: String,
    pub artifact_sha256: Option<String>,
    pub context_length: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackendCapabilities {
    pub streaming: bool,
    pub cancellation: bool,
    pub concurrent_sessions: bool,
    pub vision: bool,
    pub audio: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenerationOptions {
    pub max_output_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub seed: Option<u64>,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            max_output_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            seed: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamChunk {
    pub run_id: RunId,
    pub sequence: u64,
    pub token_ids: Vec<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    EndOfSequence,
    StopSequence,
    MaxOutputTokens,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamControl {
    Continue,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationReport {
    pub run_id: RunId,
    pub generated_tokens: u32,
    pub finish_reason: FinishReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceError {
    InvalidModel(String),
    UnsupportedCapability(String),
    InvalidSession,
    OutOfMemory,
    Cancelled,
    DeadlineExceeded,
    BackendFailure(String),
}

pub struct InferenceSession {
    pub id: SessionId,
    pub model_id: ModelId,
    state: Box<dyn Any + Send>,
}

impl InferenceSession {
    pub fn new<T: Any + Send>(id: SessionId, model_id: ModelId, state: T) -> Self {
        Self {
            id,
            model_id,
            state: Box::new(state),
        }
    }

    pub fn state_mut<T: Any + Send>(&mut self) -> Option<&mut T> {
        self.state.downcast_mut::<T>()
    }
}

pub trait InferenceBackend: Send {
    fn id(&self) -> BackendId;
    fn capabilities(&self, model: &ModelDescriptor) -> BackendCapabilities;
    fn load_model(&mut self, model: &ModelDescriptor) -> Result<(), InferenceError>;
    fn unload_model(&mut self, model_id: &ModelId) -> Result<(), InferenceError>;
    fn create_session(
        &self,
        model: &ModelDescriptor,
        session_id: SessionId,
    ) -> Result<InferenceSession, InferenceError>;

    fn generate(
        &self,
        session: &mut InferenceSession,
        run_id: RunId,
        input_tokens: &[u32],
        options: &GenerationOptions,
        on_chunk: &mut dyn FnMut(StreamChunk) -> ControlFlow<()>,
    ) -> Result<GenerationReport, InferenceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct FakeState {
        counter: u32,
    }

    #[test]
    fn inference_sessions_own_independent_mutable_state() {
        let model = ModelId("lfm3-1.7b".into());
        let mut first = InferenceSession::new(SessionId(1), model.clone(), FakeState::default());
        let mut second = InferenceSession::new(SessionId(2), model, FakeState::default());
        first.state_mut::<FakeState>().unwrap().counter = 7;
        assert_eq!(first.state_mut::<FakeState>().unwrap().counter, 7);
        assert_eq!(second.state_mut::<FakeState>().unwrap().counter, 0);
    }

    #[test]
    fn generation_defaults_are_bounded() {
        let options = GenerationOptions::default();
        assert_eq!(options.max_output_tokens, 256);
    }
}
