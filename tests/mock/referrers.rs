#[tokio::test]
async fn test_get_referrers_success() {
  let name = "my-repo/my-image";
  let digest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  let ep = format!("/v2/{name}/referrers/{digest}");

  let response_body = serde_json::json!({
    "schemaVersion": 2,
    "mediaType": "application/vnd.oci.image.index.v1+json",
    "manifests": [
      {
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "size": 1234,
        "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "artifactType": "application/vnd.example.sbom.v1",
        "platform": { "architecture": "amd64", "os": "linux" }
      }
    ]
  });

  let mut server = mockito::Server::new_async().await;
  let addr = server.host_with_port();

  let mock = server
    .mock("GET", ep.as_str())
    .with_status(200)
    .with_header("Content-Type", "application/vnd.oci.image.index.v1+json")
    .with_body(response_body.to_string())
    .create();

  let client = docker_registry::v2::Client::configure()
    .registry(&addr)
    .insecure_registry(true)
    .username(None)
    .password(None)
    .build()
    .unwrap();

  let index = client.get_referrers(name, digest, None).await.unwrap();

  mock.assert_async().await;
  assert_eq!(index.manifests.len(), 1);
}

#[tokio::test]
async fn test_get_referrers_with_artifact_type_filter() {
  let name = "my-repo/my-image";
  let digest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  let artifact_type = "application/vnd.example.sbom.v1";
  let ep = format!("/v2/{name}/referrers/{digest}?artifactType={artifact_type}");

  let response_body = serde_json::json!({
    "schemaVersion": 2,
    "mediaType": "application/vnd.oci.image.index.v1+json",
    "manifests": []
  });

  let mut server = mockito::Server::new_async().await;
  let addr = server.host_with_port();

  let mock = server
    .mock("GET", ep.as_str())
    .with_status(200)
    .with_header("Content-Type", "application/vnd.oci.image.index.v1+json")
    .with_header("OCI-Filters-Applied", "artifactType")
    .with_body(response_body.to_string())
    .create();

  let client = docker_registry::v2::Client::configure()
    .registry(&addr)
    .insecure_registry(true)
    .username(None)
    .password(None)
    .build()
    .unwrap();

  let index = client
    .get_referrers(name, digest, Some(artifact_type))
    .await
    .unwrap();

  mock.assert_async().await;
  assert!(index.manifests.is_empty());
}

#[tokio::test]
async fn test_get_referrers_not_supported() {
  let name = "my-repo/my-image";
  let digest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  let ep = format!("/v2/{name}/referrers/{digest}");

  let mut server = mockito::Server::new_async().await;
  let addr = server.host_with_port();

  let mock = server.mock("GET", ep.as_str()).with_status(404).create();

  let client = docker_registry::v2::Client::configure()
    .registry(&addr)
    .insecure_registry(true)
    .username(None)
    .password(None)
    .build()
    .unwrap();

  let result = client.get_referrers(name, digest, None).await;

  mock.assert_async().await;
  assert!(result.is_err());
}
