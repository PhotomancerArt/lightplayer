//! The releases list and each target's rendered index, held in memory.
//!
//! Sans-IO, like [`FirmwareManifestCache`](super::firmware_manifest_cache):
//! every call takes the caller's `now` (f64 epoch seconds). A restart
//! empties it; that costs one list fetch and the manifests the index needs.
//!
//! - The list is **trusted for [`LATEST_TTL_SECONDS`]** after it was last
//!   fetched or revalidated, then revalidated with its ETag.
//! - When revalidation fails, the last good list is still served for up to
//!   [`RELEASE_LIST_STALE_SECONDS`] after it was last good, and upstream is
//!   not asked again for [`RELEASE_LIST_RETRY_SECONDS`]. One warning per TTL
//!   says so.
//! - Each target's index is rendered once per list **generation**. A fetch
//!   whose releases equal the held ones (GitHub's ETag also moves with
//!   download counts) keeps the generation, so nothing is rebuilt.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::firmware_manifest_cache::LATEST_TTL_SECONDS;
use super::github_release_list::ListedRelease;

/// How long the last good list is served while upstream keeps failing.
pub const RELEASE_LIST_STALE_SECONDS: f64 = 24.0 * 60.0 * 60.0;

/// After a failed revalidation, how long a held list is served before
/// upstream is asked again (so an outage or a rate limit is not hammered
/// once per request).
pub const RELEASE_LIST_RETRY_SECONDS: f64 = 60.0;

/// A list the plane may build from, and which generation it is.
#[derive(Debug, Clone)]
pub struct HeldReleaseList {
    /// The published releases (drafts, prereleases and odd tags already
    /// skipped).
    pub releases: Arc<Vec<ListedRelease>>,
    /// Bumped whenever the releases change.
    pub generation: u64,
}

/// One target's index as served: the bytes and their SHA-256 (the ETag).
#[derive(Debug)]
pub struct RenderedReleaseIndex {
    /// `ReleaseIndex::to_json_bytes()`.
    pub bytes: Vec<u8>,
    /// SHA-256 of `bytes`, lowercase hex.
    pub sha256: String,
    /// The list generation it was built from.
    pub generation: u64,
}

#[derive(Debug)]
struct ListState {
    releases: Arc<Vec<ListedRelease>>,
    etag: Option<String>,
    generation: u64,
    /// When the list was last fetched or revalidated successfully.
    good_at: f64,
    /// When revalidation last failed, if it has since `good_at`.
    failed_at: Option<f64>,
    /// When the last stale-serving warning was logged.
    warned_at: Option<f64>,
}

/// The release index's memory.
#[derive(Debug, Default)]
pub struct ReleaseIndexCache {
    list: Option<ListState>,
    next_generation: u64,
    indexes: BTreeMap<String, Arc<RenderedReleaseIndex>>,
}

impl ReleaseIndexCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// The held list when it may be used without asking upstream: within its
    /// TTL, or inside the retry pause after a failure (and still within the
    /// stale window).
    pub fn current_list(&self, now: f64) -> Option<HeldReleaseList> {
        let state = self.list.as_ref()?;
        let fresh = now < state.good_at + LATEST_TTL_SECONDS;
        let pausing = state.failed_at.is_some_and(|failed| {
            now < failed + RELEASE_LIST_RETRY_SECONDS
                && now < state.good_at + RELEASE_LIST_STALE_SECONDS
        });
        (fresh || pausing).then(|| held(state))
    }

    /// The ETag of the held list, for a conditional fetch.
    pub fn list_etag(&self) -> Option<String> {
        self.list.as_ref().and_then(|state| state.etag.clone())
    }

    /// Hold a freshly fetched list. The generation moves only when the
    /// releases differ from the ones held.
    pub fn put_list(
        &mut self,
        releases: Vec<ListedRelease>,
        etag: Option<String>,
        now: f64,
    ) -> HeldReleaseList {
        let unchanged = self
            .list
            .as_ref()
            .filter(|state| *state.releases == releases);
        let (releases, generation) = match unchanged {
            Some(state) => (Arc::clone(&state.releases), state.generation),
            None => {
                self.next_generation += 1;
                (Arc::new(releases), self.next_generation)
            }
        };
        let state = ListState {
            releases,
            etag,
            generation,
            good_at: now,
            failed_at: None,
            warned_at: None,
        };
        let out = held(&state);
        self.list = Some(state);
        out
    }

    /// Upstream answered `304`: the held list is good again from `now`.
    /// `None` when nothing is held (a `304` to an unconditional request).
    pub fn revalidated(&mut self, now: f64) -> Option<HeldReleaseList> {
        let state = self.list.as_mut()?;
        state.good_at = now;
        state.failed_at = None;
        state.warned_at = None;
        Some(held(state))
    }

    /// Revalidation failed at `now`: the held list if it is still within
    /// the stale window, and whether this failure should be logged (once per
    /// TTL).
    pub fn stale_list(&mut self, now: f64) -> Option<(HeldReleaseList, bool)> {
        let state = self.list.as_mut()?;
        if now >= state.good_at + RELEASE_LIST_STALE_SECONDS {
            return None;
        }
        state.failed_at = Some(now);
        let warn = state
            .warned_at
            .is_none_or(|warned| now >= warned + LATEST_TTL_SECONDS);
        if warn {
            state.warned_at = Some(now);
        }
        Some((held(state), warn))
    }

    /// How long ago the held list was last good, in seconds.
    pub fn list_age(&self, now: f64) -> Option<f64> {
        self.list.as_ref().map(|state| now - state.good_at)
    }

    /// `target`'s index rendered from list `generation`, if held.
    pub fn index(&self, target: &str, generation: u64) -> Option<Arc<RenderedReleaseIndex>> {
        self.indexes
            .get(target)
            .filter(|index| index.generation == generation)
            .cloned()
    }

    /// `target`'s last rendered index, from any generation: what is served
    /// when a rebuild cannot reach upstream.
    pub fn last_index(&self, target: &str) -> Option<Arc<RenderedReleaseIndex>> {
        self.indexes.get(target).cloned()
    }

    /// Hold `target`'s rendered index. Only a target some release carries is
    /// ever rendered, so this map is bounded by the real targets.
    pub fn put_index(&mut self, target: &str, index: Arc<RenderedReleaseIndex>) {
        self.indexes.insert(target.to_string(), index);
    }
}

fn held(state: &ListState) -> HeldReleaseList {
    HeldReleaseList {
        releases: Arc::clone(&state.releases),
        generation: state.generation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_firmware_release::ReleaseVersion;

    #[test]
    fn the_list_is_trusted_for_its_ttl() {
        let mut cache = ReleaseIndexCache::new();
        assert!(cache.current_list(0.0).is_none());
        cache.put_list(vec![release("2026.10.06-9")], Some("e1".into()), 1000.0);
        assert!(
            cache
                .current_list(1000.0 + LATEST_TTL_SECONDS - 1.0)
                .is_some()
        );
        assert!(cache.current_list(1000.0 + LATEST_TTL_SECONDS).is_none());
        assert_eq!(cache.list_etag().as_deref(), Some("e1"));
        let again = cache.revalidated(2000.0).unwrap();
        assert!(
            cache
                .current_list(2000.0 + LATEST_TTL_SECONDS - 1.0)
                .is_some()
        );
        assert_eq!(again.generation, 1);
    }

    #[test]
    fn an_equal_list_keeps_its_generation_and_a_changed_one_moves_it() {
        let mut cache = ReleaseIndexCache::new();
        let first = cache.put_list(vec![release("2026.10.06-9")], Some("e1".into()), 0.0);
        let same = cache.put_list(vec![release("2026.10.06-9")], Some("e2".into()), 400.0);
        assert_eq!(first.generation, same.generation);
        assert_eq!(cache.list_etag().as_deref(), Some("e2"));
        let changed = cache.put_list(
            vec![release("2026.10.06-10"), release("2026.10.06-9")],
            None,
            800.0,
        );
        assert_ne!(changed.generation, first.generation);
    }

    #[test]
    fn a_failure_serves_the_stale_list_for_a_day_and_pauses_retries() {
        let mut cache = ReleaseIndexCache::new();
        assert!(cache.stale_list(0.0).is_none(), "nothing held");
        cache.put_list(vec![release("2026.10.06-9")], None, 0.0);

        let at = LATEST_TTL_SECONDS + 1.0;
        let (_, warn) = cache.stale_list(at).unwrap();
        assert!(warn, "the first failure warns");
        assert!(
            cache
                .current_list(at + RELEASE_LIST_RETRY_SECONDS - 1.0)
                .is_some()
        );
        assert!(
            cache
                .current_list(at + RELEASE_LIST_RETRY_SECONDS)
                .is_none()
        );
        let (_, warn) = cache.stale_list(at + 100.0).unwrap();
        assert!(!warn, "one warning per TTL");
        let (_, warn) = cache.stale_list(at + LATEST_TTL_SECONDS).unwrap();
        assert!(warn);

        assert!(cache.stale_list(RELEASE_LIST_STALE_SECONDS - 1.0).is_some());
        assert!(cache.stale_list(RELEASE_LIST_STALE_SECONDS).is_none());
        assert!(
            cache
                .current_list(RELEASE_LIST_STALE_SECONDS + 1.0)
                .is_none(),
            "past the stale window the pause no longer serves it"
        );
    }

    #[test]
    fn indexes_belong_to_a_generation() {
        let mut cache = ReleaseIndexCache::new();
        let index = Arc::new(RenderedReleaseIndex {
            bytes: b"{}".to_vec(),
            sha256: "00".repeat(32),
            generation: 3,
        });
        cache.put_index("esp32c6-4mb", Arc::clone(&index));
        assert!(cache.index("esp32c6-4mb", 3).is_some());
        assert!(cache.index("esp32c6-4mb", 4).is_none());
        assert!(cache.last_index("esp32c6-4mb").is_some());
        assert!(cache.last_index("esp32s3-8mb").is_none());
    }

    fn release(version: &str) -> ListedRelease {
        ListedRelease {
            version: ReleaseVersion::parse(version).unwrap(),
            published_at: None,
            uploaded_assets: Default::default(),
        }
    }
}
