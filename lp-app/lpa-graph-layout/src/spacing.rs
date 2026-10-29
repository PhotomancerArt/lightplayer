//! The layout's spacing, in world units. Exported so callers and
//! statements can name them.

use crate::Vec2;

/// Between nodes stacked in a column, and between controls in their row.
pub const NODE_GAP: f64 = 24.0;
/// Between columns: room for the edges running between them.
pub const COLUMN_GAP: f64 = 30.0;
/// Inside a module's frame, around its children…
pub const MODULE_PAD: f64 = 10.0;
/// …and above them, so their title tabs sit inside the frame.
pub const MODULE_PAD_TOP: f64 = 18.0;
/// An empty module's inside.
pub const EMPTY_MODULE: Vec2 = Vec2::new(40.0, 20.0);

/// The stack layout: each level of nesting is inset this far…
pub const STACK_INSET: f64 = 12.0;
/// …with this gap between one node and the next…
pub const STACK_GAP: f64 = 8.0;
/// …and a module is a header row this tall, its children under it.
pub const STACK_HEADER: f64 = 14.0;
