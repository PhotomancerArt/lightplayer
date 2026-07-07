//! Lite versioning with events.
//!
//! `lpc-history` owns project identity, canonical content hashing,
//! content-addressed snapshots, the per-project history event log, and
//! lineage queries. It is pure domain code: no IO beyond a caller-supplied
//! [`lpfs::LpFs`], no clock (timestamps are caller-supplied f64 epoch
//! seconds), no randomness (uid bytes are caller-supplied).
//!
//! # Invariants
//!
//! **History is linear per project.** A project's history is a single line
//! of versions; there is no DAG and no in-project branching. Forks mint a
//! *new project uid* whose history begins with a
//! [`EventKind::ForkedFrom`](event::history_event::EventKind) origin event
//! pointing at the parent project and version. "Diverged" therefore simply
//! means "a hash not present in my line".
//!
//! **The head rule.** Editing the head of a line advances the line; editing
//! anything else forks — lazily, on first save. This crate does not enforce
//! the rule at edit surfaces (that wiring lives in the studio layers); it
//! provides the primitives — [`lineage::project_history::ProjectHistory`]
//! recording saves at the head, and fork constructors for everything else —
//! that make the rule the only expressible behavior.
//!
//! There is no merge, by design, ever. Fast-forward detection
//! ([`lineage::sync_relation::SyncRelation`]) plus fork-as-new-project is
//! the entire model.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod device;
pub mod event;
pub mod hash;
pub mod history_error;
pub mod lineage;
pub mod snapshot;
pub mod uid;

pub use device::device_association::DeviceAssociation;
pub use event::event_log::EventLog;
pub use event::geo_point::GeoPoint;
pub use event::history_event::{EventKind, HistoryEvent};
pub use hash::content_hash::ContentHash;
pub use hash::package_hasher::hash_package;
pub use hash::tree_manifest::{TreeEntry, TreeManifest};
pub use history_error::HistoryError;
pub use lineage::project_history::ProjectHistory;
pub use lineage::sync_relation::SyncRelation;
pub use snapshot::blob_store::BlobStore;
pub use snapshot::snapshot_store::SnapshotStore;
pub use uid::prefixed_uid::{PrefixedUid, UidParseError};
pub use uid::uid_prefix::UidPrefix;
