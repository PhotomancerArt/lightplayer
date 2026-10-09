//! **lp-tree-store**: a prototype content-addressed copy-on-write tree store
//! for NOR flash — the storage testbed's T1 candidate. `no_std` + `alloc`,
//! sans-IO over a small [`Flash`] trait. See the README for the design, the
//! on-flash format, the invariants (I1 root, I2 at least one copy, I3
//! derived liveness) and the dials.

#![no_std]

extern crate alloc;

mod blob_codec;
mod dir_node;
mod file_tree;
mod flash;
mod gc_copy;
mod gc_mark;
mod gc_victim;
mod multi_node;
mod node_layout;
mod object_id;
mod ram_index;
mod record_header;
mod record_kind;
mod record_log;
mod record_plan;
mod record_scan;
mod root_record;
mod root_select;
mod sector_header;
mod sector_table;
mod small_sort;
mod space_estimate;
mod store_config;
#[cfg(feature = "encode")]
mod store_dictionary;
mod store_error;
mod store_stats;
mod tree_store;

#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod test_support;

pub use flash::Flash;
pub use object_id::ObjectId;
pub use store_config::{Codec, GcPolicy, StoreConfig};
pub use store_error::StoreError;
pub use store_stats::TreeStoreStats;
pub use tree_store::TreeStore;
