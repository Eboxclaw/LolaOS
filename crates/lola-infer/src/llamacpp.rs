//! The real oracle backend: llama.cpp (≥ b11960) linked **in-process**
//! (S2.b, ADR-003/023). Nothing of it ships in LOLA — it exists to
//! produce golden outputs and to sit behind per-kernel parity gates
//! while `lola-custom` (S3) lands.
//!
//! **Status: scaffold.** The `llama-cpp-2` dependency is intentionally NOT
//! declared in `Cargo.toml` yet — adding an optional dependency still
//! resolves it into the lockfile, and the version must be pinned to a
//! build of llama.cpp v0.8.0+ (LFM3 floor). When S2.b begins:
//! 1. `cargo add llama-cpp-2 --optional` pinned to a revision built from
//!    llama.cpp ≥ b11960 (verify LFM3-Omni support in its changelog);
//! 2. add `[features] oracle-llamacpp = ["dep:llama-cpp-2"]`;
//! 3. gate this module with `#[cfg(feature = "oracle-llamacpp")]`;
//! 4. implement `LlamaCppOracle` against the `Infer` trait per the plan
//!    below and record the real golden set (S2.c flips provenance).

use serde::{Deserialize, Serialize};

/// Loader parameters mirroring the llama.cpp surfaces we rely on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OracleConfig {
    /// GGUF file (interchange format — the oracle never sees the lola layout).
    pub model_path: String,
    /// mmproj sidecar for omni checkpoints.
    pub mmproj_path: Option<String>,
    /// Context window for decision prompts (small: state + ≤16 questions).
    pub context_tokens: u32,
    /// Prefill batch sizing (verified guidance: `-b 8192 -ub 8192`).
    pub batch_tokens: u32,
    /// Threads: `activeProcessorCount - 2` per Liquid's mobile guidance.
    pub threads: u32,
    /// Non-reasoning omni models: reasoning off, deterministic decode.
    pub reasoning_off: bool,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            mmproj_path: None,
            context_tokens: 8192,
            batch_tokens: 8192,
            threads: 4,
            reasoning_off: true,
        }
    }
}

/// Implementation plan (kept here so S2.b starts from the design):
///
/// 1. **Load**: `llama_model_load_from_file` with mmap; record
///    load-duration + resident-RSS deltas (MOBILE_FIRST instrumentation).
/// 2. **decide()**: render the question set from the active router schema
///    (`router.yaml`) into the d1 `system_one`-style prompt; one forward
///    pass (`llama_decode`, no sampling loop — output_tokens = 0); parse
///    noul/choice/score into `DecisionV1`; malformed output ⇒
///    `InferError::Contract` (fail closed, never repair).
/// 3. **golden recording**: run the shipped golden-set requests, persist
///    answers with `provenance: "oracle-llamacpp@<rev>"` (S2.c).
/// 4. **The parity rule**: `lola-custom` kernels gate against this
///    backend per `docs/ENGINE.md` §4 — no kernel serves before its gate.
///
/// What this backend must NOT do: anything on the candle-critical path,
/// anything in the shipped app (dev-time only), any weight mutation.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_defaults_match_verified_guidance() {
        let config = OracleConfig::default();
        assert_eq!(config.batch_tokens, 8192, "verified -b/-ub guidance");
        assert!(config.reasoning_off, "omni is non-reasoning");
        assert!(config.context_tokens >= config.batch_tokens);
    }
}
