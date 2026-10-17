//! The overlay: everything that stays one size on screen, in screen
//! pixels, over the world layer. Frames' title tabs (a frame is picked by
//! its tab, `area.rs`), the edges, outlines (each thing's area — what's
//! outlined is what a click hits), handles, the selection box, a drawing in
//! progress, ringed lamps; and as DOM, the right-click list and the note
//! beside the cursor.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lab_arrows::Arrow;
use lab_mapping_model::ux::area::FRAME_TITLE_PX;
use lab_mapping_model::ux::handle::HANDLE_PX;
use lab_mapping_model::{
    Camera, Editor, Emphasis, FrameKind, NodeId, OutlineKind, Rect, UiCanvasEdge, UiCanvasNode,
    UiCursorNote, UiMenu, UiOverlay, UiShape, Vec2,
};

use crate::edge_layer::EdgeLayer;
use crate::faces::lamp_radius_px;

#[component]
pub fn Overlay(
    nodes: Vec<UiCanvasNode>,
    rects: Memo<BTreeMap<NodeId, Rect>>,
    overlay: UiOverlay,
    edges: Vec<(UiCanvasEdge, Arrow)>,
    settling: bool,
    camera: Camera,
    viewport: Vec2,
) -> Element {
    let rects = rects.read();
    // Modules' tabs under fixtures', in tree order.
    let titles: Vec<(UiCanvasNode, Rect)> = nodes
        .into_iter()
        .filter(|n| n.frame != FrameKind::Card)
        .filter_map(|n| {
            let r = *rects.get(&n.id)?;
            let r = Rect::from_corners(camera.to_screen(r.min), camera.to_screen(r.max));
            Some((n, r))
        })
        .collect();
    let lamp_r = lamp_radius_px(camera.scale);
    let (vw, vh) = (viewport.x, viewport.y);
    rsx! {
        svg { class: "overlay", width: "{vw}", height: "{vh}",
            for (n, r) in titles.iter() {
                g {
                    key: "{n.id}",
                    class: title_class(n),
                    rect {
                        class: "frame-tab",
                        x: "{r.min.x}", y: "{r.min.y - FRAME_TITLE_PX - 2.0}",
                        width: "{n.header.title_px}", height: "{FRAME_TITLE_PX}",
                        rx: "3",
                    }
                    text { class: "frame-title", x: "{r.min.x + 7.0}", y: "{r.min.y - 7.0}", "{n.header.name}" }
                    if n.header.pinned {
                        {pin_mark(Vec2::new(r.min.x + n.header.title_px - 9.0, r.min.y - 11.0))}
                    }
                }
            }
            EdgeLayer { edges, settling }
            for (i, d) in overlay.draft.iter().enumerate() {
                g { key: "d{i}", class: "obj obj-draft",
                    {shape(&d.shape)}
                    for (j, (p, inside)) in d.lamps.iter().enumerate() {
                        circle {
                            key: "{j}",
                            class: if *inside { "lamp" } else { "lamp lamp-outside" },
                            cx: "{p.x}", cy: "{p.y}", r: "{lamp_r}",
                        }
                    }
                }
            }
            for (i, p) in overlay.draft_points.iter().enumerate() {
                rect {
                    key: "dp{i}",
                    class: "draft-point",
                    x: "{p.x - HANDLE_PX}", y: "{p.y - HANDLE_PX}",
                    width: "{2.0 * HANDLE_PX}", height: "{2.0 * HANDLE_PX}",
                }
            }
            for (i, p) in overlay.marked_lamps.iter().enumerate() {
                circle { key: "k{i}", class: "lamp-ring-port", cx: "{p.x}", cy: "{p.y}", r: "{lamp_r + 2.5}" }
            }
            for (i, o) in overlay.outlines.iter().enumerate() {
                g { key: "o{i}",
                    if let Some(w) = o.stroke {
                        // A path's area: a band round its polyline. The band
                        // is one stroke; its edge is a wider stroke with the
                        // band masked out of it.
                        mask {
                            id: "om{i}",
                            mask_units: "userSpaceOnUse",
                            x: "0", y: "0", width: "{vw}", height: "{vh}",
                            rect { x: "0", y: "0", width: "{vw}", height: "{vh}", fill: "white" }
                            path { class: "outline-mask", d: "{o.path}", style: "stroke-width: {w}px" }
                        }
                        path { class: "{outline_class(o.kind)}-band", d: "{o.path}", style: "stroke-width: {w}px" }
                        path {
                            class: "{outline_class(o.kind)}-edge",
                            d: "{o.path}",
                            mask: "url(#om{i})",
                            style: "stroke-width: {w + 3.0}px",
                        }
                    } else {
                        path { class: "{outline_class(o.kind)}", d: "{o.path}", fill_rule: "evenodd" }
                    }
                    if let Some((label, at)) = &o.label {
                        text { class: "{outline_class(o.kind)}-label", x: "{at.x}", y: "{at.y - 5.0}", text_anchor: "middle", "{label}" }
                    }
                }
            }
            for (i, h) in overlay.handles.iter().enumerate() {
                rect {
                    key: "h{i}",
                    class: if h.on { "handle handle-on" } else { "handle" },
                    x: "{h.rect.min.x}", y: "{h.rect.min.y}",
                    width: "{h.rect.width()}", height: "{h.rect.height()}",
                }
            }
            if let Some(r) = overlay.selection_box {
                rect { class: "select-box", x: "{r.min.x}", y: "{r.min.y}", width: "{r.width()}", height: "{r.height()}" }
            }
        }
    }
}

/// The right-click list: what you can do here, then everything under the
/// cursor. It takes its own clicks, so none reach the canvas under it.
#[component]
pub fn Menu(menu: UiMenu, viewport: Vec2, ed: Signal<Editor>) -> Element {
    let mut ed = ed;
    rsx! {
        div {
            class: "menu",
            style: "{menu_place(menu.at, viewport.x, viewport.y)}",
            onmousedown: move |evt| {
                evt.prevent_default();
                evt.stop_propagation();
            },
            onwheel: move |evt| evt.stop_propagation(),
            onmouseleave: move |_| ed.write().menu_hover(None),
            for (i, a) in menu.actions.into_iter().enumerate() {
                div {
                    key: "a{i}",
                    class: "menu-item menu-action",
                    onclick: move |_| ed.write().menu_act(i),
                    span { "{a.label}" }
                    kbd { "{a.keys}" }
                }
            }
            if !menu.items.is_empty() {
                div { class: "menu-title", "Under the cursor" }
            }
            for (i, it) in menu.items.into_iter().enumerate() {
                div {
                    key: "{i}",
                    class: if it.hovered { "menu-item on" } else { "menu-item" },
                    style: "padding-left: {10 + it.depth * 14}px",
                    onmouseenter: move |_| ed.write().menu_hover(Some(i)),
                    onclick: move |evt| ed.write().menu_choose(i, evt.modifiers().shift()),
                    "{it.label}"
                }
            }
            div { class: "menu-foot", kbd { "⇧" } " adds · " kbd { "Esc" } " closes" }
        }
    }
}

#[component]
pub fn CursorNote(note: UiCursorNote) -> Element {
    rsx! {
        div {
            class: "cursor-note",
            style: "left: {note.at.x + 16.0}px; top: {note.at.y + 18.0}px",
            "{note.text}"
        }
    }
}

fn title_class(n: &UiCanvasNode) -> &'static str {
    match (n.frame == FrameKind::Module, n.emphasis == Emphasis::Dimmed) {
        (true, true) => "frame-title-tab module emph-dimmed",
        (true, false) => "frame-title-tab module",
        (false, true) => "frame-title-tab emph-dimmed",
        (false, false) => "frame-title-tab",
    }
}

fn outline_class(k: OutlineKind) -> &'static str {
    match k {
        OutlineKind::Context => "outline-context",
        OutlineKind::Selected => "outline-selected",
        OutlineKind::Hover => "outline-hover",
    }
}

fn shape(s: &UiShape) -> Element {
    match s {
        UiShape::Polyline(points) => {
            let d: String = points
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}{},{} ", if i == 0 { "M" } else { "L" }, p.x, p.y))
                .collect();
            rsx! { path { class: "stroke", d: "{d}" } }
        }
        UiShape::Ring { center, radius } => rsx! {
            circle { class: "stroke", cx: "{center.x}", cy: "{center.y}", r: "{radius}" }
        },
    }
}

/// The pinned mark: a small pin, its head at `at` (screen).
fn pin_mark(at: Vec2) -> Element {
    rsx! {
        g { class: "pin-mark",
            line { x1: "{at.x}", y1: "{at.y}", x2: "{at.x}", y2: "{at.y + 6.0}" }
            circle { cx: "{at.x}", cy: "{at.y}", r: "2.6" }
        }
    }
}

/// Where the right-click list opens: beside the cursor, flipped inward when
/// it would run off the canvas.
fn menu_place(at: Vec2, vw: f64, vh: f64) -> String {
    const W: f64 = 200.0;
    const H: f64 = 220.0;
    let x = if at.x + 4.0 + W > vw {
        (at.x - 4.0 - W).max(0.0)
    } else {
        at.x + 4.0
    };
    let y = if at.y + 4.0 + H > vh {
        (at.y - 4.0 - H).max(0.0)
    } else {
        at.y + 4.0
    };
    format!("left: {x}px; top: {y}px")
}
