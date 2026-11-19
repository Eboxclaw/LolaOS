use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCatalog {
    pub schema_version: u32,
    pub models: Vec<ModelArtifact>,
}

impl ModelCatalog {
    pub fn from_json(source: &str) -> Result<Self, ModelCatalogError> {
        let catalog: Self =
            serde_json::from_str(source).map_err(|error| ModelCatalogError::InvalidJson(error.to_string()))?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), ModelCatalogError> {
        if self.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(ModelCatalogError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if self.models.is_empty() {
            return Err(ModelCatalogError::EmptyCatalog);
        }

        let mut ids = HashSet::with_capacity(self.models.len());
        for model in &self.models {
            if !ids.insert(model.id.as_str()) {
                return Err(ModelCatalogError::DuplicateModelId(model.id.clone()));
            }
            model
                .validate()
                .map_err(|reason| ModelCatalogError::InvalidArtifact {
                    model_id: model.id.clone(),
                    reason,
                })?;
        }
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&ModelArtifact> {
        self.models.iter().find(|model| model.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat {
    Gguf,
    Onnx,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelArtifact {
    pub id: String,
    pub display_name: String,
    pub family: String,
    pub publisher: String,
    pub source_repo: String,
    pub revision: String,
    pub file_name: String,
    pub format: ArtifactFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    pub download_url: String,
    pub sha256: String,
    /// Human-readable publisher size label; it is not used as an integrity check.
    pub size_label: String,
    pub parameter_count: u64,
    /// Publisher-advertised context only; not a device-tested context guarantee.
    pub advertised_context: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested_context_tokens: Option<u32>,
    pub architecture: String,
    pub license: String,
    pub license_url: String,
    pub model_card_url: String,
}

impl ModelArtifact {
    pub fn validate(&self) -> Result<(), ArtifactValidationError> {
        if !is_model_id(&self.id) {
            return Err(ArtifactValidationError::InvalidModelId);
        }
        for (field, value) in [
            ("display_name", self.display_name.as_str()),
            ("family", self.family.as_str()),
            ("publisher", self.publisher.as_str()),
            ("source_repo", self.source_repo.as_str()),
            ("file_name", self.file_name.as_str()),
            ("size_label", self.size_label.as_str()),
            ("advertised_context", self.advertised_context.as_str()),
            ("architecture", self.architecture.as_str()),
            ("license", self.license.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ArtifactValidationError::MissingField(field));
            }
        }
        if !is_hugging_face_repo(&self.source_repo) {
            return Err(ArtifactValidationError::InvalidSourceRepository);
        }
        if !is_pinned_revision(&self.revision) {
            return Err(ArtifactValidationError::InvalidRevision);
        }
        if !is_sha256(&self.sha256) {
            return Err(ArtifactValidationError::InvalidSha256);
        }
        if self.file_name == "." || self.file_name == ".."
            || self.file_name.contains('/')
            || self.file_name.contains('\\')
        {
            return Err(ArtifactValidationError::InvalidFileName);
        }

        let extension_matches = match &self.format {
            ArtifactFormat::Gguf => self.file_name.ends_with(".gguf"),
            ArtifactFormat::Onnx => self.file_name.ends_with(".onnx"),
        };
        if !extension_matches {
            return Err(ArtifactValidationError::FormatExtensionMismatch);
        }

        let expected_url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.source_repo, self.revision, self.file_name
        );
        if self.download_url != expected_url {
            return Err(ArtifactValidationError::DownloadUrlNotPinned);
        }
        if !is_https_url(&self.license_url) || !is_https_url(&self.model_card_url) {
            return Err(ArtifactValidationError::ReferenceUrlMustUseHttps);
        }
        if self.parameter_count == 0 {
            return Err(ArtifactValidationError::InvalidParameterCount);
        }
        if self
            .tested_context_tokens
            .is_some_and(|context_tokens| context_tokens == 0)
        {
            return Err(ArtifactValidationError::InvalidTestedContext);
        }
        if self
            .quantization
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ArtifactValidationError::MissingField("quantization"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelCatalogError {
    InvalidJson(String),
    UnsupportedSchemaVersion(u32),
    EmptyCatalog,
    DuplicateModelId(String),
    InvalidArtifact {
        model_id: String,
        reason: ArtifactValidationError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactValidationError {
    InvalidModelId,
    MissingField(&'static str),
    InvalidSourceRepository,
    InvalidRevision,
    InvalidSha256,
    InvalidFileName,
    FormatExtensionMismatch,
    DownloadUrlNotPinned,
    ReferenceUrlMustUseHttps,
    InvalidParameterCount,
    InvalidTestedContext,
}

fn is_model_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-_".contains(&byte))
}

fn is_hugging_face_repo(value: &str) -> bool {
    let mut parts = value.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(name), None) => is_repo_segment(owner) && is_repo_segment(name),
        _ => false,
    }
}

fn is_repo_segment(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || b"-_.".contains(&byte)
        })
}

fn is_pinned_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_https_url(value: &str) -> bool {
    value.starts_with("https://") && !value.bytes().any(|byte| byte.is_ascii_whitespace())
}
