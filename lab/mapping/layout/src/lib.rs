//! Layout for the mapping lab: a semantic graph of nodes in, placements
//! out.
//!
//! The canvas has two layouts (architecture §5), and both are here, pure,
//! so the view layer runs them and statements test them without a page:
//!
//! - a [`LayoutInput`] says what there is to place: each [`LayoutNode`]
//!   with its id, its parent module, its [`LayoutRole`] and its size; the
//!   dataflow between them; and any pins, fixed points everything else
//!   lays out around (D64; empty while pinning is off);
//! - [`canvas_layout`] places them by role: controls in a row along the
//!   top, then columns that follow the flow (sources, fixtures, outputs,
//!   each output level with what feeds it), modules as frames stacked
//!   below, free nodes at the right;
//! - [`stack_layout`] puts them in one column in tree order, a module's
//!   children inset under it;
//! - both give back a [`Placement`]: every node's [`Rect`] in world units.
//!
//! Stability comes first: the same input always lays out the same way, and
//! a small edit moves things little and locally. The spacing is in
//! [`spacing`]. It has no IO and no dependencies, and its own tiny
//! geometry ([`Vec2`], [`Rect`]), so it stands alone.
//!
//! ```
//! use lab_layout::*;
//!
//! let input = LayoutInput::new(vec![
//!     LayoutNode::new("shader", LayoutRole::Source, Vec2::new(60.0, 27.0)),
//!     LayoutNode::new("tail", LayoutRole::Fixture, Vec2::new(80.0, 80.0)),
//!     LayoutNode::new("out", LayoutRole::Output, Vec2::new(60.0, 27.0)),
//! ])
//! .edge("shader", "tail")
//! .edge("tail", "out");
//! let p = canvas_layout(&input);
//! assert!(p.rect("shader").unwrap().max.x < p.rect("tail").unwrap().min.x);
//! assert!(p.rect("tail").unwrap().max.x < p.rect("out").unwrap().min.x);
//! ```

mod canvas_layout;
mod layout_input;
mod layout_node;
mod layout_role;
mod node_tree;
mod placement;
mod rect;
pub mod spacing;
mod stack_layout;
mod vec2;

pub use canvas_layout::canvas_layout;
pub use layout_input::LayoutInput;
pub use layout_node::LayoutNode;
pub use layout_role::LayoutRole;
pub use placement::Placement;
pub use rect::Rect;
pub use stack_layout::stack_layout;
pub use vec2::Vec2;
