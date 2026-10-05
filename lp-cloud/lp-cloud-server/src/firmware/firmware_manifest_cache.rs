//! Manifests, `latest` and misses, held in memory.
//!
//! Sans-IO: every call takes the caller's `now` (f64 epoch seconds, from the
//! edge's [`SystemClock`](crate::ports::SystemClock)), so expiry is testable
//! without waiting. A restart empties it; that only costs refetches, since
//! the files themselves live in the blob store.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use lpc_firmware_release::OtaManifest;

/// How many `(target, version)` manifests are kept (oldest evicted first).
/// A manifest is a few KB.
pub const MANIFEST_CAPACITY: usize = 256;

/// How long `latest` → version is trusted: a new release takes this long to
/// become `latest` here.
pub const LATEST_TTL_SECONDS: f64 = 300.0;

/// How long a miss (upstream 404) is remembered: a release whose assets are
/// still uploading answers 404 for at most this long after they land.
pub const MISSING_TTL_SECONDS: f64 = 60.0;

/// How many misses are remembered at once. Versions are attacker-chosen
/// strings (within the grammar), so this table is bounded too.
pub const MISSING_CAPACITY: usize = 1024;

/// One release's manifest: the exact upstream bytes (served as they are,
/// never re-serialized), their SHA-256, and the parsed, validated form.
#[derive(Debug)]
pub struct CachedManifest {
    /// The bytes as upstream sent them.
    pub bytes: Vec<u8>,
    /// SHA-256 of `bytes`, lowercase hex: the manifest's ETag.
    pub sha256: String,
    /// The parsed manifest (already validated).
    pub manifest: OtaManifest,
}

/// What a miss was for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MissKey {
    /// `latest` for a target had no manifest.
    Latest(String),
    /// `(target, version)` had no manifest.
    Version(String, String),
}

/// The plane's memory.
#[derive(Debug, Default)]
pub struct FirmwareManifestCache {
    manifests: BTreeMap<(String, String), Arc<CachedManifest>>,
    order: VecDeque<(String, String)>,
    latest: BTreeMap<String, (String, f64)>,
    missing: BTreeMap<MissKey, f64>,
}

impl FirmwareManifestCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// The manifest for `(target, version)`, if held.
    pub fn manifest(&self, target: &str, version: &str) -> Option<Arc<CachedManifest>> {
        self.manifests
            .get(&(target.to_string(), version.to_string()))
            .cloned()
    }

    /// Hold a manifest, evicting the oldest past [`MANIFEST_CAPACITY`].
    /// Also forgets a remembered miss for it.
    pub fn put_manifest(&mut self, target: &str, version: &str, manifest: Arc<CachedManifest>) {
        let key = (target.to_string(), version.to_string());
        self.missing
            .remove(&MissKey::Version(key.0.clone(), key.1.clone()));
        if self.manifests.insert(key.clone(), manifest).is_none() {
            self.order.push_back(key);
        }
        while self.manifests.len() > MANIFEST_CAPACITY {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.manifests.remove(&oldest);
        }
    }

    /// The version `latest` resolved to for `target`, while it is fresh.
    pub fn latest(&self, target: &str, now: f64) -> Option<String> {
        self.latest
            .get(target)
            .filter(|(_, expires)| now < *expires)
            .map(|(version, _)| version.clone())
    }

    /// Remember what `latest` is for `target`, for [`LATEST_TTL_SECONDS`].
    pub fn put_latest(&mut self, target: &str, version: &str, now: f64) {
        self.missing.remove(&MissKey::Latest(target.to_string()));
        self.latest.insert(
            target.to_string(),
            (version.to_string(), now + LATEST_TTL_SECONDS),
        );
    }

    /// Whether a fresh miss is remembered for `key`.
    pub fn is_missing(&self, key: &MissKey, now: f64) -> bool {
        self.missing.get(key).is_some_and(|expires| now < *expires)
    }

    /// Remember a miss for [`MISSING_TTL_SECONDS`], dropping expired ones
    /// (and then the soonest-expiring, never this one) past
    /// [`MISSING_CAPACITY`].
    pub fn put_missing(&mut self, key: MissKey, now: f64) {
        self.missing.insert(key.clone(), now + MISSING_TTL_SECONDS);
        if self.missing.len() > MISSING_CAPACITY {
            self.missing.retain(|_, expires| now < *expires);
        }
        while self.missing.len() > MISSING_CAPACITY {
            let soonest = self
                .missing
                .iter()
                .filter(|(other, _)| **other != key)
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map(|(key, _)| key.clone());
            match soonest {
                Some(key) => {
                    self.missing.remove(&key);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached(version: &str) -> Arc<CachedManifest> {
        let bytes = include_bytes!(
            "../../../../lp-core/lpc-firmware-release/tests/fixtures/ota-manifest.v1.json"
        )
        .to_vec();
        let mut manifest = OtaManifest::parse(&bytes).unwrap();
        manifest.version = version.to_string();
        Arc::new(CachedManifest {
            sha256: lpc_firmware_release::sha256_hex(&bytes),
            bytes,
            manifest,
        })
    }

    #[test]
    fn latest_expires_after_its_ttl() {
        let mut cache = FirmwareManifestCache::new();
        cache.put_latest("esp32c6-4mb", "2026.10.05-3", 1000.0);
        assert_eq!(
            cache.latest("esp32c6-4mb", 1000.0 + LATEST_TTL_SECONDS - 1.0),
            Some("2026.10.05-3".into())
        );
        assert_eq!(
            cache.latest("esp32c6-4mb", 1000.0 + LATEST_TTL_SECONDS),
            None
        );
        assert_eq!(cache.latest("esp32s3-8mb", 1000.0), None);
    }

    #[test]
    fn misses_expire_and_a_found_manifest_clears_its_miss() {
        let mut cache = FirmwareManifestCache::new();
        let key = MissKey::Version("esp32c6-4mb".into(), "2026.10.05-3".into());
        cache.put_missing(key.clone(), 50.0);
        assert!(cache.is_missing(&key, 50.0 + MISSING_TTL_SECONDS - 1.0));
        assert!(!cache.is_missing(&key, 50.0 + MISSING_TTL_SECONDS));

        cache.put_missing(key.clone(), 100.0);
        cache.put_manifest("esp32c6-4mb", "2026.10.05-3", cached("2026.10.05-3"));
        assert!(!cache.is_missing(&key, 100.0));
    }

    #[test]
    fn manifests_are_bounded_oldest_first() {
        let mut cache = FirmwareManifestCache::new();
        for n in 1..=MANIFEST_CAPACITY + 1 {
            let version = format!("2026.10.05-{n}");
            cache.put_manifest("esp32c6-4mb", &version, cached(&version));
        }
        assert!(cache.manifest("esp32c6-4mb", "2026.10.05-1").is_none());
        assert!(cache.manifest("esp32c6-4mb", "2026.10.05-2").is_some());
        let last = format!("2026.10.05-{}", MANIFEST_CAPACITY + 1);
        assert!(cache.manifest("esp32c6-4mb", &last).is_some());
    }

    #[test]
    fn misses_are_bounded() {
        let mut cache = FirmwareManifestCache::new();
        // All inside one TTL, so nothing has expired: the soonest go first.
        let key = |n: usize| MissKey::Version("esp32c6-4mb".into(), format!("2026.10.05-{n}"));
        let total = MISSING_CAPACITY + 10;
        for n in 1..=total {
            cache.put_missing(key(n), n as f64 * 0.01);
        }
        let now = total as f64 * 0.01;
        assert_eq!(cache.missing.len(), MISSING_CAPACITY);
        assert!(cache.is_missing(&key(total), now));
        assert!(!cache.is_missing(&key(1), now));
        assert!(!cache.is_missing(&key(10), now));
        assert!(cache.is_missing(&key(11), now));
    }
}
