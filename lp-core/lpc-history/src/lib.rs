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
//! **History is an append-only event log whose ancestry forms a DAG.** The
//! only non-linear node is the **clobber join**
//! ([`EventKind::Joined`](event::history_event::EventKind)): two parents,
//! content equal to the chosen one. A join resolves a divergence between
//! the local head and one foreign version; the losing side is *set aside*,
//! never destroyed — it stays reachable and classifies as
//! [`SyncRelation::Behind`](lineage::sync_relation::SyncRelation) so peers
//! still carrying it fast-forward. There is **no computed content merge**
//! yet; a future merge is a join whose content is derived rather than
//! picked — the model does not change again. (The pre-cloud linear
//! invariant — "no DAG, no merge, ever" — was retired deliberately for
//! multi-user collaboration; see
//! `docs/adr/2026-08-05-project-history-dag-joins.md`.)
//!
//! **Forks are still new projects.** Forks mint a *new project uid* whose
//! history begins with a
//! [`EventKind::ForkedFrom`](event::history_event::EventKind) origin event
//! pointing at the parent project and version.
//!
//! **The head rule.** Editing the head advances the line; editing anything
//! else forks — lazily, on first save. This crate does not enforce the
//! rule at edit surfaces (that wiring lives in the studio layers); it
//! provides the primitives — [`lineage::project_history::ProjectHistory`]
//! recording saves and joins at the head, and fork constructors for
//! everything else — that make the rule the only expressible behavior.

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
pub use uid::prefixed_uid::{PrefixedUid, UID_BODY_LEN, UidParseError};
pub use uid::uid_prefix::UidPrefix;
