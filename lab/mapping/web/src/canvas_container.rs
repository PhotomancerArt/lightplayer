//! The canvas container (architecture §8): it clips, and it takes every
//! pointer, wheel and key event and forwards it to the editor in screen
//! pixels (Q1: core picks, so drill-down, ⌥-click and the "what's here"
//! list stay core's). Inside it, bottom to top:
//!
//! - the background dots, on a screen-space canvas;
//! - the world layer, whose transform is the camera, holding every node as
//!   a flat, keyed DOM card (`world_layer.rs`, `node_card.rs`);
//! - the overlay, screen space, never scaled: frame titles, edges,
//!   outlines, handles, the selection box, a drawing in progress;
//! - the right-click list and the note beside the cursor, as DOM.
//!
//! Layout runs here, in the view (D63): whenever what there is to lay out
//! changes, the page lays it out with `lab-layout`, places the cards from
//! it, and hands the editor where everything landed. The page makes no
//! detail decisions of its own: what's dimmed, lit or shown is the view
//! tree's (`Editor::view_tree`).

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lab_mapping_model::{Editor, NodeId, Rect, UiCursor, Vec2};

use crate::dots_layer::DotsLayer;
use crate::edge_layer;
use crate::input;
use crate::overlay::{CursorNote, Menu, Overlay};
use crate::view::View;
use crate::world_layer::WorldLayer;

/// How often the board's clock ticks while its pixels move, in ms.
const TICK_MS: u32 = 33;
/// How long a relayout's cards take to ease to their new places (the CSS
/// transition on `.node`), plus a frame: edges fade out for it.
const SETTLE_MS: u32 = 200;

#[component]
pub fn Canvas(ed: Signal<Editor>, view: View) -> Element {
    let mut ed = ed;
    // Moves and releases come from the whole window, so a drag keeps going
    // when the pointer leaves the canvas (or the window) and finishes right
    // when it comes back.
    use_hook(move || input::follow_window(ed));

    // Layout, in the view: what there is (core's), laid out (lab-layout's),
    // handed back to the editor as a fact. The input is compared, so this
    // runs only when it changes, and handing the placement back changes no
    // input, so it can't loop.
    let layout_input = use_memo(move || ed.read().layout_input());
    let placement = use_memo(move || lab_layout::canvas_layout(&layout_input.read()));
    let rects = use_memo(move || to_rects(&placement.read()));
    let mut settling = use_signal(|| false);
    let mut settles = use_signal(|| 0_u64);
    use_effect(move || {
        let p = placement.read();
        ed.write().set_placement(&p);
        // Edges fade while the cards ease to their new places.
        let n = *settles.peek() + 1;
        settles.set(n);
        settling.set(true);
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(SETTLE_MS).await;
            if *settles.peek() == n {
                settling.set(false);
            }
        });
    });

    // The board's clock. It only advances while the pixels move, so a
    // still board costs no redraws.
    let mut now = use_signal(|| 0.0_f64);
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(TICK_MS).await;
            if ed.peek().board_animates() {
                now.set(seconds());
            }
        }
    });
    // What the board shows depends on the project, the selection and a
    // drawing in progress — not on the pointer or the camera — so it is
    // worked out again only when one of those, or the clock, moves.
    let board_key = use_memo(move || {
        let e = ed.read();
        let draft = e.drawing().then(|| (e.pointer, e.path_draft.clone(), e.camera));
        (e.project.revision(), e.selection.clone(), draft)
    });
    let frame = use_memo(move || {
        board_key.read();
        let t = now();
        ed.peek().board_frame(t)
    });

    let (camera, viewport, revision) = {
        let e = ed.read();
        (e.camera, e.viewport, e.project.revision())
    };
    let edges = edge_layer::route(&view.edges.read(), &ed.read(), &rects.read(), &camera);
    let (cursor, note) = view.cursor.read().clone();
    let cursor = cursor_css(cursor);
    let nodes = view.nodes.read().clone();
    let overlay = view.overlay.read().clone();
    let menu = view.menu.read().clone();

    rsx! {
        div {
            class: "canvas",
            style: "cursor: {cursor}",
            onresize: move |evt| {
                if let Ok(size) = evt.get_content_box_size() {
                    ed.write().resize(Vec2::new(size.width, size.height));
                }
            },
            oncontextmenu: move |evt| evt.prevent_default(),
            onmousedown: move |evt| {
                if let (Some(b), Some(at)) = (input::button(&evt), input::client_point(&evt)) {
                    evt.prevent_default();
                    ed.write().pointer_down(at, b, input::mods(evt.modifiers()));
                }
            },
            ondoubleclick: move |evt| {
                if let Some(at) = input::client_point(&evt)
                    && ed.write().double_click(at, input::mods(evt.modifiers()))
                {
                    evt.prevent_default();
                }
            },
            onwheel: move |evt| {
                evt.prevent_default();
                let d = input::wheel_delta(&evt);
                if let Some(at) = input::wheel_client_point(&evt) {
                    ed.write().wheel(at, d, input::mods(evt.modifiers()));
                }
            },
            DotsLayer { camera, viewport }
            WorldLayer { nodes: nodes.clone(), rects, camera, viewport, revision, frame, ed }
            Overlay { nodes, rects, overlay, edges, settling: settling(), camera, viewport }
            if let Some(menu) = menu {
                Menu { menu, viewport, ed }
            }
            if let Some(note) = note {
                CursorNote { note }
            }
        }
    }
}

/// The page's placement as the model's rects, by node.
fn to_rects(p: &lab_layout::Placement) -> BTreeMap<NodeId, Rect> {
    p.rects
        .iter()
        .map(|(id, r)| {
            (
                NodeId::new(id.clone()),
                Rect::new(Vec2::new(r.min.x, r.min.y), Vec2::new(r.max.x, r.max.y)),
            )
        })
        .collect()
}

fn cursor_css(c: UiCursor) -> &'static str {
    match c {
        UiCursor::Default => "default",
        UiCursor::Grab => "grab",
        UiCursor::Grabbing => "grabbing",
        UiCursor::Move => "move",
        UiCursor::NotAllowed => "not-allowed",
        UiCursor::Crosshair => "crosshair",
        UiCursor::ResizeX => "ew-resize",
        UiCursor::ResizeY => "ns-resize",
        UiCursor::ResizeXY => "nwse-resize",
    }
}

/// Seconds since the page started: the board's clock.
fn seconds() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now() / 1000.0)
}
