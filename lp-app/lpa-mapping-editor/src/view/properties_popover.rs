//! The properties popover: anchored beside the selection bbox (parent
//! decision D8 — popover, not sidebar).
//!
//! Field edits use the gesture pattern: `oninput` previews through
//! `edit_uncommitted` (live canvas update, no undo spam), `onchange`
//! commits one undo step. Segmented switches and delete are single-step
//! `edit`s. Hidden while a canvas drag is live.

use dioxus::prelude::*;
use dioxus_icons::lucide::{ChevronDown, ChevronUp, Minimize2, RotateCw, Trash2, Ungroup};
use lpc_mapping::{
    GridCorner, GridRouting, Map2dShape, RingDir, RingOrder, bounds_of_points, resolve,
};

use crate::editor_core::camera::Camera;
use crate::editor_core::editor_session::{DEFAULT_REPEAT_COUNT, MapEditorSession};
use crate::editor_core::shape_path::ShapePath;
use crate::view::editor_canvas::CanvasDrag;

const POPOVER_WIDTH: f32 = 236.0;

/// What the stored popover anchor is keyed to: it may move only when the
/// SELECTION IDENTITY or the camera moves — never because a property edit
/// reshaped the selection's bbox (the G2 stepper defect).
#[derive(Clone, PartialEq)]
struct AnchorKey {
    paths: Vec<ShapePath>,
    camera: [f32; 3],
}

#[derive(Clone, PartialEq)]
struct StoredAnchor {
    key: AnchorKey,
    position: [f32; 2],
}

/// The anchor policy, pure for tests: reuse the stored position (re-clamped
/// to the viewport) while the key stands; compute fresh from the selection
/// bbox otherwise.
fn resolve_anchor(
    stored: Option<&StoredAnchor>,
    key: &AnchorKey,
    bounds: lpc_mapping::Bounds2d,
    cam: &Camera,
    viewport: [f32; 2],
) -> [f32; 2] {
    if let Some(stored) = stored
        && stored.key == *key
    {
        return clamp_anchor(stored.position, viewport);
    }
    let right = cam.doc_to_view([bounds.min_x + bounds.width, bounds.min_y]);
    let left = cam.doc_to_view([bounds.min_x, bounds.min_y]);
    let mut x = right[0] + 22.0;
    if x + POPOVER_WIDTH > viewport[0] - 10.0 {
        x = left[0] - POPOVER_WIDTH - 22.0;
    }
    clamp_anchor([x, right[1]], viewport)
}

fn clamp_anchor(position: [f32; 2], viewport: [f32; 2]) -> [f32; 2] {
    [
        position[0].clamp(10.0, (viewport[0] - POPOVER_WIDTH - 10.0).max(10.0)),
        position[1].clamp(10.0, (viewport[1] - 120.0).max(10.0)),
    ]
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PropertiesPopover(
    session: Signal<MapEditorSession>,
    camera: Signal<Camera>,
    viewport: Signal<Option<[f32; 2]>>,
    drag: Signal<Option<CanvasDrag>>,
    on_committed: EventHandler<()>,
) -> Element {
    // Hook order: created before any early return.
    let mut stored_anchor = use_signal(|| None::<StoredAnchor>);
    let session_read = session.read();
    let selection = session_read.selection.clone();
    if selection.is_empty() || drag().is_some() {
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
                .filter(|lamp| selection.object_selected(lamp.object as usize))
                .map(|lamp| lamp.pos)
                .collect()
        })
        .unwrap_or_default();
    let Some(bounds) = bounds_of_points(&selected_positions) else {
        return rsx! {};
    };

    let cam = camera();
    let [viewport_width, viewport_height] = viewport().unwrap_or([1200.0, 800.0]);
    // STABLE anchor: computed when the selection identity or camera moves,
    // reused (re-clamped only) across doc edits — a count stepper that
    // reshapes the selection bbox must not move the box out from under a
    // double-click (G2 defect, 2026-08-05).
    let anchor_key = AnchorKey {
        paths: selection.paths().cloned().collect(),
        camera: [cam.x, cam.y, cam.scale],
    };
    let [x, y] = resolve_anchor(
        stored_anchor.peek().as_ref(),
        &anchor_key,
        bounds,
        &cam,
        [viewport_width, viewport_height],
    );
    if stored_anchor
        .peek()
        .as_ref()
        .is_none_or(|stored| stored.key != anchor_key || stored.position != [x, y])
    {
        stored_anchor.set(Some(StoredAnchor {
            key: anchor_key,
            position: [x, y],
        }));
    }

    let lamp_total = selected_positions.len();
    // The popover presents the SELECTED NODE: its fields at the selection's
    // depth, with a breadcrumb standing in for the name row when descended
    // (the breadcrumb replaces inline recursion — selection/tree ADR).
    let single_path = selection.single().cloned();
    let single = single_path.as_ref().map(|path| path.object);
    let base_depth = single_path
        .as_ref()
        .map(|path| path.descent.len())
        .unwrap_or(0);
    let object = single.and_then(|index| doc.objects.get(index).cloned());
    let selected_shape = single_path
        .as_ref()
        .and_then(|path| path.resolve(doc))
        .cloned();
    // Breadcrumb labels root→selected: (label, path to select when clicked;
    // None = the current node, not clickable).
    let crumbs: Vec<(String, Option<ShapePath>)> = single_path
        .as_ref()
        .filter(|path| !path.is_root())
        .map(|path| {
            let mut crumbs = Vec::new();
            for level in 0..=path.descent.len() {
                let ancestor = ShapePath {
                    object: path.object,
                    descent: path.descent[..level].to_vec(),
                };
                let label = if level == 0 {
                    object
                        .as_ref()
                        .map(|object| object.name.clone())
                        .unwrap_or_default()
                } else {
                    ancestor
                        .resolve(doc)
                        .map(shape_kind_label)
                        .unwrap_or("?")
                        .to_string()
                };
                let target = (level < path.descent.len()).then_some(ancestor);
                crumbs.push((label, target));
            }
            crumbs
        })
        .unwrap_or_default();
    // The object's whole lamp range, strands merged: a repeat resolves to one
    // span per instance, so `spans[index]` would report only its first strand.
    let span = single.and_then(|index| {
        resolved
            .as_ref()
            .and_then(|resolved| resolved.object_span(index as u32))
    });
    let count_summary = selection.len();
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
            if let (Some(index), Some(object), Some(selected_shape)) =
                (single, object, selected_shape)
            {
                div { class: "lpme-pop-head",
                    if crumbs.is_empty() {
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
                    } else {
                        // Descended: the crumb trail names the scope chain;
                        // clicking an ancestor ascends to it (rename lives
                        // on the root selection).
                        span { class: "lpme-pop-crumbs",
                            for (slot, (label, target)) in crumbs.into_iter().enumerate() {
                                if slot > 0 {
                                    span { class: "lpme-pop-crumb-sep", "▸" }
                                }
                                if let Some(target) = target {
                                    button {
                                        class: "lpme-pop-crumb",
                                        onclick: move |_| {
                                            session.write().selection.select_only_path(target.clone());
                                        },
                                        "{label}"
                                    }
                                } else {
                                    span { class: "lpme-pop-crumb-here", "{label}" }
                                }
                            }
                        }
                    }
                }
                {shape_fields(session, on_committed, index, selected_shape, base_depth)}
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
                        title: if matches!(object.shape, Map2dShape::Repeat(_)) {
                            "expand to independent objects, one per instance (same lamps, hand-tweakable)"
                        } else {
                            "expand to a plain path (hand-tweakable, same lamps)"
                        },
                        onclick: move |_| {
                            session.write().expand_object(index);
                            on_committed.call(());
                        },
                        Ungroup { size: 13 }
                        "expand"
                    }
                }
                // Wrap ⇄ unwrap sit beside expand because all three answer the
                // same question — how parametric should this object be. The
                // pair is exclusive: nesting a repeat inside a repeat resolves
                // and edits fine, but reaching it by a stray click on an
                // already-repeated object would multiply strands by surprise.
                if let (Some(index), Some(object)) = (single, single.and_then(|i| session.read().doc().objects.get(i).cloned())) {
                    if matches!(object.shape, Map2dShape::Repeat(_)) {
                        button {
                            class: "lpme-btn",
                            title: "unwrap the repeat: keep this shape, drop the other instances",
                            onclick: move |_| {
                                session.write().unwrap_repeat(index);
                                on_committed.call(());
                            },
                            Minimize2 { size: 13 }
                            "unwrap"
                        }
                    } else {
                        button {
                            class: "lpme-btn",
                            title: "repeat around a point: {DEFAULT_REPEAT_COUNT} turned instances about the canvas center",
                            onclick: move |_| {
                                session.write().repeat_object(index, DEFAULT_REPEAT_COUNT);
                                on_committed.call(());
                            },
                            RotateCw { size: 13 }
                            "repeat"
                        }
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

pub(crate) fn shape_kind_label(shape: &Map2dShape) -> &'static str {
    match shape {
        Map2dShape::Grid(_) => "grid",
        Map2dShape::Ring(_) => "ring",
        Map2dShape::Path(_) => "path",
        Map2dShape::Polygon(_) => "polygon",
        Map2dShape::Repeat(_) => "repeat",
    }
}

/// The fields for one shape.
///
/// `depth` is how many repeat wrappers stand between `doc.objects[index]` and
/// the shape being edited: a repeat renders its own fields and then recurses
/// with `depth + 1` for the inner shape, and every field applies through
/// [`shape_at_depth`] so an inner edit lands on the boxed shape rather than
/// the wrapper.
fn shape_fields(
    mut session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
    index: usize,
    shape: Map2dShape,
    depth: usize,
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
                NumberField { session, on_committed, index, depth, label: "cols", value: grid.cols as f32, min: 1.0, is_int: true,
                    apply: FieldApply::GridCols }
                NumberField { session, on_committed, index, depth, label: "rows", value: grid.rows as f32, min: 1.0, is_int: true,
                    apply: FieldApply::GridRows }
                NumberField { session, on_committed, index, depth, label: "pitch", value: grid.pitch, min: 0.5, is_int: false,
                    apply: FieldApply::GridPitch }
                SegField {
                    session, on_committed, index, depth, label: "routing",
                    options: vec![("snake", "snake"), ("raster", "raster")],
                    current: routing_current,
                    apply: FieldApply::GridRouting,
                }
                SegField {
                    session, on_committed, index, depth, label: "start corner",
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
                NumberField { session, on_committed, index, depth, label: "outer count", value: ring.outer_count as f32, min: 1.0, is_int: true,
                    apply: FieldApply::RingCount }
                NumberField { session, on_committed, index, depth, label: "radius", value: ring.radius, min: 1.0, is_int: false,
                    apply: FieldApply::RingRadius }
                NumberField { session, on_committed, index, depth, label: "rings", value: ring.rings as f32, min: 1.0, is_int: true,
                    apply: FieldApply::RingRings }
                if ring.rings > 1 {
                    RingCountsField { session, on_committed, index, depth, counts: ring.counts.clone() }
                    SegField {
                        session, on_committed, index, depth, label: "ring order",
                        options: vec![("outer", "out→in"), ("inner", "in→out")],
                        current: order_current,
                        apply: FieldApply::RingOrder,
                    }
                }
                NumberField { session, on_committed, index, depth, label: "start angle", value: ring.start_angle_deg, min: -360.0, is_int: false,
                    apply: FieldApply::RingAngle }
                SegField {
                    session, on_committed, index, depth, label: "direction",
                    options: vec![("cw", "cw ↻"), ("ccw", "ccw ↺")],
                    current: dir_current,
                    apply: FieldApply::RingDir,
                }
            }
        }
        Map2dShape::Path(path) => {
            let dir_current = if path.reversed { "rev" } else { "fwd" };
            rsx! {
                NumberField { session, on_committed, index, depth, label: "count", value: path.count as f32, min: 1.0, is_int: true,
                    apply: FieldApply::PathCount }
                SegField {
                    session, on_committed, index, depth, label: "direction",
                    options: vec![("fwd", "forward"), ("rev", "reversed")],
                    current: dir_current,
                    apply: FieldApply::PathReversed,
                }
                PathGapsField { session, on_committed, index, depth, gaps: path.gaps.clone() }
            }
        }
        // A hand-authored polygon (no creation tool yet): the count is the
        // one live parameter; the outline is authored in the document.
        Map2dShape::Polygon(polygon) => {
            rsx! {
                NumberField { session, on_committed, index, depth, label: "count", value: polygon.count as f32, min: 1.0, is_int: true,
                    apply: FieldApply::PolygonCount }
            }
        }
        // A repeat's own parameters, then the shape it turns — one panel, read
        // top to bottom as "N copies of this, about here". The inner fields
        // recurse, so a nested repeat simply shows another instances row.
        Map2dShape::Repeat(repeat) => {
            let inner = (*repeat.shape).clone();
            let inner_kind = shape_kind_label(&inner);
            rsx! {
                NumberField { session, on_committed, index, depth, label: "instances", value: repeat.count as f32, min: 1.0, is_int: true,
                    apply: FieldApply::RepeatCount }
                NumberField { session, on_committed, index, depth, label: "center x", value: repeat.center[0], min: -100000.0, is_int: false,
                    apply: FieldApply::RepeatCenterX }
                NumberField { session, on_committed, index, depth, label: "center y", value: repeat.center[1], min: -100000.0, is_int: false,
                    apply: FieldApply::RepeatCenterY }
                // The inner shape's fields live one descent down — the
                // breadcrumb replaces inline recursion (selection/tree ADR):
                // entering the group selects the sub-object and this popover
                // re-renders showing its fields.
                div { class: "lpme-field",
                    label { "repeats" }
                    button {
                        class: "lpme-pop-descend",
                        title: "edit the repeated sub-object (double-click on canvas does the same)",
                        onclick: move |_| {
                            session.write().selection.select_only_path(ShapePath {
                                object: index,
                                descent: vec![0; depth + 1],
                            });
                        },
                        "{inner_kind} ▸"
                    }
                }
            }
        }
    }
}

/// The shape a field at `depth` edits: `depth` steps down through repeat
/// wrappers from the object's own shape.
///
/// Returns `None` when the document changed shape under the popover (an undo
/// landing mid-edit, say) — the edit is then simply dropped rather than
/// written to whatever now sits at that slot.
fn shape_at_depth(shape: &mut Map2dShape, depth: usize) -> Option<&mut Map2dShape> {
    let mut current = shape;
    for _ in 0..depth {
        match current {
            Map2dShape::Repeat(repeat) => current = &mut repeat.shape,
            _ => return None,
        }
    }
    Some(current)
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
    PolygonCount,
    RepeatCount,
    RepeatCenterX,
    RepeatCenterY,
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
        (FieldApply::PolygonCount, Map2dShape::Polygon(polygon)) => {
            polygon.count = value.max(1.0) as u32;
        }
        // Sanitize owns the upper bound (`MAX_REPEAT_COUNT`) so a typed digit
        // that overshoots is clamped on commit rather than refused mid-typing.
        (FieldApply::RepeatCount, Map2dShape::Repeat(repeat)) => {
            repeat.count = value.max(1.0) as u32;
        }
        (FieldApply::RepeatCenterX, Map2dShape::Repeat(repeat)) => repeat.center[0] = value,
        (FieldApply::RepeatCenterY, Map2dShape::Repeat(repeat)) => repeat.center[1] = value,
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
    depth: usize,
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
                            if let Some(object) = doc.objects.get_mut(index)
                                && let Some(shape) = shape_at_depth(&mut object.shape, depth)
                            {
                                apply_number(shape, apply, parsed);
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
    depth: usize,
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
                                if let Some(object) = doc.objects.get_mut(index)
                                    && let Some(shape) = shape_at_depth(&mut object.shape, depth)
                                {
                                    apply_choice(shape, apply, value);
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
    depth: usize,
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
                            && let Some(Map2dShape::Path(path)) = shape_at_depth(&mut object.shape, depth)
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
    depth: usize,
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
                            && let Some(Map2dShape::Ring(ring)) = shape_at_depth(&mut object.shape, depth)
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

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::Bounds2d;

    fn key(paths: Vec<ShapePath>) -> AnchorKey {
        AnchorKey {
            paths,
            camera: [0.0, 0.0, 1.0],
        }
    }

    fn bounds(width: f32) -> Bounds2d {
        Bounds2d {
            min_x: 100.0,
            min_y: 100.0,
            width,
            height: 50.0,
        }
    }

    /// The G2 stepper defect: a doc edit that reshapes the bbox must not
    /// move the box while the selection (and camera) stand still.
    #[test]
    fn unchanged_selection_keeps_the_stored_anchor_across_bbox_changes() {
        let key = key(vec![ShapePath::root(0)]);
        let camera = Camera::new();
        let viewport = [1200.0, 800.0];
        let first = resolve_anchor(None, &key, bounds(200.0), &camera, viewport);
        let stored = StoredAnchor {
            key: key.clone(),
            position: first,
        };
        // The count stepper grew the selection bbox; the anchor stays put.
        let second = resolve_anchor(Some(&stored), &key, bounds(600.0), &camera, viewport);
        assert_eq!(second, first);
    }

    #[test]
    fn a_new_selection_re_anchors() {
        let old_key = key(vec![ShapePath::root(0)]);
        let camera = Camera::new();
        let viewport = [1200.0, 800.0];
        let first = resolve_anchor(None, &old_key, bounds(200.0), &camera, viewport);
        let stored = StoredAnchor {
            key: old_key,
            position: first,
        };
        let new_key = key(vec![ShapePath::root(0).child(0)]);
        let fresh = resolve_anchor(Some(&stored), &new_key, bounds(600.0), &camera, viewport);
        assert_ne!(fresh, first, "a different selection computes fresh");
    }

    #[test]
    fn a_camera_move_re_anchors_and_clamping_keeps_the_box_in_view() {
        let paths = vec![ShapePath::root(1)];
        let camera = Camera::new();
        let viewport = [1200.0, 800.0];
        let stored = StoredAnchor {
            key: key(paths.clone()),
            position: [1500.0, -50.0],
        };
        // Same selection, moved camera: fresh anchor.
        let moved = AnchorKey {
            paths,
            camera: [40.0, 0.0, 1.0],
        };
        let fresh = resolve_anchor(Some(&stored), &moved, bounds(200.0), &camera, viewport);
        assert!(fresh[0] <= viewport[0] - POPOVER_WIDTH - 10.0);
        // And a stored anchor outside the viewport re-clamps without moving
        // its key.
        let clamped = resolve_anchor(
            Some(&stored),
            &stored.key.clone(),
            bounds(200.0),
            &camera,
            viewport,
        );
        assert_eq!(clamped, clamp_anchor([1500.0, -50.0], viewport));
    }
}
