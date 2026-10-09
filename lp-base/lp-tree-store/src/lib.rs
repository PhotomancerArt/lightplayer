//! **lp-tree-store**: a content-addressed copy-on-write tree store for NOR
//! flash. `no_std` + `alloc`, sans-IO over a small [`Flash`] trait and an
//! injected [`ObjectHasher`]. RAM-bounded: a sorted index of 12-byte
//! entries and 12 bytes per sector are all it keeps between operations (no
//! path in RAM: lookups walk the directories on flash). See `FORMAT.md` for every on-flash byte and the
//! README for the design, the invariants (I1 root, I2 at least one copy, I3
//! derived liveness), the dials and the measurements.

#![no_std]

extern crate alloc;

/// A statement that runs only with the `stats` feature: the counters and the
/// RAM-peak accounting `TreeStore::stats` reports. Off in firmware (they are
/// ~0.7 KB of RV32 code); the crate's tests and `lp-store-bench` turn it on.
macro_rules! stat {
    ($($s:tt)*) => {
        #[cfg(feature = "stats")]
        {
            $($s)*
        }
    };
}

mod blob_codec;
mod dir_node;
mod dir_rebuild;
mod flash;
mod gc_copy;
mod gc_mark;
mod gc_victim;
mod heap_sort;
#[cfg(feature = "host-deflate")]
mod host_deflate;
#[cfg(feature = "inspect")]
mod image_inspect;
#[cfg(feature = "lpfs")]
mod lp_fs_tree;
mod mount_walk;
mod multi_node;
mod node_read;
mod node_write;
mod object_hasher;
mod object_id;
mod ram_index;
mod record_header;
mod record_kind;
mod record_log;
mod record_scan;
mod root_record;
mod sector_header;
mod sector_table;
mod space_estimate;
mod store_config;
mod store_error;
mod store_mount;
mod store_space;
#[cfg(feature = "stats")]
mod store_stats;
mod tree_delta;
mod tree_store;
mod tree_walk;
mod vec_growth;

#[cfg(test)]
mod cut_sweep_tests;
#[cfg(test)]
mod format_extension_tests;
#[cfg(all(test, feature = "lpfs"))]
mod lp_fs_tree_tests;
#[cfg(test)]
mod ram_budget_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod txn_tests;

pub use blob_codec::MAX_LOGICAL_CHUNK;
pub use flash::Flash;
#[cfg(feature = "host-deflate")]
pub use host_deflate::{HostChunk, host_deflate_chunks};
#[cfg(feature = "inspect")]
pub use image_inspect::*;
#[cfg(feature = "lpfs")]
pub use lp_fs_tree::LpFsTree;
pub use object_hasher::ObjectHasher;
#[cfg(feature = "soft-sha")]
pub use object_hasher::SoftSha256;
pub use object_id::ObjectId;
pub use sector_header::FORMAT_VERSION;
pub use store_config::{GcPolicy, StoreConfig};
pub use store_error::StoreError;
#[cfg(feature = "stats")]
pub use store_stats::TreeStoreStats;
pub use tree_store::{MAX_DEPTH, TreeStore, is_hot, valid_path};
