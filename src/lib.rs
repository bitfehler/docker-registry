//! A pure-Rust asynchronous library for Docker Registry API.
//!
//! This library provides support for asynchronous interaction with
//! container registries conformant to the Docker Registry HTTP API V2.
//!
//! ## Example
//!
//! ```rust,no_run
//! # use tokio;
//!
//! # #[tokio::main]
//! # async fn main() {
//! # async fn run() -> docker_registry::errors::Result<()> {
//! #
//! use docker_registry::v2::Client;
//!
//! // Check whether a registry supports API v2.
//! let host = "quay.io";
//! let client = Client::configure()
//!   .insecure_registry(false)
//!   .registry(host)
//!   .build()?;
//! match client.is_v2_supported().await? {
//!   false => println!("{} does NOT support v2", host),
//!   true => println!("{} supports v2", host),
//! };
//! #
//! # Ok(())
//! # };
//! # run().await.unwrap();
//! # }
//! ```

#![deny(missing_debug_implementations)]

use log::trace;
use serde::{Deserialize, Serialize};

pub mod errors;
pub mod mediatypes;
pub mod reference;
pub mod render;
pub mod v2;

use std::{collections::HashMap, io::Read};

use base64::prelude::*;
use errors::{Error, Result};

/// Default User-Agent client identity.
pub static USER_AGENT: &str = concat!("clowdhaus/docker-registry/", env!("CARGO_PKG_VERSION"));

/// Get registry credentials from a JSON config reader.
///
/// This is a convenience decoder for docker-client credentials
/// typically stored under `~/.docker/config.json`.
pub fn get_credentials<T: Read>(reader: T, index: &str) -> Result<(Option<String>, Option<String>)> {
  let map: Auths = serde_json::from_reader(reader)?;
  let real_index = match index {
    // docker.io has some special casing in config.json
    "docker.io" | "registry-1.docker.io" => "https://index.docker.io/v1/",
    other => other,
  };
  let auth = match map.auths.get(real_index) {
    Some(x) => BASE64_STANDARD.decode(x.auth.as_str())?,
    None => return Err(Error::AuthInfoMissing(real_index.to_string())),
  };
  let s = String::from_utf8(auth)?;
  let creds: Vec<&str> = s.splitn(2, ':').collect();
  let up = match (creds.first(), creds.get(1)) {
    (Some(&""), Some(p)) => (None, Some(p.to_string())),
    (Some(u), Some(&"")) => (Some(u.to_string()), None),
    (Some(u), Some(p)) => (Some(u.to_string()), Some(p.to_string())),
    (_, _) => (None, None),
  };
  trace!("Found credentials for user={:?} on {}", up.0, index);
  Ok(up)
}

#[derive(Debug, Deserialize, Serialize)]
struct Auths {
  auths: HashMap<String, AuthObj>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct AuthObj {
  auth: String,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_get_credentials_valid() {
    let config = r#"{"auths":{"https://index.docker.io/v1/":{"auth":"dXNlcjpwYXNz"}}}"#;
    let (user, pass) = get_credentials(config.as_bytes(), "docker.io").unwrap();
    assert_eq!(user, Some("user".to_string()));
    assert_eq!(pass, Some("pass".to_string()));
  }

  #[test]
  fn test_get_credentials_registry1_docker_io() {
    let config = r#"{"auths":{"https://index.docker.io/v1/":{"auth":"dXNlcjpwYXNz"}}}"#;
    let (user, pass) = get_credentials(config.as_bytes(), "registry-1.docker.io").unwrap();
    assert_eq!(user, Some("user".to_string()));
    assert_eq!(pass, Some("pass".to_string()));
  }

  #[test]
  fn test_get_credentials_custom_registry() {
    // base64("admin:secret") = "YWRtaW46c2VjcmV0"
    let config = r#"{"auths":{"myregistry.example.com":{"auth":"YWRtaW46c2VjcmV0"}}}"#;
    let (user, pass) = get_credentials(config.as_bytes(), "myregistry.example.com").unwrap();
    assert_eq!(user, Some("admin".to_string()));
    assert_eq!(pass, Some("secret".to_string()));
  }

  #[test]
  fn test_get_credentials_missing_registry() {
    let config = r#"{"auths":{"other.io":{"auth":"dXNlcjpwYXNz"}}}"#;
    let result = get_credentials(config.as_bytes(), "missing.io");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::AuthInfoMissing(_)));
  }

  #[test]
  fn test_get_credentials_empty_user() {
    // base64(":password") = "OnBhc3N3b3Jk"
    let config = r#"{"auths":{"reg.io":{"auth":"OnBhc3N3b3Jk"}}}"#;
    let (user, pass) = get_credentials(config.as_bytes(), "reg.io").unwrap();
    assert_eq!(user, None);
    assert_eq!(pass, Some("password".to_string()));
  }

  #[test]
  fn test_get_credentials_empty_password() {
    // base64("user:") = "dXNlcjo="
    let config = r#"{"auths":{"reg.io":{"auth":"dXNlcjo="}}}"#;
    let (user, pass) = get_credentials(config.as_bytes(), "reg.io").unwrap();
    assert_eq!(user, Some("user".to_string()));
    assert_eq!(pass, None);
  }

  #[test]
  fn test_get_credentials_invalid_json() {
    let config = r#"not json"#;
    let result = get_credentials(config.as_bytes(), "reg.io");
    assert!(result.is_err());
  }

  #[test]
  fn test_get_credentials_invalid_base64() {
    let config = r#"{"auths":{"reg.io":{"auth":"!!!invalid!!!"}}}"#;
    let result = get_credentials(config.as_bytes(), "reg.io");
    assert!(result.is_err());
  }
}
