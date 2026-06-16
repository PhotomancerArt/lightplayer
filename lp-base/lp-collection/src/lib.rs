//! Embedded/low-memory friendly collections.
//!
//! Two families:
//!
//! - **Chunked** structures ([`ChunkedVec`], [`ChunkedHashMap`]) allocate in
//!   small chunks to reduce OOM risk from heap fragmentation on constrained
//!   heaps (vendored from `lp-common` at rev `c27c24b`).
//! - **Sorted-vec** structures ([`VecMap`], [`VecSet`]) store entries in one
//!   sorted `Vec` and expose the `BTreeMap`/`BTreeSet` API subset this
//!   workspace uses. Lookups are binary search; insert/remove shift the tail.
//!   For the small maps that dominate project/runtime state (tens to low
//!   hundreds of entries) this matches or beats B-trees on speed and RAM, and
//!   generates far less code per key/value instantiation — the ESP32 flash
//!   budget pays ~2-5 KB of B-tree node machinery for every distinct `(K, V)`
//!   pair. Iteration order and key uniqueness match `BTreeMap`. Not suited to
//!   maps that grow beyond a few thousand entries under steady random insert.

#![no_std]

extern crate alloc;

pub mod chunked_hashmap;
pub mod chunked_vec;
mod entry;
mod map;
mod set;

pub use chunked_hashmap::{ChunkedHashMap, ChunkedHashSet, Entry as ChunkedEntry};
pub use chunked_vec::ChunkedVec;
pub use entry::Entry;
pub use map::VecMap;
pub use set::VecSet;
