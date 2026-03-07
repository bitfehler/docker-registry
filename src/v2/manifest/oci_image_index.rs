use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::manifest_schema2::ManifestObj;

/// OCI Content Descriptor.
///
/// Specification: <https://github.com/opencontainers/image-spec/blob/main/descriptor.md>
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OciDescriptor {
  #[serde(rename = "mediaType")]
  pub media_type: String,
  pub size: u64,
  pub digest: String,
}

/// OCI Image Index.
///
/// Specification: <https://github.com/opencontainers/image-spec/blob/main/image-index.md>
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OciImageIndex {
  #[serde(rename = "schemaVersion")]
  schema_version: u16,
  #[serde(rename = "mediaType", default)]
  media_type: String,
  #[serde(rename = "artifactType", default)]
  artifact_type: Option<String>,
  pub manifests: Vec<ManifestObj>,
  subject: Option<OciDescriptor>,
  annotations: Option<HashMap<String, String>>,
}

impl OciImageIndex {
  /// Get the artifact type, if present.
  pub fn artifact_type(&self) -> Option<&str> {
    self.artifact_type.as_deref()
  }

  /// Get the subject descriptor, if present.
  pub fn subject(&self) -> Option<&OciDescriptor> {
    self.subject.as_ref()
  }

  /// Get annotations, if present.
  pub fn annotations(&self) -> Option<&HashMap<String, String>> {
    self.annotations.as_ref()
  }

  /// Get architecture of all the manifests.
  pub fn architectures(&self) -> Vec<String> {
    self.manifests.iter().map(|mo| mo.architecture()).collect()
  }

  /// Get the digest for all the manifest images.
  pub fn get_digests(&self) -> Vec<String> {
    self.manifests.iter().map(|mo| mo.digest()).collect()
  }
}
