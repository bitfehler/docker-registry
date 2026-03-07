use log::trace;
use reqwest::{self, Method, StatusCode};

use crate::{
  errors::Result,
  v2::{manifest::OciImageIndex, *},
};

impl Client {
  /// Retrieve the list of referrers for a given manifest digest.
  ///
  /// Returns an OCI Image Index containing descriptors of manifests
  /// that reference the given digest via their `subject` field.
  ///
  /// Optionally filter by `artifact_type` (e.g. `application/vnd.example.sbom.v1`).
  ///
  /// See: <https://github.com/opencontainers/distribution-spec/blob/main/spec.md#listing-referrers>
  pub async fn get_referrers(
    &self,
    name: &str,
    digest: &str,
    artifact_type: Option<&str>,
  ) -> Result<OciImageIndex> {
    let mut ep = format!("{}/v2/{}/referrers/{}", self.base_url, name, digest);
    if let Some(at) = artifact_type {
      ep = format!("{ep}?artifactType={at}");
    }
    let url = reqwest::Url::parse(&ep)?;

    let resp = self.build_reqwest(Method::GET, url.clone()).send().await?;

    let status = resp.status();
    trace!("GET '{}' status: {:?}", resp.url(), status);

    match status {
      StatusCode::OK => Ok(resp.json::<OciImageIndex>().await?),
      _ => Err(ApiErrors::from(resp).await),
    }
  }
}
