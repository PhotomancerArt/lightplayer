//! The mapping model: a fixture's **parts** (shapes, groups, repeats) form a
//! tree whose leaves are **segments**, runs of lamps (names.html draft 3).
//!
//! In memory only: no file format reads or writes these yet, and nothing
//! outside this crate calls them (the engine still runs `Map2dDoc` and
//! `PatchDoc`).

mod circle;
mod colors;
mod id_counters;
mod lamp_ref;
mod mapping;
mod order;
mod part;
mod part_id;
mod part_kind;
mod path;
mod path_command;
mod path_split;
mod render;
mod render_error;
mod repeat;
mod segment;
mod segment_key;
mod shape;
mod subpath_runs;
mod tree;
mod tree_id;
mod tree_node;

pub use circle::{Circle, Ring};
pub use colors::{ColorChannel, Colors};
pub use id_counters::{IdCounters, PART_KIND_WORDS};
pub use lamp_ref::LampRef;
pub use mapping::Mapping;
pub use order::{ORDER_MAX_LEN, Order};
pub use part::Part;
pub use part_id::PartId;
pub use part_kind::PartKind;
pub use path::Path;
pub use path_command::PathCommand;
pub use path_split::PathSplit;
pub use render::render;
pub use render_error::RenderError;
pub use repeat::Repeat;
pub use segment::Segment;
pub use segment_key::SegmentKey;
pub use shape::Shape;
pub use tree::Tree;
pub use tree_id::TreeId;
pub use tree_node::{NodeKind, TreeNode};
