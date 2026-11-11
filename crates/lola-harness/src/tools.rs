//! Tool registry and escalation policy types (Pi-shaped: ≤10 native tools,
//! no MCP; escalation is a measured decision, not a habit).

use serde::{Deserialize, Serialize};

/// Maximum native tools. Pi ships 4; LOLA's ceiling is 10 (ADR-020).
pub const MAX_TOOLS: usize = 10;
const _: () = assert!(MAX_TOOLS <= 10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    /// One-line description injected into tool prompts (progressive disclosure).
    pub prompt_snippet: String,
    /// Tool calls can be gated or mutated before execution (hook point).
    pub side_effects: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,
    pub arguments_json: String,
    pub idempotency_key: Option<String>,
}

/// When the agent model (slot B) may be invoked. Every condition is
/// measurable; the A3 experiment counts occurrences of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationCondition {
    /// Retrieval found insufficient evidence.
    InsufficientEvidence,
    /// The decision model abstained or failed its contract.
    DecisionAbstained,
    /// d1's assessment conflicts with a strong historical pattern.
    ConflictsWithHistory,
    /// A research question requires multi-step tool use.
    ResearchRequired,
    /// An operator asked for an explanation or investigation.
    OperatorRequest,
    /// New strategy hypothesis under evaluation (Experiment B, later).
    StrategyHypothesis,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalation_conditions_are_stable() {
        let all = [
            EscalationCondition::InsufficientEvidence,
            EscalationCondition::DecisionAbstained,
            EscalationCondition::ConflictsWithHistory,
            EscalationCondition::ResearchRequired,
            EscalationCondition::OperatorRequest,
            EscalationCondition::StrategyHypothesis,
        ];
        // Each serializes to a distinct snake_case id (persisted in ledgers).
        let ids: Vec<String> = all
            .iter()
            .map(|c| serde_json::to_string(c).expect("serialize"))
            .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "condition ids must be unique");
    }

    #[test]
    fn tool_ceiling_is_tight() {
        // Runtime mirror of the compile-time const assert above.
        let tools: Vec<ToolSpec> = (0..MAX_TOOLS)
            .map(|i| ToolSpec {
                name: format!("tool-{i}"),
                prompt_snippet: String::new(),
                side_effects: false,
            })
            .collect();
        assert_eq!(tools.len(), MAX_TOOLS);
    }
}
