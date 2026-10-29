//! The mapping model: a fixture's **parts** (shapes, groups, repeats) form a
//! tree whose leaves are **segments**, runs of lamps (names.html draft 3).
//!
//! Saved as map2d format 6 ([`crate::map2d6`]). Nothing outside this crate
//! calls it yet: the engine still runs `Map2dDoc` and `PatchDoc`.

mod circle;
mod circle_split;
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
mod points;
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
pub use circle_split::CircleSplit;
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
pub use points::Points;
pub use render::render;
pub use render_error::RenderError;
pub use repeat::Repeat;
pub use segment::Segment;
pub use segment_key::SegmentKey;
pub use shape::Shape;
pub use tree::Tree;
pub use tree_id::TreeId;
pub use tree_node::{NodeKind, TreeNode};
