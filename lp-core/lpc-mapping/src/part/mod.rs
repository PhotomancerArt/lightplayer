//! The mapping model: a fixture's **parts** (shapes, groups, repeats) form a
//! tree whose leaves are **segments**, runs of lamps (names.html draft 3).
//!
//! In memory only: no file format reads or writes these yet, and nothing
//! outside this crate calls them (the engine still runs `Map2dDoc` and
//! `PatchDoc`).

mod colors;
mod id_counters;
mod lamp_ref;
mod order;
mod part_id;
mod segment_key;
mod tree_id;

pub use colors::{ColorChannel, Colors};
pub use id_counters::{IdCounters, PART_KIND_WORDS};
pub use lamp_ref::LampRef;
pub use order::{ORDER_MAX_LEN, Order};
pub use part_id::PartId;
pub use segment_key::SegmentKey;
pub use tree_id::TreeId;
