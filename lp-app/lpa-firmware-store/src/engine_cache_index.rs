//! The engine cache's index (`firmware-cache/index.json`, `"format": 1`) and
//! its eviction policy.
//!
//! **Cache-like:** an unreadable index, or one of a `format` this build does
//! not know, reads as empty — the cache then refills — and an engine file is
//! never deleted *because* the index could not be read.
//!
//! **Eviction is least-recently-used first.** The brief's "oldest first" is
//! read as least recently *used*, so an engine Studio keeps reaching for
//! stays however long ago it arrived. A `held` entry is never evicted, and
//! neither is the entry just put; ties break by `addedAt`, then by hash, so
//! the order is deterministic.

use serde::{Deserialize, Serialize};

use crate::engine_cache_entry::EngineCacheEntry;

/// The index's shape version: `format`, never `version` (N1 — in a new JSON
/// shape, `version` means the app version, and an entry here carries one).
pub const ENGINE_CACHE_INDEX_FORMAT: u32 = 1;

/// How many bytes of engines the cache keeps (~35 C6 engines).
pub const ENGINE_CACHE_DEFAULT_BOUND_BYTES: u64 = 64 * 1024 * 1024;

/// `firmware-cache/index.json`: `{ "format": 1, "entries": [...] }`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EngineCacheIndex {
    /// Always [`ENGINE_CACHE_INDEX_FORMAT`] when written.
    pub format: u32,
    /// One per cached engine.
    #[serde(default)]
    pub entries: Vec<EngineCacheEntry>,
}

impl Default for EngineCacheIndex {
    fn default() -> Self {
        Self {
            format: ENGINE_CACHE_INDEX_FORMAT,
            entries: Vec::new(),
        }
    }
}

impl EngineCacheIndex {
    /// Parse the stored index; anything unreadable, or a `format` this build
    /// does not know, is an empty index.
    pub fn parse(bytes: &[u8]) -> Self {
        match serde_json::from_slice::<EngineCacheIndex>(bytes) {
            Ok(index) if index.format == ENGINE_CACHE_INDEX_FORMAT => index,
            Ok(index) => {
                log::debug!("engine cache index format {} ignored", index.format);
                Self::default()
            }
            Err(error) => {
                log::debug!("engine cache index unreadable ({error}); treated as empty");
                Self::default()
            }
        }
    }

    /// The bytes to store.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).unwrap_or_default()
    }

    /// The entry for `sha256`, if listed.
    pub fn get(&self, sha256: &str) -> Option<&EngineCacheEntry> {
        self.entries.iter().find(|e| e.sha256 == sha256)
    }

    /// Record `entry`. A hash already listed only has its metadata
    /// refreshed: the earliest `addedAt` and the latest `lastUsedAt` win,
    /// `held` is OR-ed, known facts (`target`, `buildId`, `version`) are kept
    /// unless the new entry knows them, and the newest `source` is recorded.
    pub fn upsert(&mut self, entry: EngineCacheEntry) {
        match self.entries.iter_mut().find(|e| e.sha256 == entry.sha256) {
            Some(existing) => {
                existing.length = entry.length;
                existing.added_at_epoch_seconds = existing
                    .added_at_epoch_seconds
                    .min(entry.added_at_epoch_seconds);
                existing.last_used_at_epoch_seconds = existing
                    .last_used_at_epoch_seconds
                    .max(entry.last_used_at_epoch_seconds);
                existing.held |= entry.held;
                existing.source = entry.source;
                if entry.target.is_some() {
                    existing.target = entry.target;
                }
                if entry.build_id.is_some() {
                    existing.build_id = entry.build_id;
                }
                if entry.version.is_some() {
                    existing.version = entry.version;
                }
            }
            None => self.entries.push(entry),
        }
    }

    /// Drop `sha256`'s row; `false` when it was not listed.
    pub fn remove(&mut self, sha256: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.sha256 != sha256);
        self.entries.len() != before
    }

    /// Mark `sha256` used at `now`; `false` when it is not listed.
    pub fn touch(&mut self, sha256: &str, now: f64) -> bool {
        match self.entries.iter_mut().find(|e| e.sha256 == sha256) {
            Some(entry) => {
                entry.last_used_at_epoch_seconds = entry.last_used_at_epoch_seconds.max(now);
                true
            }
            None => false,
        }
    }

    /// Pin or release `sha256`; `false` when it is not listed.
    pub fn set_held(&mut self, sha256: &str, held: bool) -> bool {
        match self.entries.iter_mut().find(|e| e.sha256 == sha256) {
            Some(entry) => {
                entry.held = held;
                true
            }
            None => false,
        }
    }

    /// The bytes every listed engine takes.
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.length).sum()
    }

    /// The hashes to evict so the cache fits `bound_bytes`: least recently
    /// used first (ties: earliest `addedAt`, then hash), never a `held`
    /// entry, never `just_put`. When only those remain the list stops short —
    /// the cache may then sit over its bound.
    pub fn eviction_victims(&self, bound_bytes: u64, just_put: &str) -> Vec<String> {
        let mut total = self.total_bytes();
        if total <= bound_bytes {
            return Vec::new();
        }
        let mut candidates: Vec<&EngineCacheEntry> = self
            .entries
            .iter()
            .filter(|e| !e.held && e.sha256 != just_put)
            .collect();
        candidates.sort_by(|a, b| {
            a.last_used_at_epoch_seconds
                .total_cmp(&b.last_used_at_epoch_seconds)
                .then(
                    a.added_at_epoch_seconds
                        .total_cmp(&b.added_at_epoch_seconds),
                )
                .then(a.sha256.cmp(&b.sha256))
        });
        let mut victims = Vec::new();
        for entry in candidates {
            if total <= bound_bytes {
                break;
            }
            total -= entry.length;
            victims.push(entry.sha256.clone());
        }
        victims
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_cache_entry::EngineSource;

    fn entry(n: u8, length: u64, added: f64, used: f64) -> EngineCacheEntry {
        let mut e = EngineCacheEntry::new(hash(n), length, EngineSource::Fetched, added);
        e.last_used_at_epoch_seconds = used;
        e
    }

    fn hash(n: u8) -> String {
        format!("{n:02x}").repeat(32)
    }

    #[test]
    fn garbage_or_a_foreign_format_reads_as_empty() {
        assert_eq!(
            EngineCacheIndex::parse(b"not json"),
            EngineCacheIndex::default()
        );
        assert_eq!(
            EngineCacheIndex::parse(br#"{"format":2,"entries":[]}"#),
            EngineCacheIndex::default()
        );
        assert_eq!(
            EngineCacheIndex::parse(br#"{"version":1,"entries":[]}"#),
            EngineCacheIndex::default(),
            "the shape version is `format`"
        );
    }

    #[test]
    fn round_trips_and_says_format_1() {
        let mut index = EngineCacheIndex::default();
        let mut e = entry(1, 100, 10.0, 20.0);
        e.target = Some("esp32c6-4mb".into());
        e.build_id = Some("2026.10.05-3+103285d5d05e".into());
        e.version = Some("2026.10.05-3".into());
        e.held = true;
        index.upsert(e);
        index.upsert(entry(2, 50, 11.0, 11.0));
        let bytes = index.to_bytes();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains("\"format\": 1"), "{text}");
        assert!(
            text.contains("\"buildId\": \"2026.10.05-3+103285d5d05e\""),
            "{text}"
        );
        assert!(text.contains("\"source\": \"fetched\""), "{text}");
        assert!(text.contains("\"lastUsedAtEpochSeconds\""), "{text}");
        assert_eq!(EngineCacheIndex::parse(&bytes), index);
    }

    #[test]
    fn upsert_keeps_the_earliest_added_and_ors_held() {
        let mut index = EngineCacheIndex::default();
        let mut first = entry(1, 100, 10.0, 10.0);
        first.held = true;
        first.target = Some("esp32c6-4mb".into());
        index.upsert(first);
        let mut again = entry(1, 100, 30.0, 30.0);
        again.source = EngineSource::ReadBack;
        index.upsert(again);

        assert_eq!(index.entries.len(), 1);
        let e = index.get(&hash(1)).unwrap();
        assert_eq!(e.added_at_epoch_seconds, 10.0);
        assert_eq!(e.last_used_at_epoch_seconds, 30.0);
        assert!(e.held, "held survives a put that does not hold");
        assert_eq!(e.target.as_deref(), Some("esp32c6-4mb"));
        assert_eq!(e.source, EngineSource::ReadBack);
    }

    #[test]
    fn eviction_is_least_recently_used_first() {
        let mut index = EngineCacheIndex::default();
        index.upsert(entry(1, 40, 1.0, 50.0)); // added first, used recently
        index.upsert(entry(2, 40, 2.0, 10.0)); // least recently used
        index.upsert(entry(3, 40, 3.0, 30.0));
        index.upsert(entry(4, 40, 4.0, 60.0)); // just put
        assert_eq!(index.eviction_victims(80, &hash(4)), vec![hash(2), hash(3)]);
        assert_eq!(index.eviction_victims(160, &hash(4)), Vec::<String>::new());
        assert_eq!(index.eviction_victims(120, &hash(4)), vec![hash(2)]);
    }

    #[test]
    fn held_and_just_put_entries_survive_even_over_the_bound() {
        let mut index = EngineCacheIndex::default();
        let mut held = entry(1, 100, 1.0, 1.0);
        held.held = true;
        index.upsert(held);
        index.upsert(entry(2, 100, 2.0, 2.0));
        index.upsert(entry(3, 500, 3.0, 3.0)); // alone exceeds the bound
        assert_eq!(index.eviction_victims(150, &hash(3)), vec![hash(2)]);
        // Only held and just-put remain: stop short, over the bound.
        index.remove(&hash(2));
        assert_eq!(index.eviction_victims(150, &hash(3)), Vec::<String>::new());
    }

    #[test]
    fn eviction_ties_break_by_added_then_hash() {
        let mut index = EngineCacheIndex::default();
        index.upsert(entry(9, 10, 5.0, 7.0));
        index.upsert(entry(8, 10, 5.0, 7.0));
        index.upsert(entry(7, 10, 4.0, 7.0));
        index.upsert(entry(1, 10, 9.0, 9.0));
        assert_eq!(
            index.eviction_victims(10, &hash(1)),
            vec![hash(7), hash(8), hash(9)]
        );
    }

    #[test]
    fn touch_and_set_held_report_missing_hashes() {
        let mut index = EngineCacheIndex::default();
        index.upsert(entry(1, 10, 1.0, 1.0));
        assert!(index.touch(&hash(1), 5.0));
        assert_eq!(index.get(&hash(1)).unwrap().last_used_at_epoch_seconds, 5.0);
        assert!(!index.touch(&hash(2), 5.0));
        assert!(index.set_held(&hash(1), true));
        assert!(!index.set_held(&hash(2), true));
        assert_eq!(index.total_bytes(), 10);
    }
}
