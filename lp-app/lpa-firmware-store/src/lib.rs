//! Studio's firmware store: the engine cache seam and the store client.
//!
//! - **The engine cache** ([`EngineCache`]): every engine Studio installs,
//!   fetches or reads back, kept by SHA-256 (`engine.bin` as flashed — the
//!   core's digest slot, the board's `engineSha256`), bounded at 64 MiB,
//!   evicted least-recently-used first, never a `held` entry. Its index is
//!   [`EngineCacheIndex`] (`"format": 1`, cache-like). The browser's backend
//!   is OPFS in `lpa-studio-web`; [`MemoryEngineCache`] serves tests, sims
//!   and hosts without OPFS.
//! - **The store client** ([`FirmwareStore`]): lookup URLs at an origin
//!   ([`DEFAULT_FIRMWARE_STORE_ORIGIN`], or a dev flag's) and verification of
//!   everything fetched, over an injected [`FirmwareFetch`].
//! - [`fetch_engine_from_store`]: an engine for a board's target and build
//!   id, checked against the hash the board reported.
//!
//! Sans-IO: no executor, no clock (time is the caller's f64 epoch seconds),
//! no HTTP client. Ordering the sources (cache → store → read-back) is the
//! update protocol's host crate's job; it emits effects, and the edge
//! answers them with this crate. The two never depend on each other.

mod engine_cache;
mod engine_cache_entry;
mod engine_cache_index;
mod fetch_engine_from_store;
mod firmware_fetch;
mod firmware_store;
mod memory_engine_cache;
#[cfg(test)]
mod test_block_on;
#[cfg(test)]
mod test_fetch;

pub use engine_cache::{
    EngineCache, EngineCacheError, LocalBoxFuture, check_engine_cache_contract, verify_engine_bytes,
};
pub use engine_cache_entry::{EngineCacheEntry, EngineSource};
pub use engine_cache_index::{
    ENGINE_CACHE_DEFAULT_BOUND_BYTES, ENGINE_CACHE_INDEX_FORMAT, EngineCacheIndex,
};
pub use fetch_engine_from_store::fetch_engine_from_store;
pub use firmware_fetch::{FetchError, FirmwareFetch};
pub use firmware_store::{DEFAULT_FIRMWARE_STORE_ORIGIN, FirmwareStore, StoreError};
pub use memory_engine_cache::MemoryEngineCache;
