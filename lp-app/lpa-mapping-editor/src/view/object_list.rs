//! The object rail: wiring order as a collapsible left overlay — swatch,
//! name, lamp range, ▲▼ reorder (chain order IS the wiring order).

use dioxus::prelude::*;
use dioxus_icons::lucide::List;
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
pub fn ObjectList(session: Signal<MapEditorSession>, on_committed: EventHandler<()>) -> Element {
    let mut open = use_signal(|| true);
    let session_read = session.read();
    let doc = session_read.doc();
    let spans = resolve(doc)
        .map(|resolved| resolved.spans)
        .unwrap_or_default();
    let rows: Vec<(usize, String, Option<(u32, u32)>, bool)> = doc
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| {
            (
                index,
                object.name.clone(),
                spans.get(index).map(|span| (span.start, span.count)),
                session_read.selection.objects.contains(&index),
            )
        })
        .collect();
    let object_count = rows.len();
    drop(session_read);

    rsx! {
        div { class: "lpme-rail",
            button {
                class: if open() { "lpme-btn lpme-btn-on lpme-rail-toggle" } else { "lpme-btn lpme-rail-toggle" },
                title: "wiring order",
                onclick: move |_| {
                    let now = *open.peek();
                    open.set(!now);
                },
                List { size: 13 }
            }
            if open() && object_count > 0 {
                div { class: "lpme-rail-list",
                    for (index, name, span, selected) in rows {
                        div {
                            key: "{index}",
                            class: if selected { "lpme-rail-row lpme-rail-row-sel" } else { "lpme-rail-row" },
                            onclick: move |evt| {
                                let mut s = session.write();
                                if evt.data().modifiers().shift() {
                                    s.selection.toggle(index);
                                } else {
                                    s.selection.select_only(index);
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
