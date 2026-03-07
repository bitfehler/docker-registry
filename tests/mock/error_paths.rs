use futures::StreamExt;

#[tokio::test]
async fn test_get_manifest_server_error() {
  let name = "my-repo/my-image";
  let reference = "latest";
  let ep = format!("/v2/{name}/manifests/{reference}");

  let mut server = mockito::Server::new_async().await;
  let addr = server.host_with_port();

  let mock = server
    .mock("GET", ep.as_str())
    .with_status(500)
    .with_body(r#"{"errors":[]}"#)
    .create();

  let client = docker_registry::v2::Client::configure()
    .registry(&addr)
    .insecure_registry(true)
    .build()
    .unwrap();

  let result = client.get_manifest(name, reference).await;
  assert!(result.is_err());
  mock.assert_async().await;
}

#[tokio::test]
async fn test_get_blob_server_error() {
  let name = "my-repo/my-image";
  let digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
  let ep = format!("/v2/{name}/blobs/{digest}");

  let mut server = mockito::Server::new_async().await;
  let addr = server.host_with_port();

  let mock = server
    .mock("GET", ep.as_str())
    .with_status(500)
    .with_body("Internal Server Error")
    .create();

  let client = docker_registry::v2::Client::configure()
    .registry(&addr)
    .insecure_registry(true)
    .build()
    .unwrap();

  let result = client.get_blob(name, digest).await;
  assert!(result.is_err());
  assert!(
    matches!(result.unwrap_err(), docker_registry::errors::Error::Server { .. }),
    "Expected Server error variant"
  );
  mock.assert_async().await;
}

#[tokio::test]
async fn test_get_blob_client_error() {
  let name = "my-repo/my-image";
  let digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
  let ep = format!("/v2/{name}/blobs/{digest}");

  let mut server = mockito::Server::new_async().await;
  let addr = server.host_with_port();

  let mock = server
    .mock("GET", ep.as_str())
    .with_status(404)
    .with_body(r#"{"errors":[{"code":"BLOB_UNKNOWN","message":"blob unknown to registry"}]}"#)
    .create();

  let client = docker_registry::v2::Client::configure()
    .registry(&addr)
    .insecure_registry(true)
    .build()
    .unwrap();

  let result = client.get_blob(name, digest).await;
  assert!(result.is_err());
  assert!(
    matches!(result.unwrap_err(), docker_registry::errors::Error::Api(_)),
    "Expected Api error variant"
  );
  mock.assert_async().await;
}

#[tokio::test]
async fn test_get_tags_not_found() {
  let name = "nonexistent/image";
  let ep = format!("/v2/{name}/tags/list");

  let mut server = mockito::Server::new_async().await;
  let addr = server.host_with_port();

  let mock = server.mock("GET", ep.as_str()).with_status(404).create();

  let client = docker_registry::v2::Client::configure()
    .registry(&addr)
    .insecure_registry(true)
    .build()
    .unwrap();

  let tags: Vec<_> = client.get_tags(name, None).collect().await;
  assert!(!tags.is_empty());
  assert!(tags[0].is_err());
  mock.assert_async().await;
}

#[tokio::test]
async fn test_get_catalog_server_error() {
  let mut server = mockito::Server::new_async().await;
  let addr = server.host_with_port();

  let mock = server.mock("GET", "/v2/_catalog").with_status(500).create();

  let client = docker_registry::v2::Client::configure()
    .registry(&addr)
    .insecure_registry(true)
    .build()
    .unwrap();

  let repos: Vec<_> = client.get_catalog(None).collect().await;
  assert!(!repos.is_empty());
  assert!(repos[0].is_err());
  mock.assert_async().await;
}

#[tokio::test]
async fn test_has_manifest_not_found() {
  let name = "my-repo/my-image";
  let reference = "nonexistent-tag";
  let ep = format!("/v2/{name}/manifests/{reference}");

  let mut server = mockito::Server::new_async().await;
  let addr = server.host_with_port();

  let mock = server.mock("HEAD", ep.as_str()).with_status(404).create();

  let client = docker_registry::v2::Client::configure()
    .registry(&addr)
    .insecure_registry(true)
    .build()
    .unwrap();

  let result = client.has_manifest(name, reference, None).await.unwrap();
  assert!(result.is_none());
  mock.assert_async().await;
}
