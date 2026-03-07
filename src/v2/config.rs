use log::trace;
use reqwest::Certificate;

use crate::{mediatypes::MediaTypes, v2::*};

/// Configuration for a `Client`.
#[derive(Clone, Debug)]
pub struct Config {
  index: String,
  insecure_registry: bool,
  user_agent: Option<String>,
  username: Option<String>,
  password: Option<String>,
  accept_invalid_certs: bool,
  root_certificates: Vec<Certificate>,
  accepted_types: Option<Vec<(MediaTypes, Option<f64>)>>,
  connect_timeout: Option<std::time::Duration>,
  request_timeout: Option<std::time::Duration>,
}

impl Config {
  /// Set registry service to use (vhost or IP).
  #[must_use]
  pub fn registry(mut self, reg: &str) -> Self {
    self.index = reg.to_owned();
    self
  }

  /// Whether to use an insecure HTTP connection to the registry.
  #[must_use]
  pub fn insecure_registry(mut self, insecure: bool) -> Self {
    self.insecure_registry = insecure;
    self
  }

  /// Set whether or not to accept invalid certificates.
  #[must_use]
  pub fn accept_invalid_certs(mut self, accept_invalid_certs: bool) -> Self {
    self.accept_invalid_certs = accept_invalid_certs;
    self
  }

  /// Add a root certificate the client should trust for TLS verification
  #[must_use]
  pub fn add_root_certificate(mut self, certificate: Certificate) -> Self {
    self.root_certificates.push(certificate);
    self
  }

  /// Set custom Accept headers
  #[must_use]
  pub fn accepted_types(mut self, accepted_types: Option<Vec<(MediaTypes, Option<f64>)>>) -> Self {
    self.accepted_types = accepted_types;
    self
  }

  /// Set the user-agent to be used for registry authentication.
  #[must_use]
  pub fn user_agent(mut self, user_agent: Option<String>) -> Self {
    self.user_agent = user_agent;
    self
  }

  /// Set the username to be used for registry authentication.
  #[must_use]
  pub fn username(mut self, user: Option<String>) -> Self {
    self.username = user;
    self
  }

  /// Set the password to be used for registry authentication.
  #[must_use]
  pub fn password(mut self, password: Option<String>) -> Self {
    self.password = password;
    self
  }

  /// Read credentials from a JSON config file
  #[must_use]
  pub fn read_credentials<T: ::std::io::Read>(mut self, reader: T) -> Self {
    if let Ok(creds) = crate::get_credentials(reader, &self.index) {
      self.username = creds.0;
      self.password = creds.1;
    };
    self
  }

  /// Set the connect timeout for the HTTP client.
  #[must_use]
  pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
    self.connect_timeout = Some(timeout);
    self
  }

  /// Set the overall request timeout for the HTTP client.
  #[must_use]
  pub fn request_timeout(mut self, timeout: std::time::Duration) -> Self {
    self.request_timeout = Some(timeout);
    self
  }

  /// Return a `Client` to interact with a v2 registry.
  pub fn build(self) -> Result<Client> {
    let base = if self.insecure_registry {
      "http://".to_string() + &self.index
    } else {
      "https://".to_string() + &self.index
    };
    trace!(
      "Built client for {:?}: endpoint {:?} - user {:?}",
      self.index, base, self.username
    );
    let creds = match (self.username, self.password) {
      (None, None) => None,
      (u, p) => Some((u.unwrap_or_else(|| "".into()), p.unwrap_or_else(|| "".into()))),
    };

    let mut builder = reqwest::ClientBuilder::new().danger_accept_invalid_certs(self.accept_invalid_certs);

    if let Some(timeout) = self.connect_timeout {
      builder = builder.connect_timeout(timeout);
    }
    if let Some(timeout) = self.request_timeout {
      builder = builder.timeout(timeout);
    }

    for ca in self.root_certificates {
      builder = builder.add_root_certificate(ca)
    }

    let client = builder.build()?;

    let accepted_types = match self.accepted_types {
      Some(a) => a,
      None => match self.index == "gcr.io" || self.index.ends_with(".gcr.io") || self.index.ends_with(".k8s.io") {
        false => vec![
          // accept header types and their q value, as documented in
          // https://tools.ietf.org/html/rfc7231#section-5.3.2
          (MediaTypes::ManifestV2S2, Some(0.5)),
          (MediaTypes::ManifestV2S1Signed, Some(0.4)),
          (MediaTypes::ManifestList, Some(0.5)),
          (MediaTypes::OciImageManifest, Some(0.5)),
          (MediaTypes::OciImageIndexV1, Some(0.5)),
        ],
        // GCR incorrectly parses `q` parameters, so we use special Accept for it.
        // Bug: https://issuetracker.google.com/issues/159827510.
        // TODO: when bug is fixed, this workaround should be removed.
        // *.k8s.io container registries use GCR and are similarly affected.
        true => vec![
          (MediaTypes::ManifestV2S2, None),
          (MediaTypes::ManifestV2S1Signed, None),
          (MediaTypes::ManifestList, None),
          (MediaTypes::OciImageManifest, None),
          (MediaTypes::OciImageIndexV1, None),
        ],
      },
    };
    let c = Client {
      base_url: base,
      credentials: creds,
      user_agent: self.user_agent,
      auth: None,
      client,
      accepted_types,
    };
    Ok(c)
  }
}

impl Default for Config {
  /// Initialize `Config` with default values.
  fn default() -> Self {
    Self {
      index: "registry-1.docker.io".into(),
      insecure_registry: false,
      accept_invalid_certs: false,
      root_certificates: Default::default(),
      accepted_types: None,
      user_agent: Some(crate::USER_AGENT.to_owned()),
      username: None,
      password: None,
      connect_timeout: None,
      request_timeout: None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_config_default() {
    let config = Config::default();
    assert_eq!(config.index, "registry-1.docker.io");
    assert!(!config.insecure_registry);
    assert!(!config.accept_invalid_certs);
    assert!(config.username.is_none());
    assert!(config.password.is_none());
    assert!(config.accepted_types.is_none());
    assert_eq!(config.user_agent.as_deref(), Some(crate::USER_AGENT));
  }

  #[test]
  fn test_config_builder_chaining() {
    let config = Config::default()
      .registry("myregistry.io")
      .insecure_registry(true)
      .accept_invalid_certs(true)
      .username(Some("user".to_string()))
      .password(Some("pass".to_string()))
      .user_agent(Some("test-agent".to_string()));

    assert_eq!(config.index, "myregistry.io");
    assert!(config.insecure_registry);
    assert!(config.accept_invalid_certs);
    assert_eq!(config.username.as_deref(), Some("user"));
    assert_eq!(config.password.as_deref(), Some("pass"));
    assert_eq!(config.user_agent.as_deref(), Some("test-agent"));
  }

  #[test]
  fn test_config_build_insecure() {
    let client = Config::default()
      .registry("localhost:5000")
      .insecure_registry(true)
      .build()
      .unwrap();
    assert!(client.base_url.starts_with("http://"));
  }

  #[test]
  fn test_config_build_secure() {
    let client = Config::default()
      .registry("myregistry.io")
      .insecure_registry(false)
      .build()
      .unwrap();
    assert!(client.base_url.starts_with("https://"));
  }

  #[test]
  fn test_config_build_credentials() {
    let client = Config::default()
      .registry("myregistry.io")
      .username(Some("user".to_string()))
      .password(Some("pass".to_string()))
      .build()
      .unwrap();
    assert_eq!(client.credentials, Some(("user".to_string(), "pass".to_string())));
  }

  #[test]
  fn test_config_build_no_credentials() {
    let client = Config::default().registry("myregistry.io").build().unwrap();
    assert!(client.credentials.is_none());
  }

  #[test]
  fn test_config_build_partial_credentials_username_only() {
    let client = Config::default()
      .registry("myregistry.io")
      .username(Some("user".to_string()))
      .build()
      .unwrap();
    assert_eq!(client.credentials, Some(("user".to_string(), "".to_string())));
  }

  #[test]
  fn test_config_read_credentials() {
    // base64("user:pass") = "dXNlcjpwYXNz"
    let config_json = r#"{"auths":{"myregistry.io":{"auth":"dXNlcjpwYXNz"}}}"#;
    let config = Config::default()
      .registry("myregistry.io")
      .read_credentials(config_json.as_bytes());
    assert_eq!(config.username.as_deref(), Some("user"));
    assert_eq!(config.password.as_deref(), Some("pass"));
  }

  #[test]
  fn test_config_read_credentials_missing_registry() {
    let config_json = r#"{"auths":{"other.io":{"auth":"dXNlcjpwYXNz"}}}"#;
    let config = Config::default()
      .registry("myregistry.io")
      .read_credentials(config_json.as_bytes());
    // Should silently fail and leave credentials as None
    assert!(config.username.is_none());
    assert!(config.password.is_none());
  }

  #[test]
  fn test_config_timeout_fields() {
    let config = Config::default()
      .connect_timeout(std::time::Duration::from_secs(5))
      .request_timeout(std::time::Duration::from_secs(30));

    assert_eq!(config.connect_timeout, Some(std::time::Duration::from_secs(5)));
    assert_eq!(config.request_timeout, Some(std::time::Duration::from_secs(30)));
  }
}
