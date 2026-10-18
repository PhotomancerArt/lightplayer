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
//!
//! Two layouts (architecture §5). On the **canvas** the world layer's
//! transform is the camera: pan and zoom. In the **stack** the nodes are one
//! column in tree order at a fixed zoom, in a container that scrolls
//! natively; the editor's camera follows the scroll so picks still land.
//! Both put the same keyed cards in the same world layer, so switching
//! eases every card to its new place while the world's own transform eases
//! from the camera to the stack's, and the edges fade for the move.
//!
//! The colours are the host's: it hands in the board's frame. The lab
//! simulates one (`board_clock.rs`); Studio builds one from the board's
//! own output frames.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lab_mapping_model::{BoardFrame, Camera, GraphInput, LayoutKind, NodeId, Rect, UiCursor, Vec2};

use crate::dots_layer::DotsLayer;
use crate::edge_layer;
use crate::graph_editor::GraphEditor;
use crate::input;
use crate::overlay::{CursorNote, Menu, Overlay};
use crate::view::View;
use crate::world_layer::WorldLayer;

/// How long a relayout's cards take to ease to their new places (the CSS
/// transition on `.node`), plus a frame: edges fade out for it.
const SETTLE_MS: u32 = 200;
/// How long switching layouts takes (`.world.switching` in `lab.css`).
const SWITCH_MS: u32 = 320;

/// The stack's fixed zoom: card text (3.9 world units) at 13 px.
pub const STACK_ZOOM: f64 = 13.0 / 3.9;
/// The stack's gutter at the column's left, where its edges run, and the
/// room at its right and top, in screen pixels.
pub const GUTTER_PX: f64 = 28.0;
const STACK_RIGHT_PX: f64 = 12.0;
pub const STACK_TOP_PX: f64 = 16.0;

/// The stack's camera when it has scrolled `scroll_top` pixels: the column
/// starts after the gutter, at the stack's zoom.
fn stack_camera(scroll_top: f64) -> Camera {
    Camera {
        offset: Vec2::new(GUTTER_PX, STACK_TOP_PX - scroll_top),
        scale: STACK_ZOOM,
    }
}

#[component]
pub fn Canvas(ed: GraphEditor, view: View, frame: ReadSignal<BoardFrame>) -> Element {
    // Moves and releases come from the whole window, so a drag keeps going
    // when the pointer leaves the canvas (or the window) and finishes right
    // when it comes back. The listeners go when this canvas does.
    use_hook({
        let ed = ed.clone();
        move || std::rc::Rc::new(input::follow_window(ed))
    });

    // Layout, in the view: what there is (core's), laid out (lab-layout's),
    // handed back to the editor as a fact. The input is compared, so this
    // runs only when it changes, and handing the placement back changes no
    // input, so it can't loop.
    let layout_input = use_memo({
        let ed = ed.clone();
        move || ed.read().layout_input()
    });
    let layout = use_memo({
        let ed = ed.clone();
        move || ed.read().layout()
    });
    // The stack's column, in world units, from the canvas's width.
    let stack_width = use_memo({
        let ed = ed.clone();
        move || {
            let w = ed.read().viewport.x;
            ((w - GUTTER_PX - STACK_RIGHT_PX) / STACK_ZOOM).max(20.0)
        }
    });
    let placement = use_memo(move || match layout() {
        LayoutKind::Canvas => lab_layout::canvas_layout(&layout_input.read()),
        LayoutKind::Stack => lab_layout::stack_layout(&layout_input.read(), stack_width()),
    });
    let rects = use_memo(move || to_rects(&placement.read()));
    let mut scroll_top = use_signal(|| 0.0_f64);
    let mut switching = use_signal(|| false);
    let mut placed_for = use_signal(|| None::<LayoutKind>);
    let mut settling = use_signal(|| false);
    let mut settles = use_signal(|| 0_u64);
    use_effect({
        let ed = ed.clone();
        move || {
            let p = placement.read().clone();
            let now = layout();
            ed.input(GraphInput::Placement(p));
            // A switch: the canvas frames everything again; the stack starts at
            // its top. The world's transform eases between them.
            let was = *placed_for.peek();
            if was.is_some_and(|w| w != now) {
                match now {
                    LayoutKind::Canvas => {
                        ed.input(GraphInput::Fit);
                    }
                    LayoutKind::Stack => {
                        set_scroll_top(0.0);
                        scroll_top.set(0.0);
                    }
                }
                switching.set(true);
                spawn(async move {
                    gloo_timers::future::TimeoutFuture::new(SWITCH_MS).await;
                    switching.set(false);
                });
            }
            placed_for.set(Some(now));
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
        }
    });

    // In the stack the editor's camera follows the scroll.
    use_effect({
        let ed = ed.clone();
        move || {
            let want = stack_camera(scroll_top());
            let e = ed.read();
            if e.layout() == LayoutKind::Stack && e.camera != want {
                drop(e);
                ed.input(GraphInput::Camera(want));
            }
        }
    });
    // In the stack, bring the card the selection is in into view.
    let focus = use_memo({
        let ed = ed.clone();
        move || ed.read().focus_node()
    });
    use_effect({
        let ed = ed.clone();
        move || {
            // Again when the column changes width: the cards move.
            let _ = stack_width();
            let Some(id) = focus() else { return };
            if layout() != LayoutKind::Stack {
                return;
            }
            let Some(r) = rects.peek().get(&id).copied() else {
                return;
            };
            let cam = stack_camera(*scroll_top.peek());
            let (top, bottom) = (cam.to_screen(r.min).y, cam.to_screen(r.max).y);
            let h = ed.peek().viewport.y;
            // In the upper part of the screen, clear of a sheet below.
            if top < 0.0 || bottom > h * 0.55 {
                let y = (*scroll_top.peek() + top - 24.0).max(0.0);
                set_scroll_top(y);
                scroll_top.set(y);
            }
        }
    });

    let stack = layout() == LayoutKind::Stack;
    let (camera, viewport, revision) = {
        let e = ed.read();
        let camera = if stack {
            stack_camera(scroll_top())
        } else {
            e.camera
        };
        (camera, e.viewport, e.project.revision())
    };
    let extent_px = if stack {
        placement.read().extent.max.y * STACK_ZOOM + 2.0 * STACK_TOP_PX
    } else {
        0.0
    };
    let edges = edge_layer::route(&view.edges.read(), &ed.read(), &rects.read(), &camera);
    let (cursor, note) = view.cursor.read().clone();
    let cursor = cursor_css(cursor);
    let nodes = view.nodes.read().clone();
    let overlay = view.overlay.read().clone();
    let menu = view.menu.read().clone();

    let (on_resize, on_down, on_double, on_wheel) =
        (ed.clone(), ed.clone(), ed.clone(), ed.clone());
    rsx! {
        div {
            class: "canvas",
            style: "cursor: {cursor}",
            onresize: move |evt| {
                if let Ok(size) = evt.get_content_box_size() {
                    on_resize.input(GraphInput::Resize(Vec2::new(size.width, size.height)));
                }
            },
            oncontextmenu: move |evt| evt.prevent_default(),
            onmousedown: move |evt| {
                if let (Some(button), Some(at)) = (input::button(&evt), input::client_point(&evt)) {
                    evt.prevent_default();
                    let mods = input::mods(evt.modifiers());
                    on_down.input(GraphInput::PointerDown { at, button, mods });
                }
            },
            ondoubleclick: move |evt| {
                if let Some(at) = input::client_point(&evt)
                    && on_double.input(GraphInput::DoubleClick { at, mods: input::mods(evt.modifiers()) })
                {
                    evt.prevent_default();
                }
            },
            onwheel: move |evt| {
                // The stack scrolls natively; only the canvas pans and zooms.
                if stack {
                    return;
                }
                evt.prevent_default();
                let delta = input::wheel_delta(&evt);
                if let Some(at) = input::wheel_client_point(&evt) {
                    on_wheel.input(GraphInput::Wheel { at, delta, mods: input::mods(evt.modifiers()) });
                }
            },
            if !stack {
                DotsLayer { camera, viewport }
            }
            div {
                class: if stack { "scroller stack" } else { "scroller" },
                onscroll: move |_| scroll_top.set(read_scroll_top()),
                div { class: "scroll-extent", style: "height: {extent_px}px" }
                WorldLayer {
                    nodes: nodes.clone(),
                    rects,
                    camera,
                    viewport,
                    stack,
                    switching: switching(),
                    revision,
                    frame,
                    ed: ed.clone(),
                }
            }
            Overlay { nodes, rects, overlay, edges, settling: settling(), camera, viewport }
            if let Some(menu) = menu {
                Menu { menu, viewport, ed: ed.clone() }
            }
            if let Some(note) = note {
                CursorNote { note }
            }
        }
    }
}

/// The scroller's scroll, in pixels.
fn read_scroll_top() -> f64 {
    scroller().map_or(0.0, |el| el.scroll_top() as f64)
}

fn set_scroll_top(y: f64) {
    if let Some(el) = scroller() {
        el.set_scroll_top(y.round() as i32);
    }
}

fn scroller() -> Option<web_sys::Element> {
    web_sys::window()?
        .document()?
        .query_selector(".graph .scroller")
        .ok()?
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
