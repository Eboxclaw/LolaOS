//! `config/models.yaml` — the model registry (ADR-006).

use serde::Deserialize;

use crate::infer::ModelRole;

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsConfig {
    /// Loadable models for the current slice.
    pub active: Vec<ModelEntry>,
    /// Metadata-only entries; not loadable until their slice lands.
    pub registered: Vec<ModelEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    pub model_id: String,
    pub repository: String,
    pub role: ModelRole,
    pub quantization: String,
    /// `reference-llamacpp` (S2 oracle), `lola-custom` (S3), or `none`.
    pub backend: String,
    pub context_limit: u32,
    /// Estimated resident footprint at the configured quantization.
    pub est_ram_mb: u64,
}

impl ModelsConfig {
    pub fn parse(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    pub fn find(&self, model_id: &str) -> Option<&ModelEntry> {
        self.active
            .iter()
            .chain(self.registered.iter())
            .find(|entry| entry.model_id == model_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infer::ModelRole;

    const MODELS_YAML: &str = include_str!("../../../config/models.yaml");

    #[test]
    fn shipped_models_yaml_parses() {
        let config = ModelsConfig::parse(MODELS_YAML).expect("parse models.yaml");
        assert_eq!(
            config.active.len(),
            2,
            "d1-2b + lfm3-1.7b are the active pair"
        );
        assert!(
            config.registered.len() >= 5,
            "specialists are metadata-only for now"
        );

        let d1 = config.find("d1-2b").expect("d1 registered");
        assert_eq!(d1.role, ModelRole::Decision);
        assert!(d1.est_ram_mb > 0);

        let agent = config.find("lfm3-1.7b").expect("lfm3 registered");
        assert_eq!(agent.role, ModelRole::Agent);
    }
}
