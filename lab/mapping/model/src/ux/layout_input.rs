//! What the view lays out. Core says what there is (D63): each node with
//! its role, its size and its module, the dataflow between them, and the
//! pins while pinning is on (D64). The page runs `lab-layout` on it and
//! hands the result back with [`Editor::set_placement`]; core never runs
//! the layout itself.

use lab_layout::{LayoutInput, LayoutNode, LayoutRole};

use crate::domain::geom::{Rect, Vec2};
use crate::domain::node::{Node, NodeKind};
use crate::domain::placement::Placement;
use crate::ux::card_geometry::node_size;
use crate::ux::editor::Editor;

impl Editor {
    /// Every node as the layout sees it, in tree order, with the dataflow
    /// (render sources into fixtures, fixtures into outputs) and, while
    /// pinning is on, the pins.
    pub fn layout_input(&self) -> LayoutInput {
        let p = &self.project;
        let mut input = LayoutInput::default();
        for n in p.all_nodes() {
            let size = node_size(n);
            input.nodes.push(LayoutNode {
                id: n.id.to_string(),
                parent: p.parent_module(&n.id).map(|m| m.to_string()),
                role: layout_role(n),
                size: lab_layout::Vec2::new(size.x, size.y),
            });
            if self.debug.pinning
                && let Some(at) = n.pinned
            {
                input
                    .pins
                    .push((n.id.to_string(), lab_layout::Vec2::new(at.x, at.y)));
            }
        }
        let mut add = |from: String, to: String| {
            if !input.edges.contains(&(from.clone(), to.clone())) {
                input.edges.push((from, to));
            }
        };
        for b in p.bindings() {
            add(b.from.to_string(), b.to.to_string());
        }
        // A fixture reaching an output only by place rules feeds it too.
        for s in &p.effective_patch().spans {
            add(s.fixture.to_string(), s.output.to_string());
        }
        input
    }

    /// Take the view's layout as a fact: where every node landed. Picks,
    /// areas, handles and framing all read it. The first placement after
    /// opening a project frames everything.
    pub fn set_placement(&mut self, placement: &lab_layout::Placement) {
        let rect = |r: &lab_layout::Rect| {
            Rect::new(Vec2::new(r.min.x, r.min.y), Vec2::new(r.max.x, r.max.y))
        };
        let ids = |id: &str| crate::domain::node::NodeId::new(id);
        let placement = Placement {
            rects: placement
                .rects
                .iter()
                .map(|(id, r)| (ids(id), rect(r)))
                .collect(),
            insides: placement
                .insides
                .iter()
                .map(|(id, v)| (ids(id), Vec2::new(v.x, v.y)))
                .collect(),
        };
        let first = self.project.placement().rects.is_empty();
        self.project.set_placement(placement);
        if first && !self.project.placement().rects.is_empty() {
            self.fit();
        }
    }
}

/// What a node is to the layout. Controls (a clock, a button) have no
/// dataflow in and sit in a row above the pipeline; a playlist makes a
/// picture, like a shader.
pub fn layout_role(n: &Node) -> LayoutRole {
    match &n.kind {
        NodeKind::Module { .. } => LayoutRole::Module,
        NodeKind::Fixture(_) => LayoutRole::Fixture,
        NodeKind::Output(_) => LayoutRole::Output,
        NodeKind::Source(_) => LayoutRole::Source,
        NodeKind::Other(o) => match o.kind.as_str() {
            "Clock" | "PowerButton" | "Button" | "ControlRadio" | "Radio" => LayoutRole::Control,
            "Playlist" => LayoutRole::Source,
            _ => LayoutRole::Other,
        },
    }
}
