# OCI Read-Only Gaps Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close the three remaining read-only gaps: opaque whiteout handling, OCI Image Index as a first-class type, and the OCI Referrers API.

**Architecture:** Three independent PRs in dependency order. Opaque whiteout is a self-contained bugfix in `render.rs`. OCI Image Index introduces a new struct and `Manifest` variant that the referrers API then returns.

**Tech Stack:** Rust, serde, reqwest, tar, mockito (tests)

---

## PR 1: Opaque Whiteout Handling

### Task 1: Add failing test for opaque whiteout

**Files:**
- Modify: `src/render.rs` (test section, ~line 230+)

**Step 1: Write the failing test**

Add this test to the `#[cfg(test)] mod tests` block in `src/render.rs`:

```rust
#[test]
fn test_opaque_whiteout_clears_directory() {
  let dir = tempfile::tempdir().unwrap();

  // Layer 1: create a directory with files
  let mut tar_buf = Vec::new();
  {
    let mut builder = tar::Builder::new(&mut tar_buf);

    // Create dir
    let mut header = tar::Header::new_gnu();
    header.set_path("mydir/").unwrap();
    header.set_size(0);
    header.set_mode(0o755);
    header.set_entry_type(tar::EntryType::Directory);
    header.set_cksum();
    builder.append(&header, &[] as &[u8]).unwrap();

    // Create file inside dir
    let content = b"old content";
    let mut header = tar::Header::new_gnu();
    header.set_path("mydir/old_file.txt").unwrap();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, content.as_slice()).unwrap();

    builder.finish().unwrap();
  }
  let mut gz_buf = Vec::new();
  {
    let mut encoder = gzip::Encoder::new(&mut gz_buf).unwrap();
    io::Write::write_all(&mut encoder, &tar_buf).unwrap();
    encoder.finish().into_result().unwrap();
  }
  let layer1 = gz_buf;

  // Layer 2: opaque whiteout marker + new file in same dir
  let mut tar_buf2 = Vec::new();
  {
    let mut builder = tar::Builder::new(&mut tar_buf2);

    // Opaque whiteout marker
    let mut header = tar::Header::new_gnu();
    header.set_path("mydir/.wh..wh..opq").unwrap();
    header.set_size(0);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, &[] as &[u8]).unwrap();

    // New file in the same directory
    let content = b"new content";
    let mut header = tar::Header::new_gnu();
    header.set_path("mydir/new_file.txt").unwrap();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, content.as_slice()).unwrap();

    builder.finish().unwrap();
  }
  let mut gz_buf2 = Vec::new();
  {
    let mut encoder = gzip::Encoder::new(&mut gz_buf2).unwrap();
    io::Write::write_all(&mut encoder, &tar_buf2).unwrap();
    encoder.finish().into_result().unwrap();
  }
  let layer2 = gz_buf2;

  unpack(&[layer1, layer2], dir.path()).unwrap();

  // Old file should be gone (opaque whiteout clears directory)
  assert!(
    !dir.path().join("mydir/old_file.txt").exists(),
    "opaque whiteout should have removed old_file.txt"
  );
  // New file from layer 2 should exist
  assert!(dir.path().join("mydir/new_file.txt").exists());
  assert_eq!(
    fs::read_to_string(dir.path().join("mydir/new_file.txt")).unwrap(),
    "new content"
  );
  // Directory itself should still exist
  assert!(dir.path().join("mydir").is_dir());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib test_opaque_whiteout_clears_directory -- --nocapture`
Expected: FAIL — old_file.txt still exists because opaque whiteout is a no-op.

**Step 3: Implement opaque whiteout handling**

In `src/render.rs`, find the whiteout processing block inside `_unpack` (~line 90). Replace the `".wh..wh..opq"` branch:

```rust
if wh_name == ".wh..wh..opq" {
  // Opaque whiteout: remove all existing directory contents
  let rel_parent = path::PathBuf::from("./".to_string() + &parent.to_string_lossy());
  let abs_parent = target_dir.join(&rel_parent);
  if abs_parent.is_dir() {
    for entry in fs::read_dir(&abs_parent)? {
      let entry = entry?;
      let entry_name = entry.file_name();
      let name_str = entry_name.to_string_lossy();
      // Don't remove whiteout markers themselves or files from the current layer
      if !name_str.starts_with(".wh.") {
        if entry.path().is_dir() {
          fs::remove_dir_all(entry.path())?;
        } else {
          fs::remove_file(entry.path())?;
        }
      }
    }
  }
  // Remove the opaque whiteout marker itself
  let abs_wh_path = target_dir.join(&rel_parent).join(fname);
  remove_whiteout(abs_wh_path)?;
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib test_opaque_whiteout_clears_directory -- --nocapture`
Expected: PASS

**Step 5: Run full test suite**

Run: `cargo test && cargo fmt -- --check && cargo clippy --all-targets --all-features`
Expected: All pass, no warnings

**Step 6: Commit**

```bash
git add src/render.rs
git commit -m "fix: Handle opaque whiteout markers during layer unpacking

Implement .wh..wh..opq handling per OCI image layer spec. When an
opaque whiteout marker is encountered, all pre-existing contents of
the parent directory are removed before the current layer's files
are applied."
```

---

## PR 2: OCI Image Index as First-Class Type

Currently, OCI Image Index (`application/vnd.oci.image.index.v1+json`) is deserialized into the Docker `ManifestList` struct, which lacks OCI-specific fields: `subject`, `artifactType`, and `annotations`. This PR adds a dedicated `OciImageIndex` struct.

### Task 2: Add OCI Image Index fixture and deserialization test

**Files:**
- Create: `tests/fixtures/oci_image_index.json`
- Modify: `tests/manifest.rs`

**Step 1: Create OCI Image Index fixture**

Create `tests/fixtures/oci_image_index.json`:

```json
{
  "schemaVersion": 2,
  "mediaType": "application/vnd.oci.image.index.v1+json",
  "artifactType": "application/vnd.example.sbom.v1",
  "manifests": [
    {
      "mediaType": "application/vnd.oci.image.manifest.v1+json",
      "size": 7143,
      "digest": "sha256:e692418e4cbaf90ca69d05a66403747baa33ee08806650b51fab815ad7fc331f",
      "platform": {
        "architecture": "amd64",
        "os": "linux"
      }
    },
    {
      "mediaType": "application/vnd.oci.image.manifest.v1+json",
      "size": 7682,
      "digest": "sha256:5b0bcabd1ed22e9fb1310cf6c2dec7cdef19f0ad69efa1f392e94a4333501270",
      "platform": {
        "architecture": "arm64",
        "os": "linux",
        "variant": "v8"
      }
    }
  ],
  "subject": {
    "mediaType": "application/vnd.oci.image.manifest.v1+json",
    "size": 1234,
    "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  },
  "annotations": {
    "org.opencontainers.image.created": "2024-01-01T00:00:00Z"
  }
}
```

**Step 2: Write failing deserialization test**

Add to `tests/manifest.rs`:

```rust
#[test]
fn test_deserialize_oci_image_index() {
  let f = fs::File::open("tests/fixtures/oci_image_index.json").expect("Missing fixture");
  let bufrd = io::BufReader::new(f);
  let index: docker_registry::v2::manifest::OciImageIndex = serde_json::from_reader(bufrd).unwrap();

  assert_eq!(index.artifact_type(), Some("application/vnd.example.sbom.v1"));
  assert_eq!(index.manifests.len(), 2);
  assert_eq!(index.architectures(), vec!["amd64", "arm64"]);
  assert!(index.subject().is_some());
  assert_eq!(
    index.annotations().unwrap().get("org.opencontainers.image.created").unwrap(),
    "2024-01-01T00:00:00Z"
  );
}
```

Run: `cargo test test_deserialize_oci_image_index`
Expected: FAIL — `OciImageIndex` doesn't exist.

### Task 3: Create OciImageIndex struct

**Files:**
- Create: `src/v2/manifest/oci_image_index.rs`
- Modify: `src/v2/manifest/mod.rs`

**Step 1: Create the OCI Image Index module**

Create `src/v2/manifest/oci_image_index.rs`:

```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::manifest_schema2::{ManifestObj, Platform};

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
```

**Step 2: Wire up the module and update Manifest enum**

In `src/v2/manifest/mod.rs`:

1. Add the module declaration after the existing ones:

```rust
mod oci_image_index;
pub use self::oci_image_index::{OciDescriptor, OciImageIndex};
```

2. Add variant to `Manifest` enum:

```rust
pub enum Manifest {
  S1Signed(manifest_schema1::ManifestSchema1Signed),
  S2(manifest_schema2::ManifestSchema2),
  ML(manifest_schema2::ManifestList),
  OciIndex(oci_image_index::OciImageIndex),
}
```

3. Update `layers_digests()` — add arm for `OciIndex`:

```rust
(Manifest::OciIndex(m), _, _) => Ok(m.get_digests()),
```

4. Update `architectures()` — add arm for `OciIndex`:

```rust
Manifest::OciIndex(m) => Ok(m.architectures()),
```

5. Update `get_manifest_and_ref()` to deserialize OCI indexes into the new type:

```rust
mediatypes::MediaTypes::ManifestList => {
  Ok((res.json::<ManifestList>().await.map(Manifest::ML)?, content_digest))
}
mediatypes::MediaTypes::OciImageIndexV1 => {
  Ok((res.json::<OciImageIndex>().await.map(Manifest::OciIndex)?, content_digest))
}
```

(Split the current combined `ManifestList | OciImageIndexV1` arm.)

**Step 3: Update re-exports in mod.rs**

Update the `pub use` line for `manifest_schema2` to include the module, and add the new re-export:

```rust
pub use self::manifest_schema2::{
  ConfigBlob, ManifestList, ManifestObj, ManifestSchema2, ManifestSchema2Spec, Platform,
};
pub use self::oci_image_index::{OciDescriptor, OciImageIndex};
```

**Step 4: Run tests**

Run: `cargo test`
Expected: All tests pass including the new `test_deserialize_oci_image_index`.

**Step 5: Commit**

```bash
git add src/v2/manifest/oci_image_index.rs src/v2/manifest/mod.rs tests/fixtures/oci_image_index.json tests/manifest.rs
git commit -m "feat: Add OCI Image Index as a first-class manifest type

Introduce OciImageIndex struct with support for subject, artifactType,
and annotations fields per OCI image-spec. OCI Image Indexes are now
deserialized into their own type (Manifest::OciIndex) instead of being
aliased to the Docker ManifestList."
```

---

## PR 3: OCI Referrers API

### Task 4: Add mock test for referrers endpoint

**Files:**
- Create: `tests/mock/referrers.rs`
- Modify: `tests/mock/mod.rs`

**Step 1: Write failing integration test**

Create `tests/mock/referrers.rs`:

```rust
use docker_registry::v2::manifest::OciImageIndex;

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
```

Add to `tests/mock/mod.rs`:

```rust
mod referrers;
```

**Step 2: Run tests to verify they fail**

Run: `cargo test test_get_referrers`
Expected: FAIL — `get_referrers` method doesn't exist.

### Task 5: Implement referrers API

**Files:**
- Create: `src/v2/referrers.rs`
- Modify: `src/v2/mod.rs`

**Step 1: Create the referrers module**

Create `src/v2/referrers.rs`:

```rust
use log::trace;
use reqwest::{self, Method, StatusCode};

use crate::{
  errors::{Error, Result},
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
      StatusCode::NOT_FOUND => Err(ApiErrors::from(resp).await),
      _ => Err(ApiErrors::from(resp).await),
    }
  }
}
```

**Step 2: Register the module**

In `src/v2/mod.rs`, add after `mod blobs;`:

```rust
mod referrers;
```

**Step 3: Run tests**

Run: `cargo test test_get_referrers`
Expected: All 3 tests pass.

**Step 4: Run full suite**

Run: `cargo test && cargo fmt -- --check && cargo clippy --all-targets --all-features`
Expected: All pass.

**Step 5: Commit**

```bash
git add src/v2/referrers.rs src/v2/mod.rs tests/mock/referrers.rs tests/mock/mod.rs
git commit -m "feat: Add OCI Referrers API support

Implement GET /v2/<name>/referrers/<digest> endpoint per OCI
distribution spec. Supports optional artifactType filtering.
Returns an OciImageIndex containing referrer descriptors."
```
