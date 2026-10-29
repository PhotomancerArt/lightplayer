//! What each part of the view tree depends on, so a page can build a part
//! again only when it would come out different. Panning moves none of
//! them: the cards, the edges, the tree and the inspector stay as they
//! were, and only the overlay (which is in screen pixels) is built again.
//!
//! Each key lives here, beside the builders it describes, so a change to
//! what a builder reads is a change to its key in the same file's
//! neighbourhood.

use std::collections::BTreeMap;
use std::sync::Arc;

use lpc_mapping::patching::OutputPorts;

use crate::domain::node::NodeId;
use crate::domain::placement::Placement;
use crate::domain::target::Target;
use crate::ux::debug_flags::Presentation;
use crate::ux::editor::Editor;
use crate::ux::patching::Armed;
use crate::ux::port_grouping::PortGrouping;
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
    armed: Option<Armed>,
    grouping: PortGrouping,
    /// A card says its fixture's problems, which the host's port tables
    /// decide as much as what the project holds.
    tables: Tables,
}

/// The port tables a host handed over (`Project::set_port_tables`): new
/// ones re-patch without an edit, so the revision alone would keep a
/// problem the new tables cleared (Yona at G3, 2026-10-28).
type Tables = Option<Arc<BTreeMap<NodeId, Vec<OutputPorts>>>>;

fn same_tables(a: &Tables, b: &Tables) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
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
    /// Debug's Presentation and Cursor ops: what the inspector shows.
    presentation: Presentation,
    cursor_ops: bool,
    /// The inspector and the tree say the problems, and where segments
    /// land: the port tables decide both.
    tables: Tables,
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
            && self.armed == o.armed
            && self.grouping == o.grouping
            && same_tables(&self.tables, &o.tables)
    }
}

impl PartialEq for PanelsKey {
    fn eq(&self, o: &Self) -> bool {
        self.revision == o.revision
            && Arc::ptr_eq(&self.placement, &o.placement)
            && self.selection == o.selection
            && self.move_draw == o.move_draw
            && self.presentation == o.presentation
            && self.cursor_ops == o.cursor_ops
            && same_tables(&self.tables, &o.tables)
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
            armed: self.armed.clone(),
            grouping: self.grouping,
            tables: self.project.port_tables.0.clone(),
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
            presentation: self.debug.presentation,
            cursor_ops: self.debug.cursor_ops,
            tables: self.project.port_tables.0.clone(),
        }
    }
}
