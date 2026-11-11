//! Kernel event types and a minimal synchronous bus.
//!
//! S1 replaces this with the real scheduler; the shape is fixed here so the
//! persistence layer and metrics can be built against it first.

use std::sync::mpsc::{channel, Receiver, Sender};

/// Events on the kernel's main loop. Every variant maps to a session event
/// row (`session_events.event_type`) when persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LolaEvent {
    BarClosed {
        instrument_id: String,
        timeframe: String,
        event_time_ns: i64,
    },
    StateSignatureComputed {
        instrument_id: String,
        signature: String,
        event_time_ns: i64,
    },
    DecisionRecorded {
        decision_id: String,
    },
    RiskVerdict {
        decision_id: String,
        accepted: bool,
    },
    OrderIntentCreated {
        decision_id: String,
    },
    PaperOrderFilled {
        decision_id: String,
        fill_price: String,
    },
    OutcomeMeasured {
        decision_id: String,
        horizon_bars: u32,
    },
}

impl LolaEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::BarClosed { .. } => "bar_closed",
            Self::StateSignatureComputed { .. } => "state_signature_computed",
            Self::DecisionRecorded { .. } => "decision_recorded",
            Self::RiskVerdict { .. } => "risk_verdict",
            Self::OrderIntentCreated { .. } => "order_intent_created",
            Self::PaperOrderFilled { .. } => "paper_order_filled",
            Self::OutcomeMeasured { .. } => "outcome_measured",
        }
    }
}

/// Minimal synchronous publish/subscribe skeleton.
pub struct EventBus {
    sender: Sender<LolaEvent>,
}

impl EventBus {
    /// Create a bus and its single consumer handle.
    pub fn new() -> (Self, Receiver<LolaEvent>) {
        let (sender, receiver) = channel();
        (Self { sender }, receiver)
    }

    /// Publish an event. Never blocks; a dropped consumer discards silently.
    pub fn publish(&self, event: LolaEvent) {
        let _ = self.sender.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_flow_through_the_bus() {
        let (bus, receiver) = EventBus::new();
        let event = LolaEvent::BarClosed {
            instrument_id: "HYPERLIQUID:BTC-PERP".into(),
            timeframe: "5m".into(),
            event_time_ns: 42,
        };
        bus.publish(event.clone());
        assert_eq!(receiver.recv().expect("event"), event);
    }

    #[test]
    fn event_names_are_stable_identifiers() {
        let event = LolaEvent::OutcomeMeasured {
            decision_id: "d".into(),
            horizon_bars: 12,
        };
        assert_eq!(event.name(), "outcome_measured");
    }
}
