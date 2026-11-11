//! Personality as a ledger object — prompt-as-ledger (ADR-029,
//! `docs/CONTEXT.md` §4).
//!
//! The persona is a versioned row, injected verbatim at position 1 of every
//! session, carried **verbatim through compaction** (never summarized —
//! practitioner reports show persona rules are exactly what compaction
//! drops), and checksummed into every decision for auditability.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The one rule this module exists to enforce.
pub const PERSONA_VERBATIM_RULE: &str =
    "the persona block is carried verbatim through compaction; it is never summarized";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaVersion {
    pub persona_id: String,
    pub version: u32,
    pub body: String,
}

impl PersonaVersion {
    /// First 16 hex chars of sha256(body) — stable persona checksum.
    pub fn body_hash(&self) -> String {
        checksum(&self.body)
    }

    /// What every decision records, so post-hoc audits can pin which persona
    /// was in play.
    pub fn checksum(&self) -> PersonaChecksum {
        PersonaChecksum {
            persona_id: self.persona_id.clone(),
            version: self.version,
            body_hash: self.body_hash(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaChecksum {
    pub persona_id: String,
    pub version: u32,
    pub body_hash: String,
}

fn checksum(body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    hex::encode(&digest[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn persona() -> PersonaVersion {
        PersonaVersion {
            persona_id: "lola-primary".into(),
            version: 3,
            body: "You are LOLA's research agent. Concise, evidence-first.".into(),
        }
    }

    #[test]
    fn checksums_are_stable() {
        assert_eq!(persona().body_hash(), persona().body_hash());
        assert_eq!(persona().body_hash().len(), 16);
    }

    #[test]
    fn checksums_change_when_the_body_changes() {
        let edited = PersonaVersion {
            body: "You are LOLA's research agent. Verbose.".into(),
            ..persona()
        };
        assert_ne!(persona().body_hash(), edited.body_hash());
    }

    #[test]
    fn verbatim_rule_is_pinned() {
        assert!(PERSONA_VERBATIM_RULE.contains("never summarized"));
        let p = persona();
        assert_eq!(p.checksum().version, 3);
    }
}
