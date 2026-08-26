//! The selected path as a STACK of editable cards, deepest first — the
//! Props dock's B′ ruling (design record: `spikes/props-stack/index.html`,
//! ratified 2026-08-14, amending the selection/tree ADR's breadcrumb
//! presentation). One card per level of the selected path: the SELECTION
//! is always the top card (inspection-panel convention — DevTools styles,
//! debugger call stacks), ancestors unwind beneath it, every card editable
//! in place. Editing the repeat while its inner item stays selected is the
//! workflow the stack exists for.
//!
//! This crate renders only the AUTHORED DOCUMENT levels. The fixture's
//! placement card and the module context strip are shell composition
//! (the workbench's Props panel) — this crate stays project-unaware, so an
//! empty selection renders nothing and the shell owns what shows instead.
//!
//! Fields use the gesture pattern: `oninput` previews through
//! `edit_uncommitted` (live canvas update, no undo spam), `onchange`
//! commits one undo step. Segmented switches and delete are single-step
//! `edit`s. Field edits never move the selection; a card HEADER click
//! selects that level (esc pops the top card via the existing ladder).

use dioxus::prelude::*;
use dioxus_icons::lucide::{ChevronDown, ChevronUp, Minimize2, RotateCw, Trash2, Ungroup};
use lpc_mapping::{GridCorner, GridRouting, Map2dShape, PathAlign, RingDir, RingOrder, resolve};

use crate::editor_core::editor_session::{DEFAULT_REPEAT_COUNT, MapEditorSession};
use crate::editor_core::shape_path::ShapePath;
use crate::view::canvas::object_color;

/// One card's worth of a selected path: the level's path prefix, its
/// shape, whether it is the selection (the top card), and precomputed
/// render facts (key, root span meta) so the rsx loop moves each field
/// exactly once.
struct LevelCard {
    key: String,
    path: ShapePath,
    shape: Map2dShape,
    selected: bool,
    span_meta: Option<String>,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ObjectPropertiesPane(
    session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
) -> Element {
    let session_read = session.read();
    let selection = session_read.selection.clone();
    if selection.is_empty() {
        // Nothing selected: no document cards. The shell renders the
        // fixture's placement card alone in this state — never a
        // "select an object" placeholder.
        return rsx! {};
    }
    let doc = session_read.doc();
    let resolved = resolve(doc).ok();
    let single_path = selection.single().cloned();
    // The ancestor chain the stack renders: the selected path itself, or
    // (multi-select) the derived shared scope — sibling-level selection
    // per the selection/tree ADR, so the scope is every ancestor at once.
    let chain_path = single_path.clone().or_else(|| selection.scope());
    let object_index = chain_path.as_ref().map(|path| path.object);
    let object_name = object_index
        .and_then(|index| doc.objects.get(index))
        .map(|object| object.name.clone())
        .unwrap_or_default();
    // The object's whole lamp range, strands merged: a repeat resolves to
    // one span per instance, so `spans[index]` would report only its first
    // strand.
    let span_meta = object_index
        .and_then(|index| {
            resolved
                .as_ref()
                .and_then(|resolved| resolved.object_span(index as u32))
        })
        .map(|span| {
            format!(
                "{} lamps · chain {}–{}",
                span.count,
                span.start + 1,
                span.start + span.count,
            )
        });
    let cards: Vec<LevelCard> = chain_path
        .as_ref()
        .map(|path| {
            let top = path.descent.len();
            (0..=top)
                .rev()
                .filter_map(|level| {
                    let prefix = ShapePath {
                        object: path.object,
                        descent: path.descent[..level].to_vec(),
                    };
                    let shape = prefix.resolve(doc)?.clone();
                    Some(LevelCard {
                        key: format!("{}-{:?}", prefix.object, prefix.descent),
                        selected: single_path.is_some() && level == top,
                        span_meta: (level == 0).then(|| span_meta.clone()).flatten(),
                        path: prefix,
                        shape,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let lamp_total = resolved
        .as_ref()
        .map(|resolved| {
            resolved
                .lamps
                .iter()
                .filter(|lamp| selection.object_selected(lamp.object as usize))
                .count()
        })
        .unwrap_or_default();
    let count_summary = selection.len();
    let multi = single_path.is_none();
    let object_total = doc.objects.len();
    drop(session_read);

    rsx! {
        // A stack fragment, not a padded pane: the shell composes this
        // above its placement card inside one outer `lpme-stack`, so the
        // nested grid keeps the same 7px rhythm and the host owns padding.
        div {
            class: "lpme-stack",
            // Keep canvas shortcuts out of field typing.
            onkeydown: move |evt| evt.stop_propagation(),
            if multi {
                // The multi-select leaf card: one "N objects" card on top
                // of the shared-ancestor cards (which, at sibling-level
                // multi-select, are the scope's levels).
                div { class: "lpme-lvl lpme-lvl-sel",
                    div { class: "lpme-lvl-head",
                        span { class: "lpme-lvl-name", "{count_summary} objects" }
                        span { class: "lpme-lvl-kind", "{lamp_total} lamps" }
                    }
                    div { class: "lpme-lvl-body",
                        div { class: "lpme-pop-meta", "drag corner handles to resize the group" }
                        div { class: "lpme-pop-actions",
                            button {
                                class: "lpme-btn lpme-btn-danger",
                                onclick: move |_| {
                                    session.write().delete_selection();
                                    on_committed.call(());
                                },
                                Trash2 { size: 13 }
                                "delete all"
                            }
                        }
                    }
                }
            }
            for card in cards {
                LevelCardView {
                    key: "{card.key}",
                    session,
                    on_committed,
                    selected: card.selected,
                    object_name: object_name.clone(),
                    span_meta: card.span_meta,
                    object_total,
                    path: card.path,
                    shape: card.shape,
                }
            }
        }
    }
}

/// One level of the stack: header (swatch · label · kind — click selects
/// this level) over the level's editable fields. The ROOT card carries
/// what acts on the whole authored object — the rename field, the span
/// meta, and the object-level actions (expand, unwrap⇄repeat, reorder,
/// delete: they all operate on `doc.objects[index]`, so the root card is
/// their honest home even while a descended level is selected).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn LevelCardView(
    session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
    path: ShapePath,
    shape: Map2dShape,
    selected: bool,
    object_name: String,
    span_meta: Option<String>,
    object_total: usize,
) -> Element {
    let index = path.object;
    let depth = path.descent.len();
    let is_root = path.is_root();
    let color = object_color(index);
    let kind_text = stack_kind_label(&shape);
    // Root cards show the authored name (or an honest placeholder);
    // descended levels have no names — the kind word IS the label, the
    // same grain the Fixtures tree rows use.
    let label = if is_root {
        if object_name.is_empty() {
            "(unnamed)".to_string()
        } else {
            object_name.clone()
        }
    } else {
        kind_text.clone()
    };
    let card_class = if selected {
        "lpme-lvl lpme-lvl-sel"
    } else {
        "lpme-lvl"
    };
    let swatch_class = if is_root {
        "lpme-lvl-swatch"
    } else {
        "lpme-lvl-swatch lpme-lvl-swatch-hollow"
    };
    let swatch_style = if is_root {
        format!("background: {color};")
    } else {
        format!("border-color: {color};")
    };
    let select_path = path.clone();
    let mut commit = move || {
        session.write().commit_gesture();
        on_committed.call(());
    };
    rsx! {
        div { class: "{card_class}",
            div {
                class: "lpme-lvl-head",
                title: "select this level",
                onclick: move |_| {
                    session.write().selection.select_only_path(select_path.clone());
                },
                span { class: "{swatch_class}", style: "{swatch_style}" }
                span { class: "lpme-lvl-name", "{label}" }
                if is_root {
                    span { class: "lpme-lvl-kind", "{kind_text}" }
                }
            }
            div { class: "lpme-lvl-body",
                if is_root {
                    div { class: "lpme-field",
                        label { "name" }
                        input {
                            r#type: "text",
                            value: "{object_name}",
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
                    }
                }
                {shape_fields(session, on_committed, index, shape.clone(), depth)}
                if let Some(meta) = span_meta {
                    div { class: "lpme-pop-meta", "{meta}" }
                }
                if is_root {
                    ObjectActions { session, on_committed, index, shape, object_total }
                }
            }
        }
    }
}

/// The object-level actions row: how parametric the object is (expand,
/// wrap ⇄ unwrap), wiring order (reorder), and delete. All of these act
/// on the authored root object regardless of the selected depth — delete
/// at depth deletes the whole object (a repeat cannot exist empty; unwrap
/// is the keep-the-inner op, per the selection/tree ADR).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ObjectActions(
    session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
    index: usize,
    shape: Map2dShape,
    object_total: usize,
) -> Element {
    rsx! {
        div { class: "lpme-pop-actions",
            if !matches!(shape, Map2dShape::Path(_)) {
                button {
                    class: "lpme-btn",
                    title: if matches!(shape, Map2dShape::Repeat(_)) {
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
            if matches!(shape, Map2dShape::Repeat(_)) {
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
            div { class: "lpme-spacer" }
            button {
                class: "lpme-btn lpme-btn-danger",
                onclick: move |_| {
                    session.write().delete_selection();
                    on_committed.call(());
                },
                Trash2 { size: 13 }
                "delete"
            }
        }
    }
}

/// The one-word kind label every surface shows for a shape.
pub fn shape_kind_label(shape: &Map2dShape) -> &'static str {
    match shape {
        Map2dShape::Grid(_) => "grid",
        Map2dShape::Ring(_) => "ring",
        Map2dShape::Path(_) => "path",
        Map2dShape::Polygon(_) => "polygon",
        Map2dShape::FilledPolygon(_) => "filled polygon",
        Map2dShape::Repeat(_) => "repeat",
    }
}

/// A card header's kind text: the kind, with a repeat carrying its
/// instance count (`repeat ×5`) — the same grain the tree rows use.
fn stack_kind_label(shape: &Map2dShape) -> String {
    match shape {
        Map2dShape::Repeat(repeat) => format!("repeat ×{}", repeat.count),
        other => shape_kind_label(other).to_string(),
    }
}

/// The fields for one shape.
///
/// `depth` is how many repeat wrappers stand between `doc.objects[index]` and
/// the shape being edited: each level of the stack renders its own card, and
/// every field applies through [`shape_at_depth`] so an inner edit lands on
/// the boxed shape rather than the wrapper.
fn shape_fields(
    session: Signal<MapEditorSession>,
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
                SegField {
                    session, on_committed, index, depth, label: "align",
                    title: "where the lamps sit relative to the drawn path — inside = the strip wraps a form",
                    options: vec![("on", "on path"), ("inside", "inside"), ("outside", "outside")],
                    current: align_current(path.align),
                    apply: FieldApply::PathAlign,
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
                SegField {
                    session, on_committed, index, depth, label: "align",
                    title: "where the lamps sit relative to the drawn path — inside = the strip wraps a form",
                    options: vec![("on", "on path"), ("inside", "inside"), ("outside", "outside")],
                    current: align_current(polygon.align),
                    apply: FieldApply::PolygonAlign,
                }
            }
        }
        // A shaped matrix has no editable fields on this build: its count is
        // derived from the outline and the lattice, and the lattice controls
        // (pitch, angle, phase, routing) arrive with the Polygon tool. The
        // card still renders its header, so the object is nameable and
        // deletable like any other.
        Map2dShape::FilledPolygon(_) => rsx! {},
        // A repeat's own parameters — "N copies, about here". The inner
        // shape is NOT recursed into and needs no descend affordance: the
        // stack shows it as its own card whenever the selection descends,
        // and descent itself lives on the tree and the canvas
        // (double-click).
        Map2dShape::Repeat(repeat) => {
            rsx! {
                NumberField { session, on_committed, index, depth, label: "instances", value: repeat.count as f32, min: 1.0, is_int: true,
                    apply: FieldApply::RepeatCount }
                NumberField { session, on_committed, index, depth, label: "center x", value: repeat.center[0], min: -100000.0, is_int: false,
                    apply: FieldApply::RepeatCenterX }
                NumberField { session, on_committed, index, depth, label: "center y", value: repeat.center[1], min: -100000.0, is_int: false,
                    apply: FieldApply::RepeatCenterY }
            }
        }
    }
}

/// The align segmented control's current-value token for a shape's
/// [`PathAlign`] — the same three tokens the `align` `SegField`'s options
/// and `apply_choice` arms use.
fn align_current(align: PathAlign) -> &'static str {
    match align {
        PathAlign::On => "on",
        PathAlign::Inside => "inside",
        PathAlign::Outside => "outside",
    }
}

/// The shape a field at `depth` edits: `depth` steps down through repeat
/// wrappers from the object's own shape.
///
/// Returns `None` when the document changed shape under the pane (an undo
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
    PathAlign,
    PolygonCount,
    PolygonAlign,
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
        (FieldApply::PathAlign, Map2dShape::Path(path)) => {
            path.align = choice_to_align(choice);
        }
        (FieldApply::PolygonAlign, Map2dShape::Polygon(polygon)) => {
            polygon.align = choice_to_align(choice);
        }
        _ => {}
    }
}

/// The align `SegField`'s option token, parsed back to [`PathAlign`] —
/// the inverse of [`align_current`]. An unrecognized token (there should
/// never be one; the three options are the only buttons rendered) lands
/// on the default `On` rather than panicking.
fn choice_to_align(choice: &str) -> PathAlign {
    match choice {
        "inside" => PathAlign::Inside,
        "outside" => PathAlign::Outside,
        _ => PathAlign::On,
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
    /// Optional tooltip text for the label — same convention as a plain
    /// `title` attribute elsewhere in the panel; empty renders no tooltip.
    #[props(default)]
    title: &'static str,
    options: Vec<(&'static str, &'static str)>,
    current: &'static str,
    apply: FieldApply,
) -> Element {
    rsx! {
        div { class: "lpme-field",
            label { title: "{title}", "{label}" }
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
