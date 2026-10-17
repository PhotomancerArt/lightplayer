//! The view tree, part by part. Each part is a memo built from the editor
//! only when its key moves (`lab_mapping_model::view_keys`): panning
//! builds the overlay again and nothing else, so the cards, the edges and
//! the panels keep what they drew. Together the parts are
//! `Editor::view_tree`.

use dioxus::prelude::*;
use lab_mapping_model::{
    Editor, HintBar, Notice, UiCanvasEdge, UiCanvasNode, UiCursor, UiCursorNote, UiInspector,
    UiMenu, UiOverlay, UiToolbar, UiTree,
};

#[derive(Clone, Copy, PartialEq)]
pub struct View {
    pub nodes: Memo<Vec<UiCanvasNode>>,
    pub edges: Memo<Vec<UiCanvasEdge>>,
    pub overlay: Memo<UiOverlay>,
    pub inspector: Memo<UiInspector>,
    pub tree: Memo<UiTree>,
    pub hints: Memo<(HintBar, Option<Notice>)>,
    pub toolbar: Memo<UiToolbar>,
    pub menu: Memo<Option<UiMenu>>,
    pub cursor: Memo<(UiCursor, Option<UiCursorNote>)>,
}

pub fn use_view(ed: Signal<Editor>) -> View {
    let nodes_key = use_memo(move || ed.read().view_nodes_key());
    let edges_key = use_memo(move || ed.read().view_edges_key());
    let panels_key = use_memo(move || ed.read().view_panels_key());
    View {
        nodes: use_memo(move || {
            nodes_key.read();
            ed.peek().view_nodes()
        }),
        edges: use_memo(move || {
            edges_key.read();
            ed.peek().view_edges()
        }),
        inspector: use_memo(move || {
            panels_key.read();
            ed.peek().view_inspector()
        }),
        tree: use_memo(move || {
            panels_key.read();
            ed.peek().view_tree_pane()
        }),
        overlay: use_memo(move || ed.read().view_overlay()),
        hints: use_memo(move || {
            let e = ed.read();
            (e.hint_bar(), e.notice.clone())
        }),
        toolbar: use_memo(move || ed.read().view_toolbar()),
        menu: use_memo(move || ed.read().view_menu()),
        cursor: use_memo(move || {
            let e = ed.read();
            let note = e.view_cursor_note();
            (e.view_cursor(), note)
        }),
    }
}
