//! Kernel configuration types for `config/memory.yaml` and `config/risk.yaml`.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::{LolaError, Result};

// ---------------------------------------------------------------------------
// memory.yaml
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryConfig {
    pub active_profile: String,
    pub profiles: BTreeMap<String, MemoryProfile>,
    pub ttl: TtlPolicy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryProfile {
    pub description: String,
    /// Hard RAM budget for the inference layer; residency plans respect it.
    pub ram_budget_mb: u64,
    /// Model ids held resident at all times.
    pub resident: Vec<String>,
    /// Model ids swapped in on demand (warm page-cache reload).
    pub on_demand: Vec<String>,
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
            LolaError::Config(format!("unknown memory profile '{}'", self.active_profile))
        })
    }
}

// ---------------------------------------------------------------------------
// risk.yaml
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    /// When true, missing or stale information blocks the order (fail closed).
    pub fail_closed: bool,
    pub testnet: TestnetRisk,
    pub mainnet: MainnetRisk,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestnetRisk {
    pub max_leverage: u32,
    /// Canonical decimal string; USD-equivalent notional cap per instrument.
    pub notional_cap: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MainnetRisk {
    /// Every mainnet deployment starts here, regardless of ladder state.
    pub initial_leverage: u32,
    pub max_leverage: u32,
    pub ladder: Vec<LadderRung>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LadderRung {
    pub leverage: u32,
    /// Evidence requirements, evaluated offline by humans — never by models.
    pub unlock_requires: Vec<String>,
}

impl RiskConfig {
    pub fn parse(yaml: &str) -> Result<Self> {
        Ok(serde_yaml::from_str(yaml)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMORY_YAML: &str = include_str!("../../../config/memory.yaml");
    const RISK_YAML: &str = include_str!("../../../config/risk.yaml");

    #[test]
    fn shipped_memory_yaml_parses() {
        let config = MemoryConfig::parse(MEMORY_YAML).expect("parse memory.yaml");
        let profile = config.profile().expect("active profile");
        assert!(profile.resident.contains(&"d1-2b".to_string()));
        assert!(profile.ram_budget_mb > 0);
        assert!(config.ttl.immutable.contains(&"decision_runs".to_string()));
    }

    #[test]
    fn shipped_risk_yaml_parses() {
        let config = RiskConfig::parse(RISK_YAML).expect("parse risk.yaml");
        assert!(config.fail_closed);
        assert_eq!(config.testnet.max_leverage, 10);
        assert_eq!(config.mainnet.initial_leverage, 1);
        assert!(config.mainnet.ladder.iter().any(|r| r.leverage == 10));
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
