//! The object rail: wiring order as a docked right-side pane — swatch,
//! name, lamp range, ▲▼ reorder (chain order IS the wiring order). The
//! host owns the open state and mounts this beside the canvas; docked, it
//! shrinks the view instead of covering it.

use dioxus::prelude::*;
use lpc_mapping::resolve;

use crate::editor_core::editor_session::MapEditorSession;
use crate::editor_core::view_geometry::LAMPS_PER_UNIVERSE;
use crate::view::editor_canvas::object_color;

/// Wiring range annotated per universe: `"1:1-23"`, and across a boundary
/// `"1:148-170 2:1-7"` (universe:within-universe lamp numbers, 1-based).
#[must_use]
pub fn universe_range_label(start: u32, count: u32) -> String {
    let mut parts = Vec::new();
    let mut lamp = start;
    let end = start + count;
    while lamp < end {
        let universe = lamp / LAMPS_PER_UNIVERSE;
        let universe_end = (universe + 1) * LAMPS_PER_UNIVERSE;
        let segment_end = end.min(universe_end);
        parts.push(format!(
            "{}:{}-{}",
            universe + 1,
            lamp % LAMPS_PER_UNIVERSE + 1,
            (segment_end - 1) % LAMPS_PER_UNIVERSE + 1
        ));
        lamp = segment_end;
    }
    parts.join(" ")
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
    // A rail row is one object, so it reports the object's WHOLE range: a
    // repeat resolves to one span per instance, and a row showing only the
    // first instance's lamps would read as a shorter object than it is.
    let rows: Vec<(usize, String, Option<(u32, u32)>, bool)> = doc
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| {
            (
                index,
                object.name.clone(),
                resolved
                    .as_ref()
                    .and_then(|resolved| resolved.object_span(index as u32))
                    .map(|span| (span.start, span.count)),
                session_read.selection.contains_root(index),
            )
        })
        .collect();
    let object_count = rows.len();
    drop(session_read);

    rsx! {
        div { class: "lpme-rail-pane",
            div { class: "lpme-rail-head", "wiring order" }
            if object_count == 0 {
                div { class: "lpme-rail-empty", "no objects yet" }
            } else {
                div { class: "lpme-rail-list",
                    for (index, name, span, selected) in rows {
                        div {
                            key: "{index}",
                            class: if selected { "lpme-rail-row lpme-rail-row-sel" } else { "lpme-rail-row" },
                            onclick: move |evt| {
                                {
                                    let mut s = session.write();
                                    if evt.data().modifiers().shift() {
                                        s.selection.toggle(index);
                                    } else {
                                        s.selection.select_only(index);
                                    }
                                }
                                // Plain click also brings the object into
                                // view; additive shift-clicks don't yank
                                // the camera mid-multi-select.
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
                            span { class: "lpme-rail-name", "{name}" }
                            if let Some((start, count)) = span {
                                span { class: "lpme-rail-range", "{universe_range_label(start, count)}" }
                            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_annotate_universes_and_split_at_boundaries() {
        assert_eq!(universe_range_label(0, 23), "1:1-23");
        assert_eq!(universe_range_label(23, 25), "1:24-48");
        // fyeah p7: lamps 148..=177 global → crosses into universe 2.
        assert_eq!(universe_range_label(147, 30), "1:148-170 2:1-7");
        assert_eq!(universe_range_label(170, 50), "2:1-50");
    }
}
