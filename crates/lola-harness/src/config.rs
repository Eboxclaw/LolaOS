//! Kernel configuration types for `config/risk.yaml` and `config/tools.yaml`.

use serde::Deserialize;

use crate::error::Result;
use crate::tools::{SafetyTier, ToolCategory};

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

// ---------------------------------------------------------------------------
// tools.yaml (ADR-033)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ToolsConfig {
    pub categories: Vec<CategoryConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CategoryConfig {
    pub category: ToolCategory,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Tightens every tool in the category (never loosens below the
    /// category default).
    pub safety_override: Option<SafetyTier>,
    pub tools: Vec<ToolConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    pub safety: Option<SafetyTier>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl ToolsConfig {
    pub fn parse(yaml: &str) -> Result<Self> {
        Ok(serde_yaml::from_str(yaml)?)
    }

    /// Total tools across enabled categories.
    pub fn tool_count(&self) -> usize {
        self.categories
            .iter()
            .filter(|c| c.enabled)
            .map(|c| c.tools.len())
            .sum()
    }

    /// Enabled categories, in file order.
    pub fn enabled_categories(&self) -> Vec<&CategoryConfig> {
        self.categories.iter().filter(|c| c.enabled).collect()
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

    const TOOLS_YAML: &str = include_str!("../../../config/tools.yaml");

    #[test]
    fn shipped_tools_yaml_parses() {
        use crate::tools::SafetyTier;

        let config = ToolsConfig::parse(TOOLS_YAML).expect("parse tools.yaml");
        assert_eq!(config.categories.len(), 11);
        assert!(config.tool_count() >= 18, "initial registry populated");

        let accessibility = config
            .categories
            .iter()
            .find(|c| c.category == ToolCategory::Accessibility)
            .expect("accessibility category present");
        assert!(!accessibility.enabled, "ADR-034: accessibility disabled");
        assert!(accessibility.tools.is_empty(), "ADR-034: no tools in v0.1");

        // Read-only default + confirm-on-write posture is visible in the file.
        let calendar_writes = config
            .categories
            .iter()
            .find(|c| c.category == ToolCategory::Calendar)
            .and_then(|c| c.tools.iter().find(|t| t.name == "calendar_create_event"))
            .expect("calendar_create_event present");
        assert_eq!(calendar_writes.safety, Some(SafetyTier::ConfirmOnWrite));
    }
}
