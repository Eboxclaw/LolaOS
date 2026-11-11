//! `config/memory.yaml` — profiles (each declaring a model *pair*), TTL
//! policy (ADR-009), and the memory-tier budget knobs.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::{LolaMemoryError, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryConfig {
    pub active_profile: String,
    pub profiles: BTreeMap<String, MemoryProfile>,
    pub ttl: TtlPolicy,
}

/// A device profile declares which model *pair* it hosts (ADR-019):
/// laptop runs the omni pair; constrained devices fall back to the text
/// pair behind the same two slots.
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryProfile {
    pub description: String,
    /// Hard RAM budget for the inference layer.
    pub ram_budget_mb: u64,
    pub pair: ModelPairRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModelPairRef {
    pub decision: String,
    pub agent: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TtlPolicy {
    /// Days before bulky model request/response payloads compact to blobs.
    pub model_payloads_days: u64,
    /// Days before bulky session payloads compact to blobs.
    pub session_bulk_days: u64,
    /// Days before feature snapshot payloads compact to blobs.
    pub feature_snapshots_days: u64,
    /// Data classes never compacted or deleted.
    pub immutable: Vec<String>,
}

impl MemoryConfig {
    pub fn parse(yaml: &str) -> Result<Self> {
        Ok(serde_yaml::from_str(yaml)?)
    }

    pub fn profile(&self) -> Result<&MemoryProfile> {
        self.profiles.get(&self.active_profile).ok_or_else(|| {
            LolaMemoryError::Config(format!("unknown memory profile '{}'", self.active_profile))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMORY_YAML: &str = include_str!("../../../config/memory.yaml");

    #[test]
    fn shipped_memory_yaml_parses() {
        let config = MemoryConfig::parse(MEMORY_YAML).expect("parse memory.yaml");
        let profile = config.profile().expect("active profile");
        assert!(profile.ram_budget_mb > 0);
        assert!(!profile.pair.decision.is_empty());
        assert!(!profile.pair.agent.is_empty());
        assert!(config.ttl.immutable.contains(&"decision_runs".to_string()));
    }

    #[test]
    fn unknown_profile_is_an_error() {
        let config = MemoryConfig::parse(MEMORY_YAML).expect("parse memory.yaml");
        let broken = MemoryConfig {
            active_profile: "nonexistent".into(),
            ..config
        };
        assert!(broken.profile().is_err());
    }
}
