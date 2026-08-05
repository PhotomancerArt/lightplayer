//! The properties popover: anchored beside the selection bbox (parent
//! decision D8 — popover, not sidebar).
//!
//! Field edits use the gesture pattern: `oninput` previews through
//! `edit_uncommitted` (live canvas update, no undo spam), `onchange`
//! commits one undo step. Segmented switches and delete are single-step
//! `edit`s. Hidden while a canvas drag is live.

use dioxus::prelude::*;
use dioxus_icons::lucide::{ChevronDown, ChevronUp, Trash2, Ungroup};
use lpc_mapping::{
    GridCorner, GridRouting, Map2dShape, RingDir, RingOrder, bounds_of_points, resolve,
};

use crate::editor_core::camera::Camera;
use crate::editor_core::editor_session::MapEditorSession;
use crate::view::editor_canvas::CanvasDrag;

const POPOVER_WIDTH: f32 = 236.0;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PropertiesPopover(
    session: Signal<MapEditorSession>,
    camera: Signal<Camera>,
    viewport: Signal<Option<[f32; 2]>>,
    drag: Signal<Option<CanvasDrag>>,
    on_committed: EventHandler<()>,
) -> Element {
    let session_read = session.read();
    let selection = session_read.selection.clone();
    if selection.objects.is_empty() || drag().is_some() {
        return rsx! {};
    }
    let doc = session_read.doc();
    let resolved = resolve(doc).ok();
    let selected_positions: Vec<[f32; 2]> = resolved
        .as_ref()
        .map(|resolved| {
            resolved
                .lamps
                .iter()
                .filter(|lamp| selection.objects.contains(&(lamp.object as usize)))
                .map(|lamp| lamp.pos)
                .collect()
        })
        .unwrap_or_default();
    let Some(bounds) = bounds_of_points(&selected_positions) else {
        return rsx! {};
    };

    // Anchor right of the bbox; flip left / clamp inside the viewport.
    let cam = camera();
    let [viewport_width, viewport_height] = viewport().unwrap_or([1200.0, 800.0]);
    let right = cam.doc_to_view([bounds.min_x + bounds.width, bounds.min_y]);
    let left = cam.doc_to_view([bounds.min_x, bounds.min_y]);
    let mut x = right[0] + 22.0;
    if x + POPOVER_WIDTH > viewport_width - 10.0 {
        x = left[0] - POPOVER_WIDTH - 22.0;
    }
    let x = x.clamp(10.0, (viewport_width - POPOVER_WIDTH - 10.0).max(10.0));
    let y = right[1].clamp(10.0, (viewport_height - 120.0).max(10.0));

    let lamp_total = selected_positions.len();
    let single = selection.single();
    let object = single.and_then(|index| doc.objects.get(index).cloned());
    let span = single.and_then(|index| {
        resolved
            .as_ref()
            .and_then(|resolved| resolved.spans.get(index).copied())
    });
    let count_summary = selection.objects.len();
    let object_total = doc.objects.len();
    drop(session_read);

    // Shared editing helpers ------------------------------------------------

    let mut commit = move || {
        session.write().commit_gesture();
        on_committed.call(());
    };
    let delete_selected = move |_| {
        session.write().delete_selection();
        on_committed.call(());
    };

    rsx! {
        div {
            class: "lpme-popover",
            style: "left: {x}px; top: {y}px; width: {POPOVER_WIDTH}px;",
            // Keep canvas shortcuts out of field typing.
            onkeydown: move |evt| evt.stop_propagation(),
            if let (Some(index), Some(object)) = (single, object) {
                div { class: "lpme-pop-head",
                    input {
                        class: "lpme-pop-name",
                        r#type: "text",
                        value: "{object.name}",
                        oninput: move |evt| {
                            let value = evt.value();
                            session.write().edit_uncommitted(move |doc| {
                                if let Some(object) = doc.objects.get_mut(index) {
                                    object.name = value;
                                }
                            });
                        },
                        onchange: move |_| commit(),
                    }
                    span { class: "lpme-pop-kind", {shape_kind_label(&object.shape)} }
                }
                {shape_fields(session, on_committed, index, object.shape.clone())}
                if let Some(span) = span {
                    div { class: "lpme-pop-meta",
                        "{span.count} lamps · chain {span.start + 1}–{span.start + span.count} · u {crate::view::object_list::universe_range_label(span.start, span.count)}"
                    }
                }
            } else {
                div { class: "lpme-pop-head",
                    span { class: "lpme-pop-name-static", "{count_summary} objects" }
                    span { class: "lpme-pop-kind", "{lamp_total} lamps" }
                }
                div { class: "lpme-pop-meta", "drag corner handles to resize the group" }
            }
            div { class: "lpme-pop-actions",
                if let (Some(index), Some(object)) = (single, single.and_then(|i| session.read().doc().objects.get(i).cloned()))
                    && !matches!(object.shape, Map2dShape::Path(_))
                {
                    button {
                        class: "lpme-btn",
                        title: "expand to a plain path (hand-tweakable, same lamps)",
                        onclick: move |_| {
                            session.write().expand_object(index);
                            on_committed.call(());
                        },
                        Ungroup { size: 13 }
                        "expand"
                    }
                }
                if let Some(index) = single {
                    button {
                        class: "lpme-btn",
                        title: "earlier in the wiring chain",
                        disabled: index == 0,
                        onclick: move |_| {
                            if index > 0 {
                                session.write().reorder_object(index, index - 1);
                                on_committed.call(());
                            }
                        },
                        ChevronUp { size: 13 }
                    }
                    button {
                        class: "lpme-btn",
                        title: "later in the wiring chain",
                        disabled: index + 1 >= object_total,
                        onclick: move |_| {
                            if index + 1 < object_total {
                                session.write().reorder_object(index, index + 1);
                                on_committed.call(());
                            }
                        },
                        ChevronDown { size: 13 }
                    }
                }
                div { class: "lpme-spacer" }
                button {
                    class: "lpme-btn lpme-btn-danger",
                    onclick: delete_selected,
                    Trash2 { size: 13 }
                    if single.is_some() { "delete" } else { "delete all" }
                }
            }
        }
    }
}

fn shape_kind_label(shape: &Map2dShape) -> &'static str {
    match shape {
        Map2dShape::Grid(_) => "grid",
        Map2dShape::Ring(_) => "ring",
        Map2dShape::Path(_) => "path",
    }
}

fn shape_fields(
    session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
    index: usize,
    shape: Map2dShape,
) -> Element {
    match shape {
        Map2dShape::Grid(grid) => {
            let routing_current = if matches!(grid.routing, GridRouting::Snake) {
                "snake"
            } else {
                "raster"
            };
            let corner_current = match grid.start_corner {
                GridCorner::Tl => "tl",
                GridCorner::Tr => "tr",
                GridCorner::Bl => "bl",
                GridCorner::Br => "br",
            };
            rsx! {
                NumberField { session, on_committed, index, label: "cols", value: grid.cols as f32, min: 1.0, is_int: true,
                    apply: FieldApply::GridCols }
                NumberField { session, on_committed, index, label: "rows", value: grid.rows as f32, min: 1.0, is_int: true,
                    apply: FieldApply::GridRows }
                NumberField { session, on_committed, index, label: "pitch", value: grid.pitch, min: 0.5, is_int: false,
                    apply: FieldApply::GridPitch }
                SegField {
                    session, on_committed, index, label: "routing",
                    options: vec![("snake", "snake"), ("raster", "raster")],
                    current: routing_current,
                    apply: FieldApply::GridRouting,
                }
                SegField {
                    session, on_committed, index, label: "start corner",
                    options: vec![("tl", "↖"), ("tr", "↗"), ("bl", "↙"), ("br", "↘")],
                    current: corner_current,
                    apply: FieldApply::GridCorner,
                }
            }
        }
        Map2dShape::Ring(ring) => {
            let order_current = if matches!(ring.order, RingOrder::OuterFirst) {
                "outer"
            } else {
                "inner"
            };
            let dir_current = if matches!(ring.dir, RingDir::Cw) {
                "cw"
            } else {
                "ccw"
            };
            rsx! {
                NumberField { session, on_committed, index, label: "outer count", value: ring.outer_count as f32, min: 1.0, is_int: true,
                    apply: FieldApply::RingCount }
                NumberField { session, on_committed, index, label: "radius", value: ring.radius, min: 1.0, is_int: false,
                    apply: FieldApply::RingRadius }
                NumberField { session, on_committed, index, label: "rings", value: ring.rings as f32, min: 1.0, is_int: true,
                    apply: FieldApply::RingRings }
                if ring.rings > 1 {
                    RingCountsField { session, on_committed, index, counts: ring.counts.clone() }
                    SegField {
                        session, on_committed, index, label: "ring order",
                        options: vec![("outer", "out→in"), ("inner", "in→out")],
                        current: order_current,
                        apply: FieldApply::RingOrder,
                    }
                }
                NumberField { session, on_committed, index, label: "start angle", value: ring.start_angle_deg, min: -360.0, is_int: false,
                    apply: FieldApply::RingAngle }
                SegField {
                    session, on_committed, index, label: "direction",
                    options: vec![("cw", "cw ↻"), ("ccw", "ccw ↺")],
                    current: dir_current,
                    apply: FieldApply::RingDir,
                }
            }
        }
        Map2dShape::Path(path) => {
            let dir_current = if path.reversed { "rev" } else { "fwd" };
            rsx! {
                NumberField { session, on_committed, index, label: "count", value: path.count as f32, min: 1.0, is_int: true,
                    apply: FieldApply::PathCount }
                SegField {
                    session, on_committed, index, label: "direction",
                    options: vec![("fwd", "forward"), ("rev", "reversed")],
                    current: dir_current,
                    apply: FieldApply::PathReversed,
                }
                PathGapsField { session, on_committed, index, gaps: path.gaps.clone() }
            }
        }
    }
}

/// Which shape field a numeric/segmented input drives.
#[derive(Clone, Copy, PartialEq)]
pub enum FieldApply {
    GridCols,
    GridRows,
    GridPitch,
    GridRouting,
    GridCorner,
    RingCount,
    RingRadius,
    RingRings,
    RingOrder,
    RingAngle,
    RingDir,
    PathCount,
    PathReversed,
}

fn apply_number(shape: &mut Map2dShape, apply: FieldApply, value: f32) {
    match (apply, shape) {
        (FieldApply::GridCols, Map2dShape::Grid(grid)) => grid.cols = value.max(1.0) as u32,
        (FieldApply::GridRows, Map2dShape::Grid(grid)) => grid.rows = value.max(1.0) as u32,
        (FieldApply::GridPitch, Map2dShape::Grid(grid)) => grid.pitch = value,
        (FieldApply::RingCount, Map2dShape::Ring(ring)) => ring.outer_count = value.max(1.0) as u32,
        (FieldApply::RingRadius, Map2dShape::Ring(ring)) => ring.radius = value,
        (FieldApply::RingRings, Map2dShape::Ring(ring)) => ring.rings = value.max(1.0) as u32,
        (FieldApply::RingAngle, Map2dShape::Ring(ring)) => ring.start_angle_deg = value,
        (FieldApply::PathCount, Map2dShape::Path(path)) => path.count = value.max(1.0) as u32,
        _ => {}
    }
}

fn apply_choice(shape: &mut Map2dShape, apply: FieldApply, choice: &str) {
    match (apply, shape) {
        (FieldApply::GridRouting, Map2dShape::Grid(grid)) => {
            grid.routing = if choice == "snake" {
                GridRouting::Snake
            } else {
                GridRouting::Raster
            };
        }
        (FieldApply::GridCorner, Map2dShape::Grid(grid)) => {
            grid.start_corner = match choice {
                "tr" => GridCorner::Tr,
                "bl" => GridCorner::Bl,
                "br" => GridCorner::Br,
                _ => GridCorner::Tl,
            };
        }
        (FieldApply::RingOrder, Map2dShape::Ring(ring)) => {
            ring.order = if choice == "inner" {
                RingOrder::InnerFirst
            } else {
                RingOrder::OuterFirst
            };
        }
        (FieldApply::RingDir, Map2dShape::Ring(ring)) => {
            ring.dir = if choice == "ccw" {
                RingDir::Ccw
            } else {
                RingDir::Cw
            };
        }
        (FieldApply::PathReversed, Map2dShape::Path(path)) => {
            path.reversed = choice == "rev";
        }
        _ => {}
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NumberField(
    session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
    index: usize,
    label: &'static str,
    value: f32,
    min: f32,
    is_int: bool,
    apply: FieldApply,
) -> Element {
    let shown = if is_int {
        format!("{}", value as i64)
    } else {
        format!("{}", (value * 10.0).round() / 10.0)
    };
    rsx! {
        div { class: "lpme-field",
            label { "{label}" }
            input {
                r#type: "number",
                min: "{min}",
                value: "{shown}",
                oninput: move |evt| {
                    if let Ok(parsed) = evt.value().parse::<f32>()
                        && parsed.is_finite()
                    {
                        session.write().edit_uncommitted(move |doc| {
                            if let Some(object) = doc.objects.get_mut(index) {
                                apply_number(&mut object.shape, apply, parsed);
                            }
                        });
                    }
                },
                onchange: move |_| {
                    session.write().commit_gesture();
                    on_committed.call(());
                },
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SegField(
    session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
    index: usize,
    label: &'static str,
    options: Vec<(&'static str, &'static str)>,
    current: &'static str,
    apply: FieldApply,
) -> Element {
    rsx! {
        div { class: "lpme-field",
            label { "{label}" }
            div { class: "lpme-seg",
                for (value, text) in options {
                    button {
                        key: "{value}",
                        class: if value == current { "lpme-seg-on" } else { "" },
                        onclick: move |_| {
                            session.write().edit(move |doc| {
                                if let Some(object) = doc.objects.get_mut(index) {
                                    apply_choice(&mut object.shape, apply, value);
                                }
                            });
                            on_committed.call(());
                        },
                        "{text}"
                    }
                }
            }
        }
    }
}

/// Comma-separated inert segment indices; empty = every segment lit. Segment
/// `i` runs from vertex `i` to vertex `i + 1`, so `0` is the first leg of the
/// polyline — the numbering the document itself uses. Sanitize sorts, dedupes
/// and clamps whatever is typed, so a half-typed list is never a dead end.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PathGapsField(
    session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
    index: usize,
    gaps: Vec<u32>,
) -> Element {
    let shown = gaps
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    rsx! {
        div { class: "lpme-field",
            label { title: "segment i runs vertex i → vertex i+1; listed segments are jumper wire and carry no lamps", "gaps" }
            input {
                r#type: "text",
                placeholder: "none",
                value: "{shown}",
                oninput: move |evt| {
                    let parsed: Vec<u32> = evt
                        .value()
                        .split(',')
                        .filter_map(|token| token.trim().parse::<u32>().ok())
                        .collect();
                    session.write().edit_uncommitted(move |doc| {
                        if let Some(object) = doc.objects.get_mut(index)
                            && let Map2dShape::Path(path) = &mut object.shape
                        {
                            path.gaps = parsed;
                        }
                    });
                },
                onchange: move |_| {
                    session.write().commit_gesture();
                    on_committed.call(());
                },
            }
        }
    }
}

/// Comma-separated per-ring counts (outer→inner); empty = derived. Typing
/// previews through the gesture path; commit on change.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RingCountsField(
    session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
    index: usize,
    counts: Vec<u32>,
) -> Element {
    let shown = counts
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    rsx! {
        div { class: "lpme-field",
            label { "ring counts" }
            input {
                r#type: "text",
                placeholder: "auto",
                value: "{shown}",
                oninput: move |evt| {
                    let parsed: Vec<u32> = evt
                        .value()
                        .split(',')
                        .filter_map(|token| token.trim().parse::<u32>().ok())
                        .collect();
                    session.write().edit_uncommitted(move |doc| {
                        if let Some(object) = doc.objects.get_mut(index)
                            && let Map2dShape::Path(_) = &object.shape
                        {
                            // not a ring; ignore
                        } else if let Some(object) = doc.objects.get_mut(index)
                            && let Map2dShape::Ring(ring) = &mut object.shape
                        {
                            ring.counts = parsed;
                        }
                    });
                },
                onchange: move |_| {
                    session.write().commit_gesture();
                    on_committed.call(());
                },
            }
        }
    }
}
