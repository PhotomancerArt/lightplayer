//! What each part of the view tree depends on, so a page can build a part
//! again only when it would come out different. Panning moves none of
//! them: the cards, the edges, the tree and the inspector stay as they
//! were, and only the overlay (which is in screen pixels) is built again.
//!
//! Each key lives here, beside the builders it describes, so a change to
//! what a builder reads is a change to its key in the same file's
//! neighbourhood.

use std::sync::Arc;

use crate::domain::node::NodeId;
use crate::domain::placement::Placement;
use crate::domain::target::Target;
use crate::ux::editor::Editor;
use crate::ux::ui::LayoutKind;

/// What the nodes and their faces depend on (`Editor::view_nodes`).
#[derive(Debug, Clone)]
pub struct NodesKey {
    revision: u64,
    placement: Arc<Placement>,
    selection: Vec<Target>,
    hover: Option<Target>,
    scale: f64,
    pinning: bool,
    layout: LayoutKind,
}

/// What the edges depend on (`Editor::view_edges`).
#[derive(Debug, Clone, PartialEq)]
pub struct EdgesKey {
    revision: u64,
    selection: Vec<Target>,
    hovered: Option<(NodeId, NodeId)>,
    layout: LayoutKind,
}

/// What the tree pane and the inspector depend on.
#[derive(Debug, Clone)]
pub struct PanelsKey {
    revision: u64,
    /// The inspector says where a lamp is, in world units.
    placement: Arc<Placement>,
    selection: Vec<Target>,
    move_draw: bool,
}

impl PartialEq for NodesKey {
    fn eq(&self, o: &Self) -> bool {
        self.revision == o.revision
            && Arc::ptr_eq(&self.placement, &o.placement)
            && self.selection == o.selection
            && self.hover == o.hover
            && self.scale == o.scale
            && self.pinning == o.pinning
            && self.layout == o.layout
    }
}

impl PartialEq for PanelsKey {
    fn eq(&self, o: &Self) -> bool {
        self.revision == o.revision
            && Arc::ptr_eq(&self.placement, &o.placement)
            && self.selection == o.selection
            && self.move_draw == o.move_draw
    }
}

impl Editor {
    pub fn view_nodes_key(&self) -> NodesKey {
        NodesKey {
            revision: self.project.revision(),
            placement: self.project.placement.0.clone(),
            selection: self.selection.clone(),
            hover: self.hover_target(),
            scale: self.camera.scale,
            pinning: self.debug.pinning,
            layout: self.layout(),
        }
    }

    pub fn view_edges_key(&self) -> EdgesKey {
        EdgesKey {
            revision: self.project.revision(),
            selection: self.selection.clone(),
            hovered: self.edge_hover_now(),
            layout: self.layout(),
        }
    }

    pub fn view_panels_key(&self) -> PanelsKey {
        PanelsKey {
            revision: self.project.revision(),
            placement: self.project.placement.0.clone(),
            selection: self.selection.clone(),
            move_draw: self.move_draw,
        }
    }
}
