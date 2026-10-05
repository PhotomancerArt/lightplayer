//! The firmware store client: lookup URLs on an origin, and verification of
//! everything that comes back.

use std::fmt;

use lpc_firmware_release::{
    FirmwareFileError, FirmwareLookupPath, OTA_MANIFEST_FILE, OtaManifest, OtaManifestError,
    ReleaseSelector, ReleaseVersion, TargetName,
};

use crate::firmware_fetch::{FetchError, FirmwareFetch};

/// Where Studio finds released firmware: lightplayer.app's `/firmware/`
/// lookup. An absolute origin, so it works from production (same origin),
/// the beta channel and a local `studio-dev` (the route answers CORS `*`).
pub const DEFAULT_FIRMWARE_STORE_ORIGIN: &str = "https://lightplayer.app";

/// Why the store could not answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// The fetch itself failed.
    Fetch(FetchError),
    /// A target name outside the grammar.
    BadTarget(String),
    /// The manifest did not parse or validate.
    BadManifest(OtaManifestError),
    /// The manifest describes something other than what was asked for.
    WrongManifest(String),
    /// A manifest that is not a release's (a dev version) has no files in
    /// the store.
    NotARelease(String),
    /// The store has no such file although the manifest names it.
    MissingFile(String),
    /// A file did not match the manifest.
    BadFile(FirmwareFileError),
    /// The store has this build, but its engine is not the one the board
    /// reported: a fact to surface, never to paper over.
    EngineMismatch {
        /// The hash the board reported.
        expected: String,
        /// The release manifest's `engine.sha256`.
        actual: String,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fetch(e) => write!(f, "{e}"),
            Self::BadTarget(t) => write!(f, "{t:?} is not a target name"),
            Self::BadManifest(e) => write!(f, "{e}"),
            Self::WrongManifest(why) => write!(f, "the store's manifest {why}"),
            Self::NotARelease(v) => write!(f, "{v} is not a release version"),
            Self::MissingFile(file) => write!(f, "the store has no {file}"),
            Self::BadFile(e) => write!(f, "{e}"),
            Self::EngineMismatch { expected, actual } => write!(
                f,
                "the store's engine for this build is {actual}, the board reports {expected}"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<FetchError> for StoreError {
    fn from(e: FetchError) -> Self {
        Self::Fetch(e)
    }
}

/// The firmware store at one origin, read through an injected fetch.
pub struct FirmwareStore<F: FirmwareFetch> {
    origin: String,
    fetch: F,
}

impl<F: FirmwareFetch> FirmwareStore<F> {
    /// A store at `origin` (`https://lightplayer.app`, or a dev flag's
    /// value); a trailing `/` is dropped.
    pub fn new(origin: impl Into<String>, fetch: F) -> Self {
        let origin: String = origin.into();
        Self {
            origin: origin.trim_end_matches('/').to_string(),
            fetch,
        }
    }

    /// The origin lookups go to.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The absolute URL of `file` of `target` in `release` (a build id's
    /// `+` percent-encoded).
    pub fn url(&self, target: &TargetName, release: &ReleaseSelector, file: &str) -> String {
        let path = FirmwareLookupPath {
            target: target.clone(),
            release: release.clone(),
            file: file.to_string(),
        };
        format!("{}{}", self.origin, path.to_path())
    }

    /// The validated manifest of `target` in `release`, or `None` when the
    /// store has none (404). It must name the target asked for, and for a
    /// version or build id, that version or build id; `latest` must be a
    /// release.
    pub async fn manifest(
        &self,
        target: &TargetName,
        release: &ReleaseSelector,
    ) -> Result<Option<OtaManifest>, StoreError> {
        let url = self.url(target, release, OTA_MANIFEST_FILE);
        let Some(bytes) = self.fetch.get(&url).await? else {
            return Ok(None);
        };
        let manifest = OtaManifest::parse_valid(&bytes).map_err(StoreError::BadManifest)?;
        if manifest.target != target.as_str() {
            return Err(StoreError::WrongManifest(format!(
                "names target {}, not {target}",
                manifest.target
            )));
        }
        let matches = match release {
            ReleaseSelector::Latest | ReleaseSelector::Reserved(_) => manifest.is_release(),
            ReleaseSelector::Version(version) => manifest.version == version.as_str(),
            ReleaseSelector::BuildId(build_id) => manifest.build_id() == build_id.to_string(),
        };
        if !matches {
            return Err(StoreError::WrongManifest(format!(
                "is build {}, not {release}",
                manifest.build_id()
            )));
        }
        Ok(Some(manifest))
    }

    /// One file the manifest names, verified against it (length, SHA-256).
    pub async fn file(&self, manifest: &OtaManifest, file: &str) -> Result<Vec<u8>, StoreError> {
        let target = TargetName::parse(&manifest.target)
            .ok_or_else(|| StoreError::BadTarget(manifest.target.clone()))?;
        let version = ReleaseVersion::parse(&manifest.version)
            .ok_or_else(|| StoreError::NotARelease(manifest.version.clone()))?;
        if manifest.file(file).is_none() {
            return Err(StoreError::BadFile(FirmwareFileError::UnknownFile(
                file.to_string(),
            )));
        }
        let url = self.url(&target, &ReleaseSelector::Version(version), file);
        let bytes = self
            .fetch
            .get(&url)
            .await?
            .ok_or_else(|| StoreError::MissingFile(file.to_string()))?;
        manifest.verify(file, &bytes).map_err(StoreError::BadFile)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_block_on::block_on;
    use crate::test_fetch::{FakeFetch, release_fixture};

    #[test]
    fn urls_are_absolute_and_encode_the_build_ids_plus() {
        let store = FirmwareStore::new("https://lightplayer.app/", FakeFetch::default());
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        assert_eq!(
            store.url(&target, &ReleaseSelector::Latest, OTA_MANIFEST_FILE),
            "https://lightplayer.app/firmware/esp32c6-4mb/latest/ota-manifest.json"
        );
        assert_eq!(
            store.url(
                &target,
                &ReleaseSelector::parse("2026.10.05-3").unwrap(),
                "engine.bin"
            ),
            "https://lightplayer.app/firmware/esp32c6-4mb/2026.10.05-3/engine.bin"
        );
        assert_eq!(
            store.url(
                &target,
                &ReleaseSelector::parse("2026.10.05-3+103285d5d05e").unwrap(),
                OTA_MANIFEST_FILE
            ),
            "https://lightplayer.app/firmware/esp32c6-4mb/2026.10.05-3%2B103285d5d05e/ota-manifest.json"
        );
    }

    #[test]
    fn a_404_manifest_is_none_and_a_malformed_one_an_error() {
        let fetch = FakeFetch::default();
        let store = FirmwareStore::new("https://store.test", fetch.clone());
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        assert_eq!(
            block_on(store.manifest(&target, &ReleaseSelector::Latest)),
            Ok(None)
        );

        fetch.put(
            "https://store.test/firmware/esp32c6-4mb/latest/ota-manifest.json",
            b"{\"format\":1}".to_vec(),
        );
        assert!(matches!(
            block_on(store.manifest(&target, &ReleaseSelector::Latest)),
            Err(StoreError::BadManifest(_))
        ));
    }

    #[test]
    fn manifests_must_be_the_one_asked_for() {
        let release = release_fixture();
        let fetch = release.fetch("https://store.test");
        let store = FirmwareStore::new("https://store.test", fetch.clone());
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        let version = ReleaseSelector::parse("2026.10.05-3").unwrap();
        assert_eq!(
            block_on(store.manifest(&target, &version)).unwrap(),
            Some(release.manifest.clone())
        );

        // The store answers another release's manifest under this path.
        let mut other = release.manifest.clone();
        other.version = "2026.10.05-4".into();
        fetch.put(
            "https://store.test/firmware/esp32c6-4mb/2026.10.05-3/ota-manifest.json",
            other.to_json_bytes(),
        );
        assert!(matches!(
            block_on(store.manifest(&target, &version)),
            Err(StoreError::WrongManifest(_))
        ));
    }

    #[test]
    fn files_are_verified_and_a_tampered_one_refused() {
        let release = release_fixture();
        let fetch = release.fetch("https://store.test");
        let store = FirmwareStore::new("https://store.test", fetch.clone());
        assert_eq!(
            block_on(store.file(&release.manifest, "engine.bin")).unwrap(),
            release.engine
        );

        let mut tampered = release.engine.clone();
        tampered[0] ^= 1;
        fetch.put(
            "https://store.test/firmware/esp32c6-4mb/2026.10.05-3/engine.bin",
            tampered,
        );
        assert!(matches!(
            block_on(store.file(&release.manifest, "engine.bin")),
            Err(StoreError::BadFile(FirmwareFileError::HashMismatch { .. }))
        ));
        assert!(matches!(
            block_on(store.file(&release.manifest, "secrets.txt")),
            Err(StoreError::BadFile(FirmwareFileError::UnknownFile(_)))
        ));
        fetch.remove("https://store.test/firmware/esp32c6-4mb/2026.10.05-3/core.bin");
        assert_eq!(
            block_on(store.file(&release.manifest, "core.bin")),
            Err(StoreError::MissingFile("core.bin".into()))
        );
    }

    #[test]
    fn offline_is_a_fetch_error() {
        let fetch = FakeFetch::default();
        fetch.go_offline();
        let store = FirmwareStore::new("https://store.test", fetch);
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        assert!(matches!(
            block_on(store.manifest(&target, &ReleaseSelector::Latest)),
            Err(StoreError::Fetch(FetchError::Offline(_)))
        ));
    }
}
