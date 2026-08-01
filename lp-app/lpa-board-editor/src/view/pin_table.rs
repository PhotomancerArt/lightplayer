//! The pin table editor: one component serving both header rails and the
//! screw-terminal band through [`RailTarget`] — label, role, gpio, capability
//! chips, reorder within the rail.

use dioxus::prelude::*;
use lpa_boards::{CapKind, PinCap};

use crate::editor_core::editor_doc::{EditorDoc, RailTarget};
use crate::view::form_widgets::{CapKindSelect, OptGpioField, RoleSelect};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PinRailEditor(doc: Signal<EditorDoc>, target: RailTarget) -> Element {
    let rows = doc.read().rail_rows(target);
    let count = rows.len();

    rsx! {
        section { class: "lpb-ed-section",
            div { class: "lpb-ed-section-head",
                h2 { "{target.title()}" }
                span { class: "lpb-ed-count", "{count}" }
                button {
                    class: "lpb-ed-btn lpb-ed-btn--add",
                    onclick: move |_| doc.write().add_pin(target),
                    "+ pin"
                }
            }
            div { class: "lpb-ed-pins",
                for (index, row) in rows.iter().enumerate() {
                    div { key: "{index}", class: "lpb-ed-pin",
                        div { class: "lpb-ed-pin-order",
                            button {
                                class: "lpb-ed-order-btn",
                                disabled: index == 0,
                                title: "move up",
                                onclick: move |_| doc.write().move_pin(target, index, -1),
                                "▲"
                            }
                            button {
                                class: "lpb-ed-order-btn",
                                disabled: index + 1 == count,
                                title: "move down",
                                onclick: move |_| doc.write().move_pin(target, index, 1),
                                "▼"
                            }
                        }
                        input {
                            r#type: "text",
                            class: "lpb-ed-input lpb-ed-input--mono lpb-ed-pin-label",
                            placeholder: "label",
                            value: "{row.label}",
                            oninput: move |event| {
                                doc.write().edit_pin(target, index, |fields| {
                                    *fields.label = event.value();
                                });
                            },
                        }
                        RoleSelect {
                            value: row.role,
                            on_change: move |role| {
                                doc.write().edit_pin(target, index, |fields| *fields.role = role);
                            },
                        }
                        OptGpioField {
                            value: row.gpio,
                            on_change: move |gpio| {
                                doc.write().edit_pin(target, index, |fields| *fields.gpio = gpio);
                            },
                        }
                        PinCapsEditor { doc, target, index, caps: row.caps.clone() }
                        button {
                            class: "lpb-ed-chip-x",
                            title: "remove pin",
                            onclick: move |_| doc.write().remove_pin(target, index),
                            "×"
                        }
                    }
                }
            }
        }
    }
}

/// One pin's capability cells: typed chips plus an inline add row.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PinCapsEditor(
    doc: Signal<EditorDoc>,
    target: RailTarget,
    index: usize,
    caps: Vec<PinCap>,
) -> Element {
    let mut draft_text = use_signal(String::new);
    let mut draft_kind = use_signal(|| CapKind::Note);
    let mut add = move || {
        let text = draft_text().trim().to_string();
        if !text.is_empty() {
            let kind = draft_kind();
            doc.write().edit_pin(target, index, |fields| {
                fields.caps.push(PinCap { text, kind });
            });
            draft_text.set(String::new());
        }
    };

    rsx! {
        div { class: "lpb-ed-caps",
            for (cap_index, cap) in caps.iter().enumerate() {
                span {
                    key: "{cap_index}",
                    class: "lpb-ed-chip lpb-ed-chip--{cap_kind_name(cap.kind)}",
                    title: "{cap_kind_name(cap.kind)}",
                    "{cap.text}"
                    button {
                        class: "lpb-ed-chip-x",
                        title: "remove capability",
                        onclick: move |_| {
                            doc.write().edit_pin(target, index, |fields| {
                                if cap_index < fields.caps.len() {
                                    fields.caps.remove(cap_index);
                                }
                            });
                        },
                        "×"
                    }
                }
            }
            span { class: "lpb-ed-cap-add",
                input {
                    r#type: "text",
                    class: "lpb-ed-input lpb-ed-cap-draft",
                    placeholder: "+ cap (Enter)",
                    value: "{draft_text}",
                    oninput: move |event| draft_text.set(event.value()),
                    onkeydown: move |event| {
                        if event.key() == Key::Enter {
                            add();
                        }
                    },
                }
                CapKindSelect {
                    value: draft_kind(),
                    on_change: move |kind| draft_kind.set(kind),
                }
            }
        }
    }
}

fn cap_kind_name(kind: CapKind) -> &'static str {
    crate::view::form_widgets::CAP_KINDS
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map(|(_, name)| *name)
        .unwrap_or("note")
}
