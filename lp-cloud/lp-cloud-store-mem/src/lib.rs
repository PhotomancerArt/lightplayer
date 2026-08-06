//! In-memory adapters for every `lp-cloud-domain` port.
//!
//! This is the local-dev and test backend (D13), and it is **first-class,
//! not a stub**: every port method is implemented for real, and the domain's
//! own unit tests run against it. A fake that diverges from the real store
//! is the classic corruption source, so the SQLite adapter (P04) and this
//! one are held to a shared conformance suite.
//!
//! Everything is a `BTreeMap`, deliberately: iteration order is the key
//! order, so a listing is stable across runs and a test that asserts on one
//! is not asserting on a hash seed.

#![no_std]

extern crate alloc;

pub mod mem_blob_store;
pub mod mem_clock;
pub mod mem_id_mint;
pub mod mem_meta_store;

pub use mem_blob_store::MemBlobStore;
pub use mem_clock::MemClock;
pub use mem_id_mint::MemIdMint;
pub use mem_meta_store::MemMetaStore;
