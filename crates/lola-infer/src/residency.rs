//! Pair-based residency planning (ADR-007/ADR-019, `docs/NANO_RIPL.md`).
//!
//! Each device profile declares a *pair* (decision slot + agent slot) in
//! `config/memory.yaml`; the planner verifies the pair fits the RAM budget
//! with kernel headroom and downgrades the agent slot to on-demand when it
//! doesn't. The decision slot is **never** downgraded — a constrained device
//! swaps the agent, never the decision loop. The decision slot is
//! prefill-only (d1 answers in one forward pass), so its KV is transient
//! and never counts against steady-state residency.

use lola_harness::contracts::Modality;
use lola_memory::config::ModelPairRef;
use thiserror::Error;

use crate::config::{ModelEntry, ModelsConfig};
use crate::infer::ModelRole;

/// Headroom reserved for KV cache, feature buffers and the kernel itself.
const KERNEL_HEADROOM_MB: u64 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidencyState {
    Unloaded,
    Loading,
    Warm,
    Busy,
    EvictionPending,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotPlan {
    /// Always in memory.
    Resident,
    /// Loaded on request; warm page-cache reload makes swaps cheap.
    OnDemand,
    /// Not loadable in this build/slice.
    Disabled,
}

impl SlotPlan {
    pub fn name(self) -> &'static str {
        match self {
            Self::Resident => "resident",
            Self::OnDemand => "on-demand",
            Self::Disabled => "disabled",
        }
    }
}

/// One slot's resolved plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotPlanEntry {
    pub model_id: String,
    pub role: ModelRole,
    pub modality: Modality,
    pub est_ram_mb: u64,
    pub plan: SlotPlan,
}

/// The resolved two-slot residency plan for a profile's pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyPlan {
    pub decision: SlotPlanEntry,
    pub agent: SlotPlanEntry,
    /// True when both slots are resident within budget + headroom.
    pub fully_resident: bool,
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("model '{0}' is not in the loadable registry")]
    UnknownModel(String),
    #[error("slot mismatch: '{model}' has role {found}, slot requires {expected}")]
    RoleMismatch {
        model: String,
        expected: &'static str,
        found: &'static str,
    },
}

/// Resolve and plan a profile's pair under its RAM budget.
pub fn plan(
    budget_mb: u64,
    pair: &ModelPairRef,
    config: &ModelsConfig,
) -> Result<ResidencyPlan, PlanError> {
    let decision = slot_entry(&pair.decision, ModelRole::Decision, config)?;
    let agent = slot_entry(&pair.agent, ModelRole::Agent, config)?;

    let total = decision.est_ram_mb + agent.est_ram_mb;
    let fully_resident = budget_mb.saturating_sub(total) >= KERNEL_HEADROOM_MB;

    Ok(ResidencyPlan {
        decision,
        agent: SlotPlanEntry {
            plan: if fully_resident {
                SlotPlan::Resident
            } else {
                SlotPlan::OnDemand
            },
            ..agent
        },
        fully_resident,
    })
}

fn slot_entry(
    model_id: &str,
    expected: ModelRole,
    config: &ModelsConfig,
) -> Result<SlotPlanEntry, PlanError> {
    let entry: &ModelEntry = config
        .find(model_id)
        .ok_or_else(|| PlanError::UnknownModel(model_id.to_string()))?;
    if entry.role != expected {
        return Err(PlanError::RoleMismatch {
            model: model_id.to_string(),
            expected: match expected {
                ModelRole::Decision => "decision",
                ModelRole::Agent => "agent",
                ModelRole::Specialist => "specialist",
            },
            found: match entry.role {
                ModelRole::Decision => "decision",
                ModelRole::Agent => "agent",
                ModelRole::Specialist => "specialist",
            },
        });
    }
    Ok(SlotPlanEntry {
        model_id: entry.model_id.clone(),
        role: entry.role,
        modality: entry.modality,
        est_ram_mb: entry.est_ram_mb,
        // The decision slot is never downgraded; the agent slot's plan is
        // decided by the caller based on budget fit.
        plan: match expected {
            ModelRole::Decision => SlotPlan::Resident,
            _ => SlotPlan::OnDemand,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelsConfig;
    use lola_memory::config::{MemoryConfig, ModelPairRef};

    const MODELS_YAML: &str = include_str!("../../../config/models.yaml");
    const MEMORY_YAML: &str = include_str!("../../../config/memory.yaml");

    fn shipped() -> (ModelsConfig, MemoryConfig) {
        let models = ModelsConfig::parse(MODELS_YAML).expect("models.yaml");
        let memory = MemoryConfig::parse(MEMORY_YAML).expect("memory.yaml");
        (models, memory)
    }

    #[test]
    fn laptop_profile_runs_the_omni_pair_fully_resident() {
        let (models, memory) = shipped();
        let profile = memory.profiles.get("laptop").expect("laptop profile");
        let plan = plan(profile.ram_budget_mb, &profile.pair, &models).expect("plan");
        assert!(plan.fully_resident);
        assert_eq!(plan.decision.model_id, "d1-omni-2b");
        assert_eq!(plan.decision.modality, Modality::Omni);
        assert_eq!(plan.decision.plan, SlotPlan::Resident);
        assert_eq!(plan.agent.model_id, "lfm3-omni-2b");
        assert_eq!(plan.agent.plan, SlotPlan::Resident);
    }

    #[test]
    fn constrained_profile_swaps_the_agent_never_the_decision() {
        let (models, memory) = shipped();
        let profile = memory
            .profiles
            .get("constrained")
            .expect("constrained profile");
        // Text pair: 1100 + 1050 = 2150 MB; 3072 - 2150 = 922 < 1024 headroom
        // → decision resident, agent on-demand (exactly ADR-007's intent).
        let plan = plan(profile.ram_budget_mb, &profile.pair, &models).expect("plan");
        assert!(!plan.fully_resident);
        assert_eq!(plan.decision.model_id, "d1-2b");
        assert_eq!(plan.decision.plan, SlotPlan::Resident);
        assert_eq!(plan.agent.model_id, "lfm3-1.7b");
        assert_eq!(plan.agent.plan, SlotPlan::OnDemand);
    }

    #[test]
    fn role_mismatch_is_rejected() {
        let (models, _) = shipped();
        let inverted = ModelPairRef {
            decision: "lfm3-1.7b".into(), // an agent model in the decision slot
            agent: "d1-2b".into(),
        };
        assert!(matches!(
            plan(8192, &inverted, &models),
            Err(PlanError::RoleMismatch { .. })
        ));
    }

    #[test]
    fn unknown_model_is_rejected() {
        let (models, _) = shipped();
        let ghost = ModelPairRef {
            decision: "does-not-exist".into(),
            agent: "lfm3-1.7b".into(),
        };
        assert!(matches!(
            plan(8192, &ghost, &models),
            Err(PlanError::UnknownModel(_))
        ));
    }
}
