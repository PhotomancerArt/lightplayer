//! The lookup itself: `latest` → a version, a version → its validated
//! manifest, a named file → bytes checked against that manifest.
//!
//! Every byte that leaves here has been checked: a manifest parses, validates
//! and names the target and version it was asked for; a file has the length
//! and SHA-256 the manifest gives it. Anything upstream sends that fails
//! those checks is a [`LookupFailure::BadUpstream`] and is never cached.
//!
//! The blob store (where checked files are kept by SHA-256) is reached by
//! the route through [`AppState::with_service`](crate::AppState::with_service);
//! this type holds the upstreams and the in-memory caches. The release index
//! (`/api/v1/firmware/<target>/releases`) is built on the same plane, in
//! [`release_index_plane`](super::release_index_plane).

use std::sync::{Arc, Mutex};

use lpc_firmware_release::{
    OTA_MANIFEST_FILE, OtaManifest, ReleaseVersion, TargetName, asset_name, sha256_hex,
};

use super::firmware_manifest_cache::{CachedManifest, FirmwareManifestCache, MissKey};
use super::firmware_upstream::{FirmwareUpstream, UpstreamAsset, UpstreamError};
use super::release_index_cache::ReleaseIndexCache;
use super::release_list_upstream::ReleaseListUpstream;

/// Why a lookup has no bytes to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupFailure {
    /// A 404, with the one-line reason for the body.
    NotFound(&'static str),
    /// Upstream could not be asked (502, or 504 for a timeout).
    Upstream(UpstreamError),
    /// Upstream answered something that does not check out (502). Logged,
    /// never cached.
    BadUpstream(String),
}

/// The upstreams and what the plane remembers.
pub struct FirmwarePlane {
    upstream: Arc<dyn FirmwareUpstream>,
    cache: Mutex<FirmwareManifestCache>,
    /// The releases list, for the release index
    /// ([`release_index_plane`](super::release_index_plane)).
    pub(super) list_upstream: Arc<dyn ReleaseListUpstream>,
    pub(super) index_cache: Mutex<ReleaseIndexCache>,
    /// Held while the list is fetched, so concurrent requests at expiry
    /// share one upstream call.
    pub(super) list_refresh: tokio::sync::Mutex<()>,
}

impl FirmwarePlane {
    /// A plane fetching files through `upstream` and the releases list
    /// through `list_upstream`, with empty caches.
    pub fn new(
        upstream: Arc<dyn FirmwareUpstream>,
        list_upstream: Arc<dyn ReleaseListUpstream>,
    ) -> Self {
        Self {
            upstream,
            cache: Mutex::new(FirmwareManifestCache::new()),
            list_upstream,
            index_cache: Mutex::new(ReleaseIndexCache::new()),
            list_refresh: tokio::sync::Mutex::new(()),
        }
    }

    /// The upstream files are fetched through.
    pub fn upstream(&self) -> Arc<dyn FirmwareUpstream> {
        Arc::clone(&self.upstream)
    }

    /// The upstream the releases list is fetched through.
    pub fn list_upstream(&self) -> Arc<dyn ReleaseListUpstream> {
        Arc::clone(&self.list_upstream)
    }

    /// The version `latest` is for `target` (GitHub's "Latest" release, which
    /// is the newest release that carries firmware). Resolving it also caches
    /// that release's manifest.
    pub async fn resolve_latest(
        &self,
        target: &TargetName,
        now: f64,
    ) -> Result<ReleaseVersion, LookupFailure> {
        let miss = MissKey::Latest(target.to_string());
        {
            let cache = self.lock();
            if let Some(version) = cache.latest(target.as_str(), now) {
                return ReleaseVersion::parse(&version).ok_or(LookupFailure::BadUpstream(format!(
                    "cached latest {version}"
                )));
            }
            if cache.is_missing(&miss, now) {
                return Err(LookupFailure::NotFound("no release carries this target"));
            }
        }
        let asset = asset_name(target, OTA_MANIFEST_FILE);
        let Some(bytes) = self
            .upstream
            .fetch(UpstreamAsset::Latest { asset })
            .await
            .map_err(LookupFailure::Upstream)?
        else {
            self.lock().put_missing(miss, now);
            return Err(LookupFailure::NotFound("no release carries this target"));
        };
        let cached = checked_manifest(bytes, target, None)?;
        let version = ReleaseVersion::parse(&cached.manifest.version).ok_or_else(|| {
            LookupFailure::BadUpstream(format!(
                "latest manifest for {target} has a non-release version {}",
                cached.manifest.version
            ))
        })?;
        let mut cache = self.lock();
        cache.put_manifest(target.as_str(), version.as_str(), Arc::new(cached));
        cache.put_latest(target.as_str(), version.as_str(), now);
        Ok(version)
    }

    /// The validated manifest of `target` in release `version`.
    pub async fn manifest(
        &self,
        target: &TargetName,
        version: &ReleaseVersion,
        now: f64,
    ) -> Result<Arc<CachedManifest>, LookupFailure> {
        let miss = MissKey::Version(target.to_string(), version.to_string());
        {
            let cache = self.lock();
            if let Some(cached) = cache.manifest(target.as_str(), version.as_str()) {
                return Ok(cached);
            }
            if cache.is_missing(&miss, now) {
                return Err(LookupFailure::NotFound("no such release"));
            }
        }
        let asset = asset_name(target, OTA_MANIFEST_FILE);
        let Some(bytes) = self
            .upstream
            .fetch(UpstreamAsset::Release {
                version: version.clone(),
                asset,
            })
            .await
            .map_err(LookupFailure::Upstream)?
        else {
            self.lock().put_missing(miss, now);
            return Err(LookupFailure::NotFound("no such release"));
        };
        let cached = Arc::new(checked_manifest(bytes, target, Some(version))?);
        self.lock()
            .put_manifest(target.as_str(), version.as_str(), Arc::clone(&cached));
        Ok(cached)
    }

    /// Fetch `file` of `target` in `version` from upstream and check it
    /// against `manifest`. The caller has already found the file in the
    /// manifest and missed the blob store.
    pub async fn fetch_file(
        &self,
        target: &TargetName,
        version: &ReleaseVersion,
        manifest: &OtaManifest,
        file: &str,
    ) -> Result<Vec<u8>, LookupFailure> {
        let asset = asset_name(target, file);
        let bytes = self
            .upstream
            .fetch(UpstreamAsset::Release {
                version: version.clone(),
                asset: asset.clone(),
            })
            .await
            .map_err(LookupFailure::Upstream)?
            .ok_or_else(|| {
                LookupFailure::BadUpstream(format!(
                    "{} has no asset {asset} although its manifest names {file}",
                    version.tag()
                ))
            })?;
        manifest
            .verify(file, &bytes)
            .map_err(|e| LookupFailure::BadUpstream(format!("{} {asset}: {e}", version.tag())))?;
        Ok(bytes)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FirmwareManifestCache> {
        // Every critical section is a map operation with no await inside, so
        // a poisoned lock can only follow a panic in this file; the cache
        // holds nothing that cannot be refetched.
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Parse and validate upstream manifest bytes, and check they describe the
/// target (and version) that was asked for.
fn checked_manifest(
    bytes: Vec<u8>,
    target: &TargetName,
    version: Option<&ReleaseVersion>,
) -> Result<CachedManifest, LookupFailure> {
    let manifest = OtaManifest::parse_valid(&bytes)
        .map_err(|e| LookupFailure::BadUpstream(format!("{target}.{OTA_MANIFEST_FILE}: {e}")))?;
    if manifest.target != target.as_str() {
        return Err(LookupFailure::BadUpstream(format!(
            "{target}.{OTA_MANIFEST_FILE} names target {}",
            manifest.target
        )));
    }
    if let Some(version) = version
        && manifest.version != version.as_str()
    {
        return Err(LookupFailure::BadUpstream(format!(
            "{}'s {target}.{OTA_MANIFEST_FILE} names version {}",
            version.tag(),
            manifest.version
        )));
    }
    Ok(CachedManifest {
        sha256: sha256_hex(&bytes),
        bytes,
        manifest,
    })
}
