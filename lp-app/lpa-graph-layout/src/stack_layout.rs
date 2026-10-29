//! The stack layout: one column in tree order, as Studio lays its cards
//! out today (architecture §5.6). It scrolls; it never zooms.
//!
//! Each node, then a module's children right after it, inset a step per
//! level. A node is as wide as the column less its inset. A fixture keeps
//! its box's shape at that width; every other node keeps its height. A
//! module is a header row with its children under it, and its box covers
//! them, so the page can draw a rail down its left. Pins don't apply.

use crate::node_tree::NodeTree;
use crate::spacing::{STACK_GAP, STACK_HEADER, STACK_INSET};
use crate::{LayoutInput, LayoutRole, Placement, Rect, Vec2};

/// Lay `input` out as one column `width` wide, in world units.
pub fn stack_layout(input: &LayoutInput, width: f64) -> Placement {
    let tree = NodeTree::new(input);
    let mut stack = Stack {
        input,
        tree: &tree,
        width,
        y: 0.0,
        out: Placement::default(),
    };
    stack.lay_out(None, 0);
    stack.out.with_extent()
}

struct Stack<'a> {
    input: &'a LayoutInput,
    tree: &'a NodeTree,
    width: f64,
    /// Where the next node goes.
    y: f64,
    out: Placement,
}

impl Stack<'_> {
    fn lay_out(&mut self, container: Option<usize>, depth: usize) {
        for &i in self.tree.children_of(container) {
            let node = &self.input.nodes[i];
            let x = depth as f64 * STACK_INSET;
            let w = (self.width - x).max(0.0);
            let top = self.y;
            let bottom = match node.role {
                LayoutRole::Module => {
                    self.y = top + STACK_HEADER + STACK_GAP;
                    self.out
                        .insides
                        .insert(node.id.clone(), Vec2::new(x + STACK_INSET, self.y));
                    self.lay_out(Some(i), depth + 1);
                    if self.tree.children_of(Some(i)).is_empty() {
                        top + STACK_HEADER
                    } else {
                        self.y - STACK_GAP
                    }
                }
                LayoutRole::Fixture if node.size.x > 0.0 => top + w * node.size.y / node.size.x,
                _ => top + node.size.y,
            };
            self.out.rects.insert(
                node.id.clone(),
                Rect::new(Vec2::new(x, top), Vec2::new(x + w, bottom)),
            );
            self.y = bottom + STACK_GAP;
        }
    }
}
