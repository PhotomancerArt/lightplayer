//! An in-memory [`EngineCache`] (tests, sims, hosts without OPFS; lp-cli
//! later).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpc_firmware_release::sha256_hex;

use crate::engine_cache::{EngineCache, EngineCacheError, LocalBoxFuture, verify_engine_bytes};
use crate::engine_cache_entry::EngineCacheEntry;
use crate::engine_cache_index::{ENGINE_CACHE_DEFAULT_BOUND_BYTES, EngineCacheIndex};

/// Engines in a map, bounded like the browser's. Cloning shares the store.
#[derive(Clone)]
pub struct MemoryEngineCache {
    inner: Rc<RefCell<MemoryInner>>,
}

struct MemoryInner {
    engines: BTreeMap<String, Vec<u8>>,
    index: EngineCacheIndex,
    bound_bytes: u64,
    /// Test knob: every put fails as unavailable.
    unavailable: bool,
}

impl Default for MemoryEngineCache {
    fn default() -> Self {
        Self::with_bound(ENGINE_CACHE_DEFAULT_BOUND_BYTES)
    }
}

impl MemoryEngineCache {
    /// An empty cache with the default bound (64 MiB).
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty cache holding at most `bound_bytes` of unheld engines.
    pub fn with_bound(bound_bytes: u64) -> Self {
        Self {
            inner: Rc::new(RefCell::new(MemoryInner {
                engines: BTreeMap::new(),
                index: EngineCacheIndex::default(),
                bound_bytes,
                unavailable: false,
            })),
        }
    }

    /// A cache whose puts fail (the "storage unavailable" path).
    pub fn unavailable() -> Self {
        let cache = Self::default();
        cache.inner.borrow_mut().unavailable = true;
        cache
    }

    /// Test knob: flip a byte of a stored engine, as a damaged disk would.
    /// `false` when the engine is not stored.
    pub fn corrupt(&self, sha256: &str) -> bool {
        match self.inner.borrow_mut().engines.get_mut(sha256) {
            Some(bytes) if !bytes.is_empty() => {
                bytes[0] ^= 0xff;
                true
            }
            _ => false,
        }
    }

    fn put_now(&self, entry: EngineCacheEntry, bytes: Vec<u8>) -> Result<(), EngineCacheError> {
        verify_engine_bytes(&entry, &bytes)?;
        let mut inner = self.inner.borrow_mut();
        if inner.unavailable {
            return Err(EngineCacheError::Unavailable("no storage".to_string()));
        }
        let sha256 = entry.sha256.clone();
        inner.engines.insert(sha256.clone(), bytes);
        inner.index.upsert(entry);
        let victims = inner.index.eviction_victims(inner.bound_bytes, &sha256);
        for victim in victims {
            inner.engines.remove(&victim);
            inner.index.remove(&victim);
        }
        Ok(())
    }

    fn get_now(&self, sha256: &str, now: f64) -> Result<Vec<u8>, EngineCacheError> {
        let mut inner = self.inner.borrow_mut();
        let Some(bytes) = inner.engines.get(sha256).cloned() else {
            return Err(EngineCacheError::Missing(sha256.to_string()));
        };
        if sha256_hex(&bytes) != sha256 {
            log::warn!("engine cache: {sha256} was corrupt; dropped");
            inner.engines.remove(sha256);
            inner.index.remove(sha256);
            return Err(EngineCacheError::Missing(sha256.to_string()));
        }
        inner.index.touch(sha256, now);
        Ok(bytes)
    }
}

impl EngineCache for MemoryEngineCache {
    fn put(
        &self,
        entry: EngineCacheEntry,
        bytes: Vec<u8>,
    ) -> LocalBoxFuture<'_, Result<(), EngineCacheError>> {
        Box::pin(std::future::ready(self.put_now(entry, bytes)))
    }

    fn get(&self, sha256: &str, now: f64) -> LocalBoxFuture<'_, Result<Vec<u8>, EngineCacheError>> {
        Box::pin(std::future::ready(self.get_now(sha256, now)))
    }

    fn has(&self, sha256: &str) -> LocalBoxFuture<'_, bool> {
        let has = self.inner.borrow().index.get(sha256).is_some();
        Box::pin(std::future::ready(has))
    }

    fn index(&self) -> LocalBoxFuture<'_, EngineCacheIndex> {
        Box::pin(std::future::ready(self.inner.borrow().index.clone()))
    }

    fn set_held(
        &self,
        sha256: &str,
        held: bool,
    ) -> LocalBoxFuture<'_, Result<(), EngineCacheError>> {
        let result = if self.inner.borrow_mut().index.set_held(sha256, held) {
            Ok(())
        } else {
            Err(EngineCacheError::Missing(sha256.to_string()))
        };
        Box::pin(std::future::ready(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_cache::check_engine_cache_contract;
    use crate::engine_cache_entry::EngineSource;
    use crate::test_block_on::block_on;

    #[test]
    fn the_memory_cache_keeps_the_contract() {
        block_on(check_engine_cache_contract(
            &MemoryEngineCache::with_bound(3 * 1024),
            3 * 1024,
        ));
    }

    #[test]
    fn an_unavailable_cache_refuses_puts() {
        let cache = MemoryEngineCache::unavailable();
        let bytes = vec![1, 2, 3];
        let entry = EngineCacheEntry::new(sha256_hex(&bytes), 3, EngineSource::Installed, 1.0);
        assert!(matches!(
            block_on(cache.put(entry.clone(), bytes)),
            Err(EngineCacheError::Unavailable(_))
        ));
        assert!(!block_on(cache.has(&entry.sha256)));
    }

    #[test]
    fn a_corrupt_engine_is_dropped_and_reported_missing() {
        let cache = MemoryEngineCache::new();
        let bytes = vec![7; 64];
        let entry = EngineCacheEntry::new(sha256_hex(&bytes), 64, EngineSource::ReadBack, 1.0);
        block_on(cache.put(entry.clone(), bytes)).unwrap();
        assert!(cache.corrupt(&entry.sha256));
        assert_eq!(
            block_on(cache.get(&entry.sha256, 2.0)),
            Err(EngineCacheError::Missing(entry.sha256.clone()))
        );
        assert!(!block_on(cache.has(&entry.sha256)));
        assert!(block_on(cache.index()).entries.is_empty());
    }

    #[test]
    fn a_just_put_engine_larger_than_the_bound_stays() {
        let cache = MemoryEngineCache::with_bound(10);
        let small = vec![1; 8];
        block_on(cache.put(
            EngineCacheEntry::new(sha256_hex(&small), 8, EngineSource::Fetched, 1.0),
            small.clone(),
        ))
        .unwrap();
        let big = vec![2; 100];
        let big_sha = sha256_hex(&big);
        block_on(cache.put(
            EngineCacheEntry::new(big_sha.clone(), 100, EngineSource::Fetched, 2.0),
            big,
        ))
        .unwrap();
        assert!(block_on(cache.has(&big_sha)));
        assert!(!block_on(cache.has(&sha256_hex(&small))));
    }
}
