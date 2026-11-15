//! `config/models.yaml` — the model registry (ADR-006, ADR-019).
//!
//! Entries are modality-tagged; profiles (memory.yaml) pick the resident
//! *pair* from this registry.

use serde::Deserialize;

use lola_harness::contracts::Modality;

use crate::infer::ModelRole;

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsConfig {
    /// Loadable models for the current slice (any profile's pair must be
    /// resolvable from this list).
    pub models: Vec<ModelEntry>,
    /// Metadata-only entries; not loadable until their slice lands.
    pub registered: Vec<ModelEntry>,
    /// Streaming audio codecs (LiquidCodec) powering the omni voice path —
    /// first-class engine components (K9, ADR-035).
    #[serde(default)]
    pub codecs: Vec<CodecEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodecEntry {
    pub codec_id: String,
    /// Codec builds ship inside the Audio/Omni GGUF releases.
    pub repository: String,
    pub quantizations: Vec<String>,
    /// Decoder-only QAT: Q4_0 is 3.5× smaller with minimal quality loss.
    pub default_quantization: String,
    pub est_ram_mb: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    pub model_id: String,
    pub repository: String,
    pub role: ModelRole,
    pub modality: Modality,
    pub quantization: String,
    /// `reference-llamacpp` (S2 oracle), `lola-custom` (S3), or `none`.
    pub backend: String,
    pub context_limit: u32,
    /// Estimated resident footprint at the configured quantization.
    pub est_ram_mb: u64,
    /// Omni checkpoints ship encoder towers as an mmproj sidecar GGUF.
    #[serde(default)]
    pub mmproj: Option<String>,
    /// Pinned GGUF filename (interchange format; repacked at install).
    #[serde(default)]
    pub gguf_file: Option<String>,
    /// Pinned sha256 — filled by S2.c when weights are fetched/verified.
    #[serde(default)]
    pub sha256: Option<String>,
    /// How this quantization came to exist (ADR-040 ladder input).
    #[serde(default)]
    pub quant_provenance: crate::engine::QuantProvenance,
}

impl ModelsConfig {
    pub fn parse(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    pub fn find(&self, model_id: &str) -> Option<&ModelEntry> {
        self.models
            .iter()
            .chain(self.registered.iter())
            .find(|entry| entry.model_id == model_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infer::ModelRole;
    use lola_harness::contracts::Modality;

    const MODELS_YAML: &str = include_str!("../../../config/models.yaml");

    #[test]
    fn shipped_models_yaml_parses() {
        let config = ModelsConfig::parse(MODELS_YAML).expect("parse models.yaml");
        assert_eq!(config.models.len(), 4, "omni pair + text pair are loadable");
        assert!(
            config.registered.len() >= 3,
            "specialists are metadata-only for now"
        );

        let omni_decision = config.find("d1-omni-2b").expect("d1-omni registered");
        assert_eq!(omni_decision.role, ModelRole::Decision);
        assert_eq!(omni_decision.modality, Modality::Omni);
        assert!(omni_decision.mmproj.is_some(), "omni needs mmproj sidecar");
        assert_eq!(omni_decision.quantization, "q8_0");

        let omni_agent = config.find("lfm3-omni-2b").expect("lfm3-omni registered");
        assert_eq!(omni_agent.role, ModelRole::Agent);
        assert_eq!(omni_agent.context_limit, 32768);

        let text_agent = config.find("lfm3-1.7b").expect("lfm3-1.7b registered");
        assert_eq!(text_agent.modality, Modality::Text);
        assert!(text_agent.mmproj.is_none());

        // ADR-040 ladder: QAD-Q4_0 where it exists (1.7B only today).
        assert_eq!(text_agent.quantization, "q4_0");
        assert_eq!(
            text_agent.quant_provenance,
            crate::engine::QuantProvenance::Qad
        );
        assert_eq!(
            text_agent.gguf_file.as_deref(),
            Some("LFM3-1.7B-QAD-Q4_0.gguf")
        );
        // Ladder position 2 (no QAD): Q4_K_M.
        assert_eq!(
            config.find("lfm3-omni-2b").unwrap().quant_provenance,
            crate::engine::QuantProvenance::Ptq
        );
        assert_eq!(config.find("lfm3-omni-2b").unwrap().quantization, "q4_k_m");
        // Ladder position 3 (no QAD, no Q4_K_M): Q8_0.
        assert_eq!(config.find("d1-omni-2b").unwrap().quantization, "q8_0");

        // LiquidCodec is a first-class engine component (K9, ADR-035).
        let codec = config
            .codecs
            .iter()
            .find(|c| c.codec_id == "liquidcodec")
            .expect("liquidcodec registered");
        assert!(codec.quantizations.contains(&"q4_0".to_string()));
        assert_eq!(codec.default_quantization, "q4_0");
        assert!(codec.est_ram_mb < 150, "~74M params — tiny resident cost");
    }
}
