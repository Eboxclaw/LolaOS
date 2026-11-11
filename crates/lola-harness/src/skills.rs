//! Agent Skills — we adopt the open standard as-is (ADR-028,
//! `docs/CONTEXT.md`). A skill is a folder with a `SKILL.md`: YAML
//! frontmatter + Markdown body, with normative 3-level progressive
//! disclosure so nothing is preloaded into the agent's context.
//!
//! Spec: https://agentskills.io/specification.md (stewarded at
//! github.com/agentskills/agentskills). LOLA extensions live in the
//! sanctioned `metadata:` map — never in forked fields.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Past this many installed skills, stop listing metadata in every prompt
/// and expose a `search_skills(query)` tool instead (Anthropic Tool Search
/// pattern).
pub const SKILL_SEARCH_THRESHOLD: usize = 40;

/// Frontmatter size budgets per the spec.
pub const NAME_MAX: usize = 64;
pub const DESCRIPTION_MAX: usize = 1024;
pub const COMPATIBILITY_MAX: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SkillManifest {
    /// ≤64 chars, lowercase alphanumerics + hyphens, no leading/trailing/
    /// double hyphens; must equal the skill's directory name.
    pub name: String,
    /// What it does + when to use it; keyword-rich; ≤1024 chars.
    pub description: String,
    pub license: Option<String>,
    /// ≤500 chars.
    pub compatibility: Option<String>,
    /// Sanctioned extension point — LOLA keys like `lola-tier` go here.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// Space-separated pre-approved tools (experimental in the spec;
    /// treated as advisory).
    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Option<String>,
}

/// The three disclosure levels (normative in the spec):
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisclosureTier {
    /// name + description (~100 tokens), listed at session start.
    Metadata,
    /// SKILL.md body (<5K tokens), loaded on activation.
    Body,
    /// references/ · assets/ · scripts/, loaded on demand.
    Resources,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillError {
    #[error("missing or malformed YAML frontmatter")]
    MissingFrontmatter,
    #[error("frontmatter is not valid YAML: {0}")]
    InvalidFrontmatter(String),
    #[error("name violates the spec: {0}")]
    NameRule(String),
    #[error("description exceeds {DESCRIPTION_MAX} chars")]
    DescriptionTooLong,
    #[error("compatibility exceeds {COMPATIBILITY_MAX} chars")]
    CompatibilityTooLong,
}

impl SkillManifest {
    /// Parse a `SKILL.md` document (frontmatter between `---` fences).
    pub fn parse(skill_md: &str) -> Result<Self, SkillError> {
        let rest = skill_md
            .strip_prefix("---\n")
            .ok_or(SkillError::MissingFrontmatter)?;
        let end = rest.find("\n---").ok_or(SkillError::MissingFrontmatter)?;
        let frontmatter = &rest[..end];
        let manifest: SkillManifest = serde_yaml::from_str(frontmatter)
            .map_err(|e| SkillError::InvalidFrontmatter(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Enforce the spec constraints (subset the `skills-ref` validator checks).
    pub fn validate(&self) -> Result<(), SkillError> {
        let name = &self.name;
        if name.is_empty() || name.len() > NAME_MAX {
            return Err(SkillError::NameRule(format!(
                "length {name_len} not in 1..={NAME_MAX}",
                name_len = name.len()
            )));
        }
        let valid = !name.starts_with('-')
            && !name.ends_with('-')
            && !name.contains("--")
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid {
            return Err(SkillError::NameRule(name.clone()));
        }
        if self.description.len() > DESCRIPTION_MAX {
            return Err(SkillError::DescriptionTooLong);
        }
        if self
            .compatibility
            .as_deref()
            .is_some_and(|c| c.len() > COMPATIBILITY_MAX)
        {
            return Err(SkillError::CompatibilityTooLong);
        }
        Ok(())
    }

    /// Level-1 payload: what may enter a session-start skill index.
    pub fn metadata_tier(&self) -> DisclosureTier {
        DisclosureTier::Metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\nname: hyperliquid-research\ndescription: Research Hyperliquid market structure via the ledger and knowledge graph when investigating regime shifts or unusual fills.\nlicense: MIT\ncompatibility: lola>=0.1\nmetadata:\n  lola-tier: support-pass\nallowed-tools: graph_query fts_search\n---\n\n# Hyperliquid research\n\nBody loaded only on activation…\n";

    #[test]
    fn parses_a_spec_compliant_skill() {
        let manifest = SkillManifest::parse(SAMPLE).expect("parse");
        assert_eq!(manifest.name, "hyperliquid-research");
        assert_eq!(
            manifest.metadata.get("lola-tier").map(String::as_str),
            Some("support-pass")
        );
        assert_eq!(
            manifest.allowed_tools.as_deref(),
            Some("graph_query fts_search")
        );
        assert_eq!(manifest.metadata_tier(), DisclosureTier::Metadata);
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn rejects_uppercase_names() {
        let bad = SAMPLE.replace("hyperliquid-research", "Hyperliquid-Research");
        assert!(matches!(
            SkillManifest::parse(&bad),
            Err(SkillError::NameRule(_))
        ));
    }

    #[test]
    fn rejects_double_hyphens_and_overlong_descriptions() {
        let bad = SAMPLE.replace("hyperliquid-research", "hyper--liquid");
        assert!(matches!(
            SkillManifest::parse(&bad),
            Err(SkillError::NameRule(_))
        ));

        let long = "x".repeat(1025);
        let bad = SAMPLE.replace(
            "Research Hyperliquid market structure via the ledger and knowledge graph when investigating regime shifts or unusual fills.",
            &long,
        );
        assert!(matches!(
            SkillManifest::parse(&bad),
            Err(SkillError::DescriptionTooLong)
        ));
    }

    #[test]
    fn rejects_documents_without_frontmatter() {
        assert!(matches!(
            SkillManifest::parse("no frontmatter here"),
            Err(SkillError::MissingFrontmatter)
        ));
    }
}
