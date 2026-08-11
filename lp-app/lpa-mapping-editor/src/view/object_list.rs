//! The object rail: the document TREE in wiring order, as a docked
//! right-side pane — swatch, name, lamp range, ▲▼ reorder on root rows
//! (chain order IS the wiring order), and an indented child row per
//! structural child (a repeat's authored sub-object, `×N` badged). The
//! rail is the tree — no separate layers panel; wiring order and
//! structure are the same fact here (selection/tree ADR). The host owns
//! the open state and mounts this beside the canvas; docked, it shrinks
//! the view instead of covering it.

use dioxus::prelude::*;
use lpc_mapping::{Map2dShape, resolve};

use crate::editor_core::editor_session::MapEditorSession;
use crate::editor_core::shape_path::{ShapePath, structural_child, structural_child_count};
use crate::view::editor_canvas::object_color;
use crate::view::properties_popover::shape_kind_label;

/// Wiring range as 1-based chain lamp numbers: `"1-23"`.
#[must_use]
pub fn chain_range_label(start: u32, count: u32) -> String {
    format!("{}-{}", start + 1, start + count)
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ObjectList(
    session: Signal<MapEditorSession>,
    on_committed: EventHandler<()>,
    /// Fired with the object index when a row is clicked, after selection —
    /// the host brings the object into view.
    #[props(default)]
    on_focus: Option<EventHandler<usize>>,
) -> Element {
    let session_read = session.read();
    let doc = session_read.doc();
    let resolved = resolve(doc).ok();
    // One row per TREE NODE, depth-first in wiring order. A root row is an
    // object and reports the object's WHOLE range (a repeat resolves to one
    // span per instance; showing only the first would read short); child
    // rows are the authored sub-objects a group repeats.
    let mut rows: Vec<RailRow> = Vec::new();
    for (index, object) in doc.objects.iter().enumerate() {
        let span = resolved
            .as_ref()
            .and_then(|resolved| resolved.object_span(index as u32))
            .map(|span| (span.start, span.count));
        push_rows(
            &mut rows,
            &session_read,
            ShapePath::root(index),
            &object.shape,
            object.name.clone(),
            span,
        );
    }
    let object_count = doc.objects.len();
    drop(session_read);

    rsx! {
        div { class: "lpme-rail-pane",
            div { class: "lpme-rail-head", "wiring order" }
            if object_count == 0 {
                div { class: "lpme-rail-empty", "no objects yet" }
            } else {
                div { class: "lpme-rail-list",
                    for row in rows {
                        {
                            let path = row.path.clone();
                            let toggle_path = row.path.clone();
                            let index = row.path.object;
                            let is_root = row.path.is_root();
                            let depth = row.path.descent.len();
                            rsx! {
                                div {
                                    key: "{row.key}",
                                    class: if row.selected { "lpme-rail-row lpme-rail-row-sel" } else { "lpme-rail-row" },
                                    style: if depth > 0 { "padding-left: {10 + depth * 16}px;" } else { "" },
                                    onclick: move |evt| {
                                        {
                                            let mut s = session.write();
                                            if evt.data().modifiers().shift() {
                                                s.selection.toggle_path(toggle_path.clone());
                                            } else {
                                                s.selection.select_only_path(path.clone());
                                            }
                                        }
                                        // Plain click also brings the object
                                        // into view; additive shift-clicks
                                        // don't yank the camera mid-select.
                                        if !evt.data().modifiers().shift()
                                            && let Some(focus) = &on_focus
                                        {
                                            focus.call(index);
                                        }
                                    },
                                    span {
                                        class: "lpme-rail-swatch",
                                        style: "background: {object_color(index)};",
                                    }
                                    span { class: "lpme-rail-name", "{row.label}" }
                                    if let Some(count) = row.badge {
                                        span { class: "lpme-rail-badge", "×{count}" }
                                    }
                                    if let Some((start, count)) = row.span {
                                        span { class: "lpme-rail-range", "{chain_range_label(start, count)}" }
                                    }
                                    if is_root {
                                        span { class: "lpme-rail-move",
                                            button {
                                                title: "earlier in chain",
                                                disabled: index == 0,
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    if index > 0 {
                                                        session.write().reorder_object(index, index - 1);
                                                        on_committed.call(());
                                                    }
                                                },
                                                "▲"
                                            }
                                            button {
                                                title: "later in chain",
                                                disabled: index + 1 >= object_count,
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    if index + 1 < object_count {
                                                        session.write().reorder_object(index, index + 1);
                                                        on_committed.call(());
                                                    }
                                                },
                                                "▼"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One rail row = one tree node.
struct RailRow {
    key: String,
    path: ShapePath,
    label: String,
    /// Instance count when this node is a repeat (`×N`).
    badge: Option<u32>,
    span: Option<(u32, u32)>,
    selected: bool,
}

/// Depth-first rows for one object's shape chain. Children are always
/// disclosed — arity is ≤1 today, so the tree stays one indented row per
/// group level and needs no open/closed state.
fn push_rows(
    rows: &mut Vec<RailRow>,
    session: &MapEditorSession,
    path: ShapePath,
    shape: &Map2dShape,
    label: String,
    span: Option<(u32, u32)>,
) {
    let badge = match shape {
        Map2dShape::Repeat(repeat) => Some(repeat.count),
        _ => None,
    };
    let key = if path.is_root() {
        format!("o{}", path.object)
    } else {
        format!(
            "o{}d{}",
            path.object,
            path.descent
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join("-")
        )
    };
    rows.push(RailRow {
        key,
        path: path.clone(),
        label,
        badge,
        span,
        selected: session.selection.contains(&path),
    });
    for step in 0..structural_child_count(shape) {
        if let Some(child) = structural_child(shape, step) {
            // Child rows carry no span: the parent row already reports the
            // whole range, and the child IS that range's authored source.
            push_rows(
                rows,
                session,
                path.child(step),
                child,
                shape_kind_label(child).to_string(),
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_annotate_chain_lamp_numbers() {
        assert_eq!(chain_range_label(0, 23), "1-23");
        assert_eq!(chain_range_label(23, 25), "24-48");
        assert_eq!(chain_range_label(147, 30), "148-177");
        assert_eq!(chain_range_label(170, 50), "171-220");
    }
}
