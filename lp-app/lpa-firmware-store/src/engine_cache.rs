//! The engine cache seam: every engine Studio installs, fetches or reads
//! back, kept by SHA-256 so the common case needs no network.
//!
//! The browser's cache is OPFS (`lpa-studio-web`, `firmware-cache/`);
//! tests, sims and hosts without OPFS use
//! [`MemoryEngineCache`](crate::MemoryEngineCache). Both keep the same
//! contract, and [`check_engine_cache_contract`] is it, as a test either can
//! run.
//!
//! Time is always the caller's (`now`, f64 epoch seconds; on `put` it is in
//! the entry); a cache never reads a clock. Futures are runtime-neutral: no
//! spawns, no executor-flavored sleeps.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use lpc_firmware_release::sha256_hex;

use crate::engine_cache_entry::EngineCacheEntry;
use crate::engine_cache_index::EngineCacheIndex;

/// Single-threaded boxed future, the shape every seam method returns —
/// the same alias as `lpa-studio-core`'s `library_host::LocalBoxFuture`
/// (this crate sits below studio-core, so it cannot import it).
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Why the cache could not do something.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineCacheError {
    /// No storage, or a write or read-back that failed.
    Unavailable(String),
    /// The engine is not cached (or its stored bytes were corrupt and have
    /// been dropped).
    Missing(String),
    /// The bytes do not hash to the entry's key.
    HashMismatch {
        /// The entry's `sha256`.
        expected: String,
        /// SHA-256 of the bytes.
        actual: String,
    },
    /// The bytes are not the entry's length.
    LengthMismatch {
        /// The entry's `length`.
        expected: u64,
        /// The bytes' length.
        actual: u64,
    },
}

impl fmt::Display for EngineCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(why) => write!(f, "the engine cache is unavailable: {why}"),
            Self::Missing(sha) => write!(f, "engine {sha} is not cached"),
            Self::HashMismatch { expected, actual } => {
                write!(f, "engine bytes hash to {actual}, not {expected}")
            }
            Self::LengthMismatch { expected, actual } => {
                write!(f, "engine is {actual} bytes, not {expected}")
            }
        }
    }
}

impl std::error::Error for EngineCacheError {}

/// The seam.
pub trait EngineCache {
    /// Verify SHA-256(bytes) == `entry.sha256` and the length, store, READ
    /// BACK equal, record, then evict to the bound. A second put of a known
    /// hash only refreshes its metadata (the earliest `addedAt` is kept,
    /// `held` is OR-ed). `Ok` only once the bytes were read back equal.
    fn put(
        &self,
        entry: EngineCacheEntry,
        bytes: Vec<u8>,
    ) -> LocalBoxFuture<'_, Result<(), EngineCacheError>>;

    /// The bytes, re-verified against the key — a corrupt file is removed
    /// and reported `Missing` — and `lastUsedAt` set to `now`.
    fn get(&self, sha256: &str, now: f64) -> LocalBoxFuture<'_, Result<Vec<u8>, EngineCacheError>>;

    /// Whether `sha256` is listed.
    fn has(&self, sha256: &str) -> LocalBoxFuture<'_, bool>;

    /// The index (empty when unreadable).
    fn index(&self) -> LocalBoxFuture<'_, EngineCacheIndex>;

    /// Pin (`true`) or release an entry. M7 pins the backup engine of an
    /// in-flight update: for a dev build it is the only copy while its board
    /// is engine-less.
    fn set_held(
        &self,
        sha256: &str,
        held: bool,
    ) -> LocalBoxFuture<'_, Result<(), EngineCacheError>>;
}

/// Check `bytes` against `entry` (length, then SHA-256): what every `put`
/// does before it stores anything.
pub fn verify_engine_bytes(entry: &EngineCacheEntry, bytes: &[u8]) -> Result<(), EngineCacheError> {
    let actual_len = bytes.len() as u64;
    if actual_len != entry.length {
        return Err(EngineCacheError::LengthMismatch {
            expected: entry.length,
            actual: actual_len,
        });
    }
    let actual = sha256_hex(bytes);
    if actual != entry.sha256 {
        return Err(EngineCacheError::HashMismatch {
            expected: entry.sha256.clone(),
            actual,
        });
    }
    Ok(())
}

/// The contract every [`EngineCache`] keeps, as an async check a backend's
/// tests can run: a fresh cache with a bound of at least 3 KiB.
///
/// Puts verify; gets touch; a second put merges; `set_held` on a missing
/// hash is `Missing`; and putting past the bound evicts the least recently
/// used unheld entry, never the one just put.
pub async fn check_engine_cache_contract(cache: &dyn EngineCache, bound_bytes: u64) {
    use crate::engine_cache_entry::EngineSource;

    let engine = |fill: u8, len: usize| vec![fill; len];
    let entry_for = |bytes: &[u8], now: f64| {
        EngineCacheEntry::new(
            sha256_hex(bytes),
            bytes.len() as u64,
            EngineSource::Fetched,
            now,
        )
    };
    let third = (bound_bytes / 3) as usize;

    // A put that does not verify stores nothing.
    let a = engine(0xa0, third);
    let mut wrong = entry_for(&a, 1.0);
    wrong.sha256 = sha256_hex(b"something else");
    assert!(matches!(
        cache.put(wrong.clone(), a.clone()).await,
        Err(EngineCacheError::HashMismatch { .. })
    ));
    assert!(!cache.has(&wrong.sha256).await);
    let mut short = entry_for(&a, 1.0);
    short.length += 1;
    assert!(matches!(
        cache.put(short, a.clone()).await,
        Err(EngineCacheError::LengthMismatch { .. })
    ));

    // Put, has, get (which touches).
    let entry_a = entry_for(&a, 1.0);
    cache.put(entry_a.clone(), a.clone()).await.expect("put a");
    assert!(cache.has(&entry_a.sha256).await);
    assert_eq!(cache.get(&entry_a.sha256, 5.0).await.expect("get a"), a);
    let index = cache.index().await;
    assert_eq!(
        index
            .get(&entry_a.sha256)
            .unwrap()
            .last_used_at_epoch_seconds,
        5.0
    );

    // A second put of a known hash merges: earliest added, held OR-ed.
    // (At 3.0, before the get at 5.0: lastUsedAt stays 5.0.)
    let mut again = entry_for(&a, 3.0);
    again.held = true;
    cache.put(again, a.clone()).await.expect("put a again");
    let index = cache.index().await;
    assert_eq!(index.entries.len(), 1);
    let row = index.get(&entry_a.sha256).unwrap();
    assert_eq!(row.added_at_epoch_seconds, 1.0);
    assert!(row.held);
    cache
        .set_held(&entry_a.sha256, false)
        .await
        .expect("release a");

    assert_eq!(
        cache.set_held(&sha256_hex(b"absent"), true).await,
        Err(EngineCacheError::Missing(sha256_hex(b"absent")))
    );
    assert!(matches!(
        cache.get(&sha256_hex(b"absent"), 6.0).await,
        Err(EngineCacheError::Missing(_))
    ));

    // Fill past the bound: b is held, a was used at 5, c at 7, d is new.
    let b = engine(0xb0, third);
    let entry_b = entry_for(&b, 2.0);
    cache.put(entry_b.clone(), b).await.expect("put b");
    cache.set_held(&entry_b.sha256, true).await.expect("hold b");
    let c = engine(0xc0, third);
    let entry_c = entry_for(&c, 7.0);
    cache.put(entry_c.clone(), c).await.expect("put c");
    let d = engine(0xd0, third);
    let entry_d = entry_for(&d, 8.0);
    cache.put(entry_d.clone(), d.clone()).await.expect("put d");

    assert!(!cache.has(&entry_a.sha256).await, "a: least recently used");
    assert!(cache.has(&entry_b.sha256).await, "b: held");
    assert!(cache.has(&entry_c.sha256).await);
    assert!(cache.has(&entry_d.sha256).await, "d: just put");
    assert!(cache.index().await.total_bytes() <= bound_bytes);
    assert_eq!(cache.get(&entry_d.sha256, 10.0).await.expect("get d"), d);
}
