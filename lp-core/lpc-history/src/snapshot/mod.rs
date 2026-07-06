//! Content-addressed snapshot storage over a history-root filesystem.

pub mod blob_store;
pub mod snapshot_store;

pub use blob_store::BlobStore;
pub use snapshot_store::SnapshotStore;
