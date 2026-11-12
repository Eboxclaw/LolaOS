//! Categorized tool registry (ADR-033, `docs/TOOLS.md`).
//!
//! Two sizes matter and they are different:
//! - the **registry** may be large (categories grow over time);
//! - the **prompt-injected set** stays tiny (≤ [`MAX_PROMPT_TOOLS`]) — past
//!   that, the agent gets `search_tools(query)` instead (Tool Search
//!   pattern, ADR-028/032).
//!
//! Safety posture (ADR-033): native Android tools are **read-only by
//! default**; any write is confirm-on-write (agent proposes → human
//! confirms in the UI). The accessibility category is hard-gated and ships
//! with zero tools in v0.1 (ADR-034).

use serde::{Deserialize, Serialize};

/// Maximum tools injected into a prompt per turn. The registry is not
/// capped by this — only what the model sees.
pub const MAX_PROMPT_TOOLS: usize = 10;
const _: () = assert!(MAX_PROMPT_TOOLS <= 10);

/// Capability categories. Stable snake_case ids — they persist in configs,
/// ledgers and skill manifests, so they are append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    /// Calendar provider: events, reminders.
    Calendar,
    /// Clock, alarms, timers.
    TimeAlarm,
    /// Notification reading (special access) and interactions.
    Notifications,
    /// Battery, torch, screen, network state.
    Device,
    /// User-driven file access (SAF).
    Files,
    /// Microphone capture (feeds the omni path).
    Media,
    /// App launching and app info.
    Apps,
    /// Other-app screen reading/actuation. Hard-gated; empty in v0.1 (ADR-034).
    Accessibility,
    /// Core-side: graph, FTS, ledger, case bank.
    Memory,
    /// Core-side: market data, backtests.
    Market,
    /// Core-side: support passes, kill switch.
    System,
}

impl ToolCategory {
    pub const ALL: [ToolCategory; 11] = [
        Self::Calendar,
        Self::TimeAlarm,
        Self::Notifications,
        Self::Device,
        Self::Files,
        Self::Media,
        Self::Apps,
        Self::Accessibility,
        Self::Memory,
        Self::Market,
        Self::System,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Calendar => "calendar",
            Self::TimeAlarm => "time_alarm",
            Self::Notifications => "notifications",
            Self::Device => "device",
            Self::Files => "files",
            Self::Media => "media",
            Self::Apps => "apps",
            Self::Accessibility => "accessibility",
            Self::Memory => "memory",
            Self::Market => "market",
            Self::System => "system",
        }
    }

    /// Category-level safety default (ADR-033): read-only unless a tool is
    /// explicitly riskier. Write tools declare `ConfirmOnWrite` themselves.
    pub fn default_safety(self) -> SafetyTier {
        match self {
            Self::Accessibility => SafetyTier::Gated,
            Self::System => SafetyTier::Gated,
            _ => SafetyTier::ReadOnly,
        }
    }

    /// Core-side categories execute inside the Rust process; the rest bridge
    /// to Kotlin over UniFFI (`docs/TOOLS.md` §3).
    pub fn default_side(self) -> ToolSide {
        match self {
            Self::Memory | Self::Market | Self::System => ToolSide::Core,
            _ => ToolSide::AndroidBridge,
        }
    }
}

/// Where a tool executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSide {
    /// Rust-resident (ledger, graph, backtest…).
    Core,
    /// Kotlin-side via the UniFFI tool-call bridge (native Android APIs).
    AndroidBridge,
}

/// Safety tiers, ordered by restriction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyTier {
    /// No side effects; results still enter context only via the support gate.
    ReadOnly,
    /// Agent proposes; a human confirms in the UI before execution.
    ConfirmOnWrite,
    /// Requires an explicit policy grant + per-call review (accessibility,
    /// kill switch).
    Gated,
    /// Present in the taxonomy but turned off.
    Disabled,
}

/// One registered tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Stable id, e.g. `calendar_create_event`.
    pub name: String,
    pub category: ToolCategory,
    pub side: ToolSide,
    pub safety: SafetyTier,
    /// Android permission strings this tool needs (Kotlin side requests).
    pub permissions: Vec<String>,
    /// One-line description for prompt injection (progressive disclosure).
    pub prompt_snippet: String,
}

impl ToolSpec {
    /// May this tool's result enter the prompt without a support-pass gate?
    /// (Never — ADR-032 — but read-only tools may be *listed* freely.)
    pub fn promptable(&self) -> bool {
        self.safety != SafetyTier::Disabled
    }
}

/// A tool call request (crosses the UniFFI bridge for `AndroidBridge` tools).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,
    pub arguments_json: String,
    /// Side-effecting calls carry one; results are deduplicated by it.
    pub idempotency_key: Option<String>,
}

/// Escalation conditions stay as-is (see `docs/HARNESS.md` §5) — kept here
/// so the tool layer owns the full agent-facing surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationCondition {
    InsufficientEvidence,
    DecisionAbstained,
    ConflictsWithHistory,
    ResearchRequired,
    OperatorRequest,
    StrategyHypothesis,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_ids_are_stable_and_unique() {
        let ids: Vec<&str> = ToolCategory::ALL.iter().map(|c| c.id()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "category ids must be unique");
        assert_eq!(ToolCategory::TimeAlarm.id(), "time_alarm");
        assert_eq!(ToolCategory::Accessibility.id(), "accessibility");
    }

    #[test]
    fn safety_defaults_follow_adr_033() {
        // Native categories: read-only default.
        assert_eq!(
            ToolCategory::Calendar.default_safety(),
            SafetyTier::ReadOnly
        );
        assert_eq!(
            ToolCategory::Notifications.default_safety(),
            SafetyTier::ReadOnly
        );
        // Hard-gated categories.
        assert_eq!(
            ToolCategory::Accessibility.default_safety(),
            SafetyTier::Gated
        );
        assert_eq!(ToolCategory::System.default_safety(), SafetyTier::Gated);
    }

    #[test]
    fn core_vs_bridge_split() {
        assert_eq!(ToolCategory::Memory.default_side(), ToolSide::Core);
        assert_eq!(
            ToolCategory::Calendar.default_side(),
            ToolSide::AndroidBridge
        );
    }

    #[test]
    fn disabled_tools_are_not_promptable() {
        let tool = ToolSpec {
            name: "screen_read".into(),
            category: ToolCategory::Accessibility,
            side: ToolSide::AndroidBridge,
            safety: SafetyTier::Disabled,
            permissions: vec![],
            prompt_snippet: String::new(),
        };
        assert!(!tool.promptable());
    }

    #[test]
    fn escalation_conditions_are_stable() {
        let ids: Vec<String> = [EscalationCondition::InsufficientEvidence]
            .iter()
            .map(|c| serde_json::to_string(c).expect("serialize"))
            .collect();
        assert_eq!(ids[0], "\"insufficient_evidence\"");
    }
}
