//! The view tree: everything the page draws, emitted by core with no
//! positions (architecture §3). The page lays the nodes out, places the
//! cards, and draws what it is given; every decision a statement could
//! test — what's dimmed, what's lit, what an edge says, how much of a card
//! shows — is made here.
//!
//! Dense data travels beside the tree, read by id: a fixture's lamps
//! (`Project::objects_in`) and the board's colours (`Editor::board_frame`).

use crate::domain::node::{Node, NodeKind};
use crate::domain::target::Target;
use crate::ux::area::title_width;
use crate::ux::card_geometry::node_size;
use crate::ux::editor::{Editor, Gesture, Tool};
use crate::ux::handle::{HandleKind, SizeEdge};
use crate::ux::layout_input::layout_role;
use crate::ux::ui::{
    DetailLevel, Emphasis, FrameKind, UiCanvasEdge, UiCanvasNode, UiCanvasView, UiCursor,
    UiCursorNote, UiMenu, UiMenuAction, UiMenuItem, UiNodeHeader, UiToolbar,
};

/// A card at least this wide on screen shows its words: its text is then
/// at least 4.5 px, which is as small as it reads (60 units × 1.154).
pub const FULL_DETAIL_PX: f64 = 69.2;
/// A card narrower than this on screen is just its box.
pub const FACE_DETAIL_PX: f64 = 12.0;

impl Editor {
    /// Everything the page draws, for the editor as it is now.
    pub fn view_tree(&self) -> UiCanvasView {
        UiCanvasView {
            layout: self.layout,
            nodes: self.view_nodes(),
            edges: self.view_edges(),
            overlay: self.view_overlay(),
            inspector: self.view_inspector(),
            tree: self.view_tree_pane(),
            hints: self.hint_bar(),
            notice: self.notice.clone(),
            toolbar: self.view_toolbar(),
            menu: self.view_menu(),
            cursor_note: self.view_cursor_note(),
            cursor: self.view_cursor(),
        }
    }

    /// Every node, in tree order, with its header, face, detail and
    /// emphasis.
    pub fn view_nodes(&self) -> Vec<UiCanvasNode> {
        let p = &self.project;
        let cx = self.face_context();
        let (context, open) = (self.context(), self.open_fixtures());
        p.all_nodes()
            .into_iter()
            .map(|n| {
                let t = Target::Node(n.id.clone());
                let outside = context.as_ref().is_some_and(|c| {
                    !p.is_ancestor_or_self(c, &t) && !p.is_ancestor_or_self(&t, c)
                });
                // An output fed by a fixture you are working in stays lit.
                let fed_by_open = n.output().is_some()
                    && open.iter().any(|f| p.outputs_of(f).contains(&n.id));
                let size = node_size(n);
                UiCanvasNode {
                    id: n.id.clone(),
                    parent: p.parent_module(&n.id).cloned(),
                    role: layout_role(n),
                    size,
                    frame: frame_kind(n),
                    header: UiNodeHeader {
                        name: n.name.clone(),
                        kind: n.kind_word().to_string(),
                        pinned: self.debug.pinning && n.pinned.is_some(),
                        title_px: title_width(&n.name, n.pinned.is_some()),
                    },
                    face: self.view_face(n, &cx),
                    detail: detail_level(size.x * self.camera.scale),
                    emphasis: if outside && !fed_by_open {
                        Emphasis::Dimmed
                    } else {
                        Emphasis::Full
                    },
                    selected: self.selection.contains(&t),
                }
            })
            .collect()
    }

    /// Every edge, and what it says. On the canvas every edge shows; those
    /// touching the selection are lit, the rest step back while anything is
    /// selected.
    pub fn view_edges(&self) -> Vec<UiCanvasEdge> {
        let node = |t: &Target| t.node().cloned();
        self.edges()
            .into_iter()
            .filter_map(|x| {
                Some(UiCanvasEdge {
                    from: node(&x.from)?,
                    to: node(&x.to)?,
                    label: (x.labelled && !x.label.is_empty()).then(|| x.label.clone()),
                    channel: x.channel,
                    carries: x.carries,
                    lands_on: x.lands_on,
                    shown: true,
                    emphasis: if x.selected {
                        Emphasis::Lit
                    } else if !self.selection.is_empty() {
                        Emphasis::Quiet
                    } else {
                        Emphasis::Full
                    },
                    hovered: x.hovered,
                })
            })
            .collect()
    }

    pub fn view_toolbar(&self) -> UiToolbar {
        UiToolbar {
            tool: self.tool,
            move_draw: self.move_draw,
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            layout: self.layout,
            pinning: self.debug.pinning,
        }
    }

    pub fn view_menu(&self) -> Option<UiMenu> {
        let m = self.menu.as_ref()?;
        Some(UiMenu {
            at: m.at,
            actions: m
                .actions
                .iter()
                .map(|a| UiMenuAction {
                    label: a.label.clone(),
                    keys: a.keys.clone(),
                })
                .collect(),
            items: m
                .items
                .iter()
                .enumerate()
                .map(|(i, it)| UiMenuItem {
                    depth: it.depth,
                    label: it.label.clone(),
                    hovered: m.hover == Some(i),
                })
                .collect(),
        })
    }

    /// The note beside the cursor, where the cursor is.
    pub fn view_cursor_note(&self) -> Option<UiCursorNote> {
        self.cursor_note()
            .zip(self.pointer)
            .map(|(text, at)| UiCursorNote { text, at })
    }

    /// What the pointer looks like: what it is doing, or what pressing
    /// would do.
    pub fn view_cursor(&self) -> UiCursor {
        let handle = self
            .dragged_handle()
            .cloned()
            .or_else(|| self.hover_handle().map(|h| h.kind));
        match (&self.gesture, self.space_held, self.tool) {
            (Gesture::Panning { .. }, _, _) => UiCursor::Grabbing,
            (_, true, _) => UiCursor::Grab,
            _ if handle.is_some() => match handle {
                Some(HandleKind::PathPoint { .. }) => UiCursor::Move,
                Some(HandleKind::RingRadius { .. }) => UiCursor::ResizeX,
                Some(HandleKind::FixtureSize { edge, .. }) => match edge {
                    SizeEdge::Corner => UiCursor::ResizeXY,
                    SizeEdge::Right => UiCursor::ResizeX,
                    SizeEdge::Bottom => UiCursor::ResizeY,
                },
                None => UiCursor::Default,
            },
            (Gesture::Moving { owners, nodes, .. }, _, _)
                if owners.is_empty() && nodes.is_empty() =>
            {
                UiCursor::NotAllowed
            }
            (Gesture::Moving { .. }, _, _) => UiCursor::Move,
            (_, _, Tool::Path | Tool::Circle) => UiCursor::Crosshair,
            _ => UiCursor::Default,
        }
    }
}

/// How much of a card fits at `px` wide on screen.
pub fn detail_level(px: f64) -> DetailLevel {
    if px >= FULL_DETAIL_PX {
        DetailLevel::Full
    } else if px >= FACE_DETAIL_PX {
        DetailLevel::Face
    } else {
        DetailLevel::Dot
    }
}

fn frame_kind(n: &Node) -> FrameKind {
    match n.kind {
        NodeKind::Module { .. } => FrameKind::Module,
        NodeKind::Fixture(_) => FrameKind::Fixture,
        _ => FrameKind::Card,
    }
}
