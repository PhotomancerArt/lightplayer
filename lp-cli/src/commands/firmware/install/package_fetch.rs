//! Downloading and verifying one target's release package: `package.json`
//! and the merged image it names, from lightplayer.app's public firmware
//! lookup (`lpc_firmware_release::FirmwareLookupPath` — the same grammar
//! and file allowlist the server itself enforces). Every file is checked
//! against the manifest's own length and SHA-256 before anything is
//! written to disk, and nothing is written at all until every file has
//! verified.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lpc_firmware_release::{FirmwareLookupPath, sha256_hex};

use super::super::distribution_manifest::{DistributionManifest, ManifestImage};

/// One file fetch, by the lookup's three segments
/// (`<target>/<release>/<file>`). A trait so `fetch_package`'s verify-then-
/// write logic is tested against an in-memory stub, never a server.
pub trait ReleaseFile {
    fn fetch(&self, target: &str, release: &str, file: &str) -> Result<Vec<u8>>;
}

/// `https://lightplayer.app/firmware/<target>/<release>/<file>`.
pub struct LightplayerLookup {
    client: reqwest::blocking::Client,
    base_url: String,
}

impl LightplayerLookup {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("lp-cli-firmware-install")
            .build()
            .context("building the HTTP client")?;
        Ok(Self {
            client,
            base_url: "https://lightplayer.app".to_string(),
        })
    }
}

impl ReleaseFile for LightplayerLookup {
    fn fetch(&self, target: &str, release: &str, file: &str) -> Result<Vec<u8>> {
        let path = FirmwareLookupPath::from_segments(target, release, file)
            .map_err(|e| anyhow::anyhow!("{target}/{release}/{file} is not a lookup path: {e}"))?;
        let url = format!("{}{}", self.base_url, path.to_path());
        let response = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("GET {url}"))?;
        response
            .bytes()
            .map(|b| b.to_vec())
            .with_context(|| format!("reading the body of {url}"))
    }
}

/// A verified package, written to its own directory: `manifest.json`
/// (`package.json`'s bytes verbatim) and every image it names, named
/// exactly as the manifest says — the shape
/// `lp-cli hardware lpfs migrate --manifest` reads.
#[derive(Debug)]
pub struct FetchedPackage {
    pub dir: PathBuf,
    pub manifest: DistributionManifest,
    pub version: String,
}

/// Fetch `<target>.package.json` for `release` (a lookup segment: `latest`
/// or a concrete version) and every image it names, verifying each file's
/// length and SHA-256 against the manifest before writing it.
pub fn fetch_package(
    source: &dyn ReleaseFile,
    target: &str,
    release: &str,
    out_dir: &Path,
) -> Result<FetchedPackage> {
    let manifest_bytes = source
        .fetch(target, release, "package.json")
        .with_context(|| format!("fetching {target}'s package.json for {release}"))?;
    let manifest = DistributionManifest::parse(&manifest_bytes)
        .with_context(|| format!("{target}/{release}/package.json"))?;
    if manifest.firmware_id != target {
        bail!(
            "{target}/{release}/package.json is for `{}`, not `{target}`",
            manifest.firmware_id
        );
    }
    let version = manifest
        .core_str("version")
        .context("the package's manifest core has no `version`")?
        .to_string();

    // Download and verify everything before writing anything, same rule as
    // `firmware release-check`'s own verification.
    let mut images = Vec::with_capacity(manifest.images.len());
    for image in &manifest.images {
        let bytes = source
            .fetch(target, release, &image.path)
            .with_context(|| format!("fetching {target}/{release}/{}", image.path))?;
        verify_image(image, &bytes)?;
        images.push((image.path.clone(), bytes));
    }

    fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    fs::write(out_dir.join("manifest.json"), &manifest_bytes)
        .with_context(|| format!("writing {}/manifest.json", out_dir.display()))?;
    for (path, bytes) in &images {
        fs::write(out_dir.join(path), bytes)
            .with_context(|| format!("writing {}/{path}", out_dir.display()))?;
    }

    Ok(FetchedPackage {
        dir: out_dir.to_path_buf(),
        manifest,
        version,
    })
}

fn verify_image(image: &ManifestImage, bytes: &[u8]) -> Result<()> {
    let actual_len = bytes.len() as u64;
    if actual_len != image.size_bytes {
        bail!(
            "{}: {actual_len} bytes, the package manifest says {}",
            image.path,
            image.size_bytes
        );
    }
    let actual_sha = sha256_hex(bytes);
    if actual_sha != image.sha256 {
        bail!(
            "{}: sha256 {actual_sha}, the package manifest says {}",
            image.path,
            image.sha256
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// An in-memory [`ReleaseFile`]: no network, so hash-mismatch refusal
    /// and "nothing written until everything verifies" are tested without
    /// a server.
    struct StubSource {
        files: Mutex<BTreeMap<(String, String, String), Vec<u8>>>,
    }

    impl StubSource {
        fn new() -> Self {
            Self {
                files: Mutex::new(BTreeMap::new()),
            }
        }
        fn set(&self, target: &str, release: &str, file: &str, bytes: Vec<u8>) {
            self.files.lock().unwrap().insert(
                (target.to_string(), release.to_string(), file.to_string()),
                bytes,
            );
        }
    }

    impl ReleaseFile for StubSource {
        fn fetch(&self, target: &str, release: &str, file: &str) -> Result<Vec<u8>> {
            self.files
                .lock()
                .unwrap()
                .get(&(target.to_string(), release.to_string(), file.to_string()))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("stub has no {target}/{release}/{file}"))
        }
    }

    fn sample_manifest(image_bytes: &[u8]) -> Vec<u8> {
        let json = serde_json::json!({
            "schemaVersion": 2,
            "firmwareId": "esp32c6-4mb",
            "displayName": "test",
            "generatedAt": "2026-10-05T00:00:00Z",
            "core": {"version": "2026.10.05-3"},
            "flash": {
                "format": "espflash-merged-image", "address": "0x0",
                "flashSizeBytes": 4194304, "erasePolicy": "x",
                "mayAffectDeviceData": true, "resetAfterFlash": true, "notes": ""
            },
            "images": [{
                "path": "fw-esp32c6-merged.bin",
                "address": "0x0",
                "sizeBytes": image_bytes.len(),
                "sha256": sha256_hex(image_bytes),
            }],
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn fetches_verifies_and_writes_the_package() {
        let image = b"pretend firmware bytes".to_vec();
        let source = StubSource::new();
        source.set(
            "esp32c6-4mb",
            "latest",
            "package.json",
            sample_manifest(&image),
        );
        source.set(
            "esp32c6-4mb",
            "latest",
            "fw-esp32c6-merged.bin",
            image.clone(),
        );

        let dir = tempfile::tempdir().unwrap();
        let fetched = fetch_package(&source, "esp32c6-4mb", "latest", dir.path()).unwrap();
        assert_eq!(fetched.version, "2026.10.05-3");
        assert_eq!(
            std::fs::read(dir.path().join("fw-esp32c6-merged.bin")).unwrap(),
            image
        );
        assert!(dir.path().join("manifest.json").exists());
    }

    #[test]
    fn a_hash_mismatch_is_refused_and_writes_nothing() {
        let image = b"pretend firmware bytes".to_vec();
        let source = StubSource::new();
        source.set(
            "esp32c6-4mb",
            "latest",
            "package.json",
            sample_manifest(&image),
        );
        // Tampered bytes: different length AND hash from what the manifest
        // describes.
        source.set(
            "esp32c6-4mb",
            "latest",
            "fw-esp32c6-merged.bin",
            b"tampered bytes, wrong length".to_vec(),
        );

        let dir = tempfile::tempdir().unwrap();
        let error = fetch_package(&source, "esp32c6-4mb", "latest", dir.path()).unwrap_err();
        assert!(
            format!("{error:#}").contains("the package manifest says"),
            "{error:#}"
        );
        assert!(
            !dir.path().join("manifest.json").exists(),
            "nothing written on failure"
        );
    }

    #[test]
    fn a_package_for_the_wrong_target_is_refused() {
        let image = b"x".to_vec();
        let source = StubSource::new();
        // firmwareId inside is "esp32c6-4mb", asked for under "esp32s3-8mb".
        source.set(
            "esp32s3-8mb",
            "latest",
            "package.json",
            sample_manifest(&image),
        );
        let dir = tempfile::tempdir().unwrap();
        let error = fetch_package(&source, "esp32s3-8mb", "latest", dir.path()).unwrap_err();
        assert!(
            format!("{error:#}").contains("is for `esp32c6-4mb`"),
            "{error:#}"
        );
    }

    #[test]
    fn a_missing_file_names_what_is_missing() {
        let source = StubSource::new();
        let dir = tempfile::tempdir().unwrap();
        let error = fetch_package(&source, "esp32c6-4mb", "latest", dir.path()).unwrap_err();
        assert!(format!("{error:#}").contains("package.json"), "{error:#}");
    }
}
