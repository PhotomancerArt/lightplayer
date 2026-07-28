//! The object rail: wiring order as a collapsible left overlay — swatch,
//! name, lamp range, ▲▼ reorder (chain order IS the wiring order).

use dioxus::prelude::*;
use dioxus_icons::lucide::List;
use lpc_mapping::resolve;

use crate::editor_core::editor_session::MapEditorSession;
use crate::view::editor_canvas::object_color;

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
                                span { class: "lpme-rail-range", "{start + 1}–{start + count}" }
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
