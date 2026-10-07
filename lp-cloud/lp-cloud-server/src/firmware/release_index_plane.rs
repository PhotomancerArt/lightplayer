//! The release index: every release one target can install, built from the
//! releases list and each release's verified manifest.
//!
//! - The list comes from [`ReleaseListUpstream`] (GitHub's REST list), held
//!   in [`ReleaseIndexCache`] for its TTL and revalidated by ETag.
//! - A release is a **candidate** when `<target>.ota-manifest.json` is among
//!   its uploaded assets. Its manifest comes through the plane's own
//!   [`FirmwarePlane::manifest`] (verified, cached), at most
//!   [`MANIFEST_FETCH_CONCURRENCY`] at a time.
//! - A candidate is **listed** only when every file its manifest names is an
//!   uploaded asset too, so no listed version answers a 404 while its upload
//!   is still running.
//! - A manifest that fails verification (or is missing) drops only its
//!   entry, with a warning. A manifest upstream cannot be asked for fails
//!   the rebuild: the target's last good index is served if there is one.
//!
//! Every upstream URL comes from configuration or from a parsed
//! [`ReleaseVersion`]; the request's only input is the parsed target, used
//! to filter asset names (no SSRF).
//!
//! [`ReleaseListUpstream`]: super::release_list_upstream::ReleaseListUpstream
//! [`ReleaseVersion`]: lpc_firmware_release::ReleaseVersion

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, MutexGuard, PoisonError};

use futures_util::StreamExt as _;
use lpc_firmware_release::{
    OTA_MANIFEST_FILE, ReleaseIndex, ReleaseIndexEntry, TargetName, asset_name, sha256_hex,
};

use super::firmware_manifest_cache::CachedManifest;
use super::firmware_plane::{FirmwarePlane, LookupFailure};
use super::github_release_list::{ListedRelease, parse_release_list};
use super::release_index_cache::{HeldReleaseList, ReleaseIndexCache, RenderedReleaseIndex};
use super::release_list_upstream::ReleaseListFetch;

/// How many manifests one index build fetches at once.
pub const MANIFEST_FETCH_CONCURRENCY: usize = 8;

/// The 404 reason for a target no release carries.
pub const NO_RELEASE_FOR_TARGET: &str = "no release carries this target";

/// One candidate's manifest fetch.
type ManifestFetch<'a> = Pin<
    Box<
        dyn Future<
                Output = (
                    &'a ListedRelease,
                    Result<Arc<CachedManifest>, LookupFailure>,
                ),
            > + Send
            + 'a,
    >,
>;

impl FirmwarePlane {
    /// `target`'s release index, rendered: format 1, newest first, only
    /// releases whose every update file is uploaded.
    pub async fn release_index(
        &self,
        target: &TargetName,
        now: f64,
    ) -> Result<Arc<RenderedReleaseIndex>, LookupFailure> {
        let list = self.release_list(now).await?;
        if let Some(index) = self.index_cache().index(target.as_str(), list.generation) {
            return Ok(index);
        }

        let manifest_asset = asset_name(target, OTA_MANIFEST_FILE);
        let candidates: Vec<_> = list
            .releases
            .iter()
            .filter(|release| release.uploaded_assets.contains(&manifest_asset))
            .collect();
        if candidates.is_empty() {
            return Err(LookupFailure::NotFound(NO_RELEASE_FOR_TARGET));
        }

        // Boxed with a named type: an unboxed `async` closure here trips the
        // compiler's higher-ranked `Send` check on the axum handler.
        let fetches: Vec<ManifestFetch<'_>> = candidates
            .into_iter()
            .map(|release| -> ManifestFetch<'_> {
                Box::pin(
                    async move { (release, self.manifest(target, &release.version, now).await) },
                )
            })
            .collect();
        let fetched: Vec<_> = futures_util::stream::iter(fetches)
            .buffered(MANIFEST_FETCH_CONCURRENCY)
            .collect()
            .await;

        let mut entries = Vec::with_capacity(fetched.len());
        for (release, manifest) in fetched {
            let cached = match manifest {
                Ok(cached) => cached,
                Err(LookupFailure::Upstream(error)) => {
                    return match self.index_cache().last_index(target.as_str()) {
                        Some(last) => {
                            log::warn!(
                                "firmware: {target} release index not rebuilt ({}: {error}); serving the last one",
                                release.version.tag()
                            );
                            Ok(last)
                        }
                        None => Err(LookupFailure::Upstream(error)),
                    };
                }
                Err(LookupFailure::NotFound(reason)) => {
                    log::warn!(
                        "firmware: {} lists {manifest_asset} but it is not there ({reason}); left out of the index",
                        release.version.tag()
                    );
                    continue;
                }
                Err(LookupFailure::BadUpstream(detail)) => {
                    log::warn!(
                        "firmware: {} left out of the {target} release index: {detail}",
                        release.version.tag()
                    );
                    continue;
                }
            };
            let missing = cached
                .manifest
                .files()
                .iter()
                .map(|file| asset_name(target, file.file))
                .find(|asset| !release.uploaded_assets.contains(asset));
            if let Some(missing) = missing {
                log::info!(
                    "firmware: {} left out of the {target} release index until {missing} is uploaded",
                    release.version.tag()
                );
                continue;
            }
            entries.push(ReleaseIndexEntry::from_manifest(
                &cached.manifest,
                release.published_at.clone(),
            ));
        }
        if entries.is_empty() {
            return Err(LookupFailure::NotFound(NO_RELEASE_FOR_TARGET));
        }

        let index = ReleaseIndex::newest_first(target, entries);
        index.validate().map_err(|e| {
            LookupFailure::BadUpstream(format!("{target} release index does not validate: {e}"))
        })?;
        let bytes = index.to_json_bytes();
        let rendered = Arc::new(RenderedReleaseIndex {
            sha256: sha256_hex(&bytes),
            bytes,
            generation: list.generation,
        });
        self.index_cache()
            .put_index(target.as_str(), Arc::clone(&rendered));
        Ok(rendered)
    }

    /// The releases list: held, revalidated, or fetched; the last good one
    /// within its stale window when upstream fails.
    async fn release_list(&self, now: f64) -> Result<HeldReleaseList, LookupFailure> {
        if let Some(held) = self.index_cache().current_list(now) {
            return Ok(held);
        }
        let _refresh = self.list_refresh.lock().await;
        if let Some(held) = self.index_cache().current_list(now) {
            return Ok(held);
        }
        let etag = self.index_cache().list_etag();
        let failure = match self.list_upstream.list(etag).await {
            Ok(ReleaseListFetch::NotModified) => match self.index_cache().revalidated(now) {
                Some(held) => return Ok(held),
                None => LookupFailure::BadUpstream(
                    "the releases list answered 304 to an unconditional request".into(),
                ),
            },
            Ok(ReleaseListFetch::Fresh { bytes, etag }) => match parse_release_list(&bytes) {
                Ok(releases) => return Ok(self.index_cache().put_list(releases, etag, now)),
                Err(detail) => LookupFailure::BadUpstream(detail),
            },
            Err(error) => LookupFailure::Upstream(error),
        };
        let mut cache = self.index_cache();
        let age = cache.list_age(now);
        match cache.stale_list(now) {
            Some((held, warn)) => {
                if warn {
                    log::warn!(
                        "firmware: the releases list could not be refreshed ({failure:?}); serving the one from {:.0} s ago",
                        age.unwrap_or_default()
                    );
                }
                Ok(held)
            }
            None => Err(failure),
        }
    }

    fn index_cache(&self) -> MutexGuard<'_, ReleaseIndexCache> {
        // Every critical section is a map operation with no await inside;
        // the cache holds nothing that cannot be refetched.
        self.index_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
