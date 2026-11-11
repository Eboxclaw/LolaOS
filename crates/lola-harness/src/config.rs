//! `config/risk.yaml` — deterministic risk policy (ADR-008/ADR-012).
//!
//! The engine hard-clamps whatever this file says. Model confidence can
//! never raise a cap. Missing or stale information fails closed.

use serde::Deserialize;

use crate::error::Result;

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

    const RISK_YAML: &str = include_str!("../../../config/risk.yaml");

    #[test]
    fn shipped_risk_yaml_parses() {
        let config = RiskConfig::parse(RISK_YAML).expect("parse risk.yaml");
        assert!(config.fail_closed);
        assert_eq!(config.testnet.max_leverage, 10);
        assert_eq!(config.mainnet.initial_leverage, 1);
        assert!(config.mainnet.ladder.iter().any(|r| r.leverage == 10));
    }
}
