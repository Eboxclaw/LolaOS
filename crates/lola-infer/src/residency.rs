//! Profile-based residency planning (ADR-007, `docs/NANO_RIPL.md`).
//!
//! `config/memory.yaml` declares the intent (`resident` / `on_demand` per
//! profile); the planner verifies the intent fits the RAM budget with
//! headroom and downgrades agent/specialist slots to on-demand when it
//! doesn't. Decision models stay resident in every plan — a constrained
//! device swaps the agent, never the decision loop.

use crate::config::ModelsConfig;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyPlan {
    pub slots: Vec<(String, SlotPlan)>,
}

impl ResidencyPlan {
    pub fn slot_for(&self, model_id: &str) -> Option<SlotPlan> {
        self.slots
            .iter()
            .find(|(id, _)| id == model_id)
            .map(|(_, plan)| *plan)
    }
}

/// Plan residency for the active models under a RAM budget.
pub fn plan(budget_mb: u64, config: &ModelsConfig) -> ResidencyPlan {
    let total: u64 = config.active.iter().map(|entry| entry.est_ram_mb).sum();
    let both_fit = budget_mb.saturating_sub(total) >= KERNEL_HEADROOM_MB;

    let mut slots = Vec::with_capacity(config.active.len());
    for entry in &config.active {
        let slot = match entry.role {
            ModelRole::Decision => SlotPlan::Resident,
            ModelRole::Agent | ModelRole::Specialist => {
                if both_fit {
                    SlotPlan::Resident
                } else {
                    SlotPlan::OnDemand
                }
            }
        };
        slots.push((entry.model_id.clone(), slot));
    }
    ResidencyPlan { slots }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infer::ModelRole;

    fn config() -> ModelsConfig {
        ModelsConfig {
            active: vec![
                crate::config::ModelEntry {
                    model_id: "d1-2b".into(),
                    repository: "LiquidAI/d1-2B".into(),
                    role: ModelRole::Decision,
                    quantization: "q4_k_m".into(),
                    backend: "reference-llamacpp".into(),
                    context_limit: 131072,
                    est_ram_mb: 1400,
                },
                crate::config::ModelEntry {
                    model_id: "lfm3-1.7b".into(),
                    repository: "LiquidAI/LFM3-1.7B".into(),
                    role: ModelRole::Agent,
                    quantization: "q4_k_m".into(),
                    backend: "reference-llamacpp".into(),
                    context_limit: 262144,
                    est_ram_mb: 1200,
                },
            ],
            registered: vec![],
        }
    }

    #[test]
    fn laptop_budget_keeps_both_models_resident() {
        let plan = plan(8192, &config());
        assert_eq!(plan.slot_for("d1-2b"), Some(SlotPlan::Resident));
        assert_eq!(plan.slot_for("lfm3-1.7b"), Some(SlotPlan::Resident));
    }

    #[test]
    fn constrained_budget_keeps_decision_model_swaps_agent() {
        let plan = plan(3072, &config());
        assert_eq!(plan.slot_for("d1-2b"), Some(SlotPlan::Resident));
        assert_eq!(plan.slot_for("lfm3-1.7b"), Some(SlotPlan::OnDemand));
    }

    #[test]
    fn decision_model_is_never_downgraded() {
        // Even under an absurdly tight budget the decision slot stays resident;
        // startup must refuse to boot a plan without it (checked by callers).
        let plan = plan(1024, &config());
        assert_eq!(plan.slot_for("d1-2b"), Some(SlotPlan::Resident));
        assert_eq!(plan.slot_for("lfm3-1.7b"), Some(SlotPlan::OnDemand));
    }
}
