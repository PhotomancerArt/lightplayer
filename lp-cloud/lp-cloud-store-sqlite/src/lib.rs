//! Persistent adapters for the `lp-cloud-domain` ports: a SQLite
//! `MetaStore` and two `BlobStore`s.
//!
//! Three things live here, and they are deliberately separate planes:
//!
//! - [`SqliteMetaStore`] — every piece of service state (users, sessions,
//!   projects, membership, the head frontier, sidecars, the event log, the
//!   blob index) in one SQLite file, which is one consistency domain and
//!   one file to back up.
//! - [`FsBlobStore`] — blob bytes as content-addressed files under a
//!   directory. The default, and what the tests use.
//! - [`S3BlobStore`] — the same bytes in an S3-compatible bucket (Tigris),
//!   behind the `s3` feature (on by default).
//!
//! # Fail-fast
//!
//! The ports are **infallible by design** — the domain's error vocabulary
//! is what a client is told, and a disk that stopped answering is not one
//! of those things. So every adapter here **panics** when its backend
//! fails, with a message naming the operation. That is the whole
//! backend-failure policy, and [`store_fatal`] is where it is written down
//! and argued for. Recovery is the process supervisor plus a Litestream
//! restore, not a `Result` threaded through the domain.
//!
//! # The conformance suite is the contract
//!
//! These adapters and the in-memory ones in `lp-cloud-store-mem` are held
//! to one shared test battery (`tests/store_conformance.rs`): the same
//! checks run against the fake and against the real thing. A fake that
//! drifts from the real store is a data-corruption bug that keeps the tests
//! green, which is exactly the failure that suite exists to catch.
//!
//! The two adapters differ in exactly one deliberate way, pinned by a test
//! in [`sqlite_meta_store`]: foreign keys are enforced here, so a child row
//! written before its project is fatal rather than accepted.

pub mod blob_layout;
pub mod fs_blob_store;
pub mod migrations;
pub mod sqlite_meta_store;
pub mod store_fatal;

#[cfg(feature = "s3")]
pub mod s3_blob_store;

pub use blob_layout::blob_object_path;
pub use fs_blob_store::FsBlobStore;
pub use migrations::{MigrationError, run_migrations};
pub use sqlite_meta_store::SqliteMetaStore;

#[cfg(feature = "s3")]
pub use s3_blob_store::{S3BlobStore, S3Config};
