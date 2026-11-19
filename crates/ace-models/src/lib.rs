//! Portable model catalogue, artifact validation and lifecycle primitives.
//!
//! This crate deliberately does not download files or load inference backends.
//! It defines the validated metadata and state transitions used by those layers.

mod catalog;
mod integrity;
mod lifecycle;

pub use catalog::{
    ArtifactFormat, ArtifactValidationError, ModelArtifact, ModelCatalog, ModelCatalogError,
    CATALOG_SCHEMA_VERSION,
};
pub use integrity::{verify_reader, IntegrityError, VerificationReport};
pub use lifecycle::{ArtifactState, LifecycleError, ModelLifecycle};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_valid_and_pinned() {
        let catalog = ModelCatalog::from_json(include_str!("../../../models/catalog.v1.json"))
            .expect("bundled model catalog should validate");

        assert_eq!(catalog.schema_version, CATALOG_SCHEMA_VERSION);
        assert_eq!(catalog.models.len(), 1);

        let model = catalog
            .find("liquidai.lfm3-1.7b.gguf.q4_k_m")
            .expect("LFM3 Q4_K_M entry should exist");
        assert_eq!(model.quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(model.tested_context_tokens, None);
    }

    #[test]
    fn artifact_rejects_mutable_revision_and_mismatched_download_url() {
        let catalog = ModelCatalog::from_json(include_str!("../../../models/catalog.v1.json"))
            .expect("fixture should validate");
        let mut model = catalog.models[0].clone();

        model.revision = "main".to_owned();
        assert_eq!(
            model.validate(),
            Err(ArtifactValidationError::InvalidRevision)
        );

        model.revision = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned();
        assert_eq!(
            model.validate(),
            Err(ArtifactValidationError::DownloadUrlNotPinned)
        );
    }

    #[test]
    fn lifecycle_rejects_skipping_verification() {
        let mut lifecycle = ModelLifecycle::new();
        assert_eq!(
            lifecycle.transition(ArtifactState::Installed),
            Err(LifecycleError::InvalidTransition {
                from: ArtifactState::Available,
                to: ArtifactState::Installed,
            })
        );
        assert_eq!(lifecycle.state(), ArtifactState::Available);
    }

    #[test]
    fn lifecycle_supports_download_verify_install_load_use_unload() {
        let mut lifecycle = ModelLifecycle::new();
        for state in [
            ArtifactState::Downloading,
            ArtifactState::Downloaded,
            ArtifactState::Verifying,
            ArtifactState::Verified,
            ArtifactState::Installing,
            ArtifactState::Installed,
            ArtifactState::Loading,
            ArtifactState::Ready,
            ArtifactState::InUse,
            ArtifactState::Unloading,
            ArtifactState::Installed,
        ] {
            lifecycle.transition(state).unwrap();
        }
        assert_eq!(lifecycle.state(), ArtifactState::Installed);
    }
}
