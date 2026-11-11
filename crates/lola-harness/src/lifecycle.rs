//! Android-first lifecycle types (ADR-025, `docs/MOBILE_FIRST.md`).
//!
//! The phone runs the trading loop. Android will kill us — the 6-hour
//! `dataSync`/`mediaProcessing` FGS caps (Android 15+), thermal headroom,
//! memory pressure. The harness therefore treats checkpoint/resume as the
//! normal path, never the error path: the ledger already holds everything
//! needed to resume an interrupted tick.

use serde::{Deserialize, Serialize};

/// Android 15+ caps `dataSync`/`mediaProcessing` foreground services at
/// ~6 cumulative hours per 24h. We never assume an immortal process.
pub const FGS_DATASYNC_CAP_HOURS: u32 = 6;
/// Below this much thermal headroom (°C), suspend before Android suspends us.
pub const THERMAL_SUSPEND_HEADROOM_C: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FgsType {
    DataSync,
    MediaProcessing,
    SpecialUse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    /// User is interacting: chat/voice bursts are fair game.
    ForegroundActivity,
    /// Checkpointed loop keeps running under a typed foreground service.
    ForegroundService { kind: FgsType },
    /// Periodic decision tick without a service (quota-aware).
    WorkManagerJob,
}

/// What one iteration of the decision loop wants to do. (No `Eq`: thermal
/// headroom is a float.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopTick {
    /// d1 prefill burst over the current closed bars (milliseconds).
    DecisionBurst { instruments: Vec<String> },
    /// Nothing to do; extend the checkpoint heartbeat.
    IdleCheckpoint,
    /// Voluntarily quiesce: checkpoint the ledger, release what we can.
    Suspend { reason: SuspendReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuspendReason {
    /// The FGS budget is nearly spent — resume via WorkManager.
    FgsCapApproaching,
    /// Thermal headroom below [`THERMAL_SUSPEND_HEADROOM_C`].
    ThermalHeadroomLow { remaining_c: f64 },
    /// lmkd pressure / Android-17 memory limiter signals.
    MemoryPressure,
    /// User left the app; no active session justifies the service.
    UserBackgrounded,
}

/// What a checkpoint guarantees: resume reads these and continues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointIntent {
    pub last_decision_id: Option<String>,
    pub ledger_sequence: u64,
    /// Open slots evicted (slot A is transient by construction — ADR-024).
    pub slot_b_kv_saved: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fgs_cap_is_six_hours() {
        assert_eq!(FGS_DATASYNC_CAP_HOURS, 6);
    }

    #[test]
    fn suspend_reasons_serialize_stably() {
        let json = serde_json::to_string(&SuspendReason::ThermalHeadroomLow { remaining_c: 4.2 })
            .expect("serialize");
        assert!(json.contains("thermal_headroom_low"));
        let back: SuspendReason = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, SuspendReason::ThermalHeadroomLow { remaining_c: 4.2 });
    }

    #[test]
    fn checkpoints_resume_from_the_ledger() {
        let checkpoint = CheckpointIntent {
            last_decision_id: Some("d-42".into()),
            ledger_sequence: 1337,
            slot_b_kv_saved: false,
        };
        assert_eq!(checkpoint.ledger_sequence, 1337);
    }
}
