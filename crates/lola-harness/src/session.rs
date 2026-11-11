//! Durable session event tree (Pi model, ADR-021; Hermes archive semantics).
//!
//! A session is an append-only tree of typed entries linked by
//! `event_id`/`parent_event_id`; the current leaf pins the active branch.
//! Branching, crash-safe durability and provenance fall out of the shape.
//!
//! ## The three cache-stability laws
//! On-device, a rewritten prefix is real prefill compute. Therefore:
//! 1. **No mid-session memory injection** — curated memory is snapshotted at
//!    session start (`sessions.memory_snapshot_hash`) and frozen for the
//!    session's life.
//! 2. **No mid-turn model swap** — a slot swap ends the turn first.
//! 3. **No history mutation** — corrections are `ContextEdit` entries that
//!    reshape future context; the original rows are never rewritten.

use serde::{Deserialize, Serialize};

use lola_memory::compaction::CompactionRecord;

use crate::contracts::Modality;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    Tool,
    System,
}

/// Entry taxonomy — persisted as `session_events.event_type` (stable ids).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    Message,
    ModelChange,
    Compaction,
    ContextEdit,
    Custom,
    Media,
}

/// A tree node: one durable session entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEventNode {
    pub event_id: String,
    pub parent_event_id: Option<String>,
    pub sequence_no: u64,
    pub event_time_ns: i64,
    pub entry: Entry,
    /// sha256 of the payload; rows survive blob TTL compaction.
    pub payload_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Entry {
    /// A conversation turn. Assistant messages record the serving model.
    Message(MessageEntry),
    /// Slot A/B swap — ends the current turn first (cache-stability law 2).
    ModelChange(ModelChangeEntry),
    /// Archive-with-witnesses compaction (drives the KV-side `seq_rm` twin).
    Compaction(CompactionRecord),
    /// Append-only override reshaping future context; never rewrites history.
    ContextEdit(ContextEditEntry),
    /// Harness/extension state not destined for any model context.
    Custom(CustomEntry),
    /// Omni media (audio clip, frame, video slice) as a content-addressed ref.
    Media(MediaEntry),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEntry {
    pub role: Role,
    pub model_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChangeEntry {
    pub slot: ModelSlot,
    pub from_model: String,
    pub to_model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSlot {
    /// Decision slot (d1 family): prefill-only, transient KV.
    Decision,
    /// Agent slot (LFM3 family): streaming decode, persistent KV.
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEditEntry {
    pub target_event_id: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomEntry {
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaEntry {
    pub modality: Modality,
    /// Content-addressed blob reference (`blob:<sha256>`).
    pub media_uri: String,
    /// Duration ns for audio/video; frame count for video; 0 otherwise.
    pub extent_ns: i64,
}

impl SessionEventNode {
    pub fn entry_type(&self) -> EntryType {
        match &self.entry {
            Entry::Message(_) => EntryType::Message,
            Entry::ModelChange(_) => EntryType::ModelChange,
            Entry::Compaction(_) => EntryType::Compaction,
            Entry::ContextEdit(_) => EntryType::ContextEdit,
            Entry::Custom(_) => EntryType::Custom,
            Entry::Media(_) => EntryType::Media,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(entry: Entry) -> SessionEventNode {
        SessionEventNode {
            event_id: "ev-1".into(),
            parent_event_id: None,
            sequence_no: 1,
            event_time_ns: 42,
            entry,
            payload_hash: "deadbeef".into(),
        }
    }

    #[test]
    fn entry_types_map_to_stable_ids() {
        assert_eq!(
            node(Entry::Media(MediaEntry {
                modality: Modality::Audio,
                media_uri: "blob:abc".into(),
                extent_ns: 960_000_000,
            }))
            .entry_type(),
            EntryType::Media
        );
        assert_eq!(
            node(Entry::Compaction(CompactionRecord {
                summary: String::new(),
                first_kept_event_id: "ev-9".into(),
                absorbed_event_ids: vec![],
                tokens_before: 0,
            }))
            .entry_type(),
            EntryType::Compaction
        );
    }

    #[test]
    fn compaction_entries_carry_witnesses() {
        let entry = Entry::Compaction(CompactionRecord {
            summary: "benchmarked A0 baseline".into(),
            first_kept_event_id: "ev-9".into(),
            absorbed_event_ids: vec!["ev-1".into(), "ev-2".into()],
            tokens_before: 8192,
        });
        let Entry::Compaction(record) = &entry else {
            panic!("expected compaction entry");
        };
        assert!(record.absorbs("ev-2"));
    }
}
