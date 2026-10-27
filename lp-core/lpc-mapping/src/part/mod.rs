//! The mapping model: a fixture's **parts** (shapes, groups, repeats) form a
//! tree whose leaves are **segments**, runs of lamps (names.html draft 3).
//!
//! In memory only: no file format reads or writes these yet, and nothing
//! outside this crate calls them (the engine still runs `Map2dDoc` and
//! `PatchDoc`).

mod id_counters;
mod part_id;
mod segment_key;
mod tree_id;

pub use id_counters::{IdCounters, PART_KIND_WORDS};
pub use part_id::PartId;
pub use segment_key::SegmentKey;
pub use tree_id::TreeId;
