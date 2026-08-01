//! Identity & commerce section: everything the catalog card and detail view
//! show that isn't the drawing — id, naming, specs, tier, price, links,
//! bridge, notes.

use dioxus::prelude::*;
use lpa_boards::{BoardNote, NoteOs, PurchaseUrl};

use crate::editor_core::editor_doc::EditorDoc;
use crate::view::form_widgets::{
    BridgeSelect, NumField, OptTextField, TextAreaField, TextField, TierSelect,
};

const NOTE_OS_OPTIONS: &[(Option<NoteOs>, &str)] = &[
    (None, "all OSes"),
    (Some(NoteOs::MacOs), "macos"),
    (Some(NoteOs::Windows), "windows"),
    (Some(NoteOs::Linux), "linux"),
];

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn IdentitySection(doc: Signal<EditorDoc>) -> Element {
    let board = doc.read().board.clone();
    let capability_draft = use_signal(String::new);

    rsx! {
        section { class: "lpb-ed-section",
            h2 { "Identity & catalog" }
            div { class: "lpb-ed-grid",
                TextField {
                    label: "board_id",
                    value: board.board_id.clone(),
                    mono: true,
                    placeholder: "vendor/product",
                    on_change: move |value| doc.write().edit(|b| b.board_id = value),
                }
                TextField {
                    label: "display_name",
                    value: board.display_name.clone(),
                    on_change: move |value| doc.write().edit(|b| b.display_name = value),
                }
                TextField {
                    label: "manufacturer",
                    value: board.manufacturer.clone(),
                    on_change: move |value| doc.write().edit(|b| b.manufacturer = value),
                }
                TextField {
                    label: "soc",
                    value: board.soc.clone(),
                    placeholder: "ESP32-C6",
                    on_change: move |value| doc.write().edit(|b| b.soc = value),
                }
                TextField {
                    label: "family",
                    value: board.family.clone(),
                    mono: true,
                    placeholder: "esp32c6",
                    on_change: move |value| doc.write().edit(|b| b.family = value),
                }
                TextField {
                    label: "flash",
                    value: board.flash.clone(),
                    placeholder: "4 MB",
                    on_change: move |value| doc.write().edit(|b| b.flash = value),
                }
                OptTextField {
                    label: "psram",
                    value: board.psram.clone(),
                    placeholder: "8 MB",
                    on_change: move |value| doc.write().edit(|b| b.psram = value),
                }
                NumField {
                    label: "price_usd",
                    value: board.price_usd,
                    step: 0.1,
                    on_change: move |value| doc.write().edit(|b| b.price_usd = value),
                }
                TierSelect {
                    value: board.tier,
                    on_change: move |value| doc.write().edit(|b| b.tier = value),
                }
                BridgeSelect {
                    value: board.usb_bridge,
                    on_change: move |value| doc.write().edit(|b| b.usb_bridge = value),
                }
            }
            TextAreaField {
                label: "blurb",
                value: board.blurb.clone(),
                on_change: move |value| doc.write().edit(|b| b.blurb = value),
            }
            OptTextField {
                label: "support_note",
                value: board.support_note.clone(),
                placeholder: "honest caveat shown with the tier",
                on_change: move |value| doc.write().edit(|b| b.support_note = value),
            }
            CapabilityChips { doc, draft: capability_draft, capabilities: board.capabilities.clone() }
            PurchaseUrlRows { doc, urls: board.purchase_urls.clone() }
            NoteRows { doc, notes: board.notes.clone() }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CapabilityChips(
    doc: Signal<EditorDoc>,
    mut draft: Signal<String>,
    capabilities: Vec<String>,
) -> Element {
    let mut add = move || {
        let text = draft().trim().to_string();
        if !text.is_empty() {
            doc.write().edit(|b| b.capabilities.push(text));
            draft.set(String::new());
        }
    };
    rsx! {
        div { class: "lpb-ed-field lpb-ed-field--wide",
            label { "capabilities" }
            div { class: "lpb-ed-chiprow",
                for (index, capability) in capabilities.iter().enumerate() {
                    span { key: "{index}-{capability}", class: "lpb-ed-chip",
                        "{capability}"
                        button {
                            class: "lpb-ed-chip-x",
                            title: "remove",
                            onclick: move |_| {
                                doc.write().edit(|b| {
                                    b.capabilities.remove(index);
                                });
                            },
                            "×"
                        }
                    }
                }
                input {
                    r#type: "text",
                    class: "lpb-ed-input lpb-ed-chip-add",
                    placeholder: "add… (Enter)",
                    value: "{draft}",
                    oninput: move |event| draft.set(event.value()),
                    onkeydown: move |event| {
                        if event.key() == Key::Enter {
                            add();
                        }
                    },
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PurchaseUrlRows(doc: Signal<EditorDoc>, urls: Vec<PurchaseUrl>) -> Element {
    rsx! {
        div { class: "lpb-ed-field lpb-ed-field--wide",
            label { "purchase_urls" }
            div { class: "lpb-ed-rows",
                for (index, url) in urls.iter().enumerate() {
                    div { key: "{index}", class: "lpb-ed-row",
                        input {
                            r#type: "text",
                            class: "lpb-ed-input lpb-ed-row-label",
                            placeholder: "label",
                            value: "{url.label}",
                            oninput: move |event| {
                                doc.write().edit(|b| {
                                    if let Some(entry) = b.purchase_urls.get_mut(index) {
                                        entry.label = event.value();
                                    }
                                });
                            },
                        }
                        input {
                            r#type: "text",
                            class: "lpb-ed-input lpb-ed-input--mono lpb-ed-row-grow",
                            placeholder: "https://…",
                            value: "{url.href}",
                            oninput: move |event| {
                                doc.write().edit(|b| {
                                    if let Some(entry) = b.purchase_urls.get_mut(index) {
                                        entry.href = event.value();
                                    }
                                });
                            },
                        }
                        button {
                            class: "lpb-ed-chip-x",
                            title: "remove link",
                            onclick: move |_| {
                                doc.write().edit(|b| {
                                    if index < b.purchase_urls.len() {
                                        b.purchase_urls.remove(index);
                                    }
                                });
                            },
                            "×"
                        }
                    }
                }
                button {
                    class: "lpb-ed-btn lpb-ed-btn--add",
                    onclick: move |_| {
                        doc.write().edit(|b| {
                            b.purchase_urls.push(PurchaseUrl {
                                label: String::new(),
                                href: "https://".into(),
                            });
                        });
                    },
                    "+ link"
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NoteRows(doc: Signal<EditorDoc>, notes: Vec<BoardNote>) -> Element {
    rsx! {
        div { class: "lpb-ed-field lpb-ed-field--wide",
            label { "notes" }
            div { class: "lpb-ed-rows",
                for (index, note) in notes.iter().enumerate() {
                    div { key: "{index}", class: "lpb-ed-row",
                        input {
                            r#type: "text",
                            class: "lpb-ed-input lpb-ed-row-grow",
                            placeholder: "note text",
                            value: "{note.text}",
                            oninput: move |event| {
                                doc.write().edit(|b| {
                                    if let Some(entry) = b.notes.get_mut(index) {
                                        entry.text = event.value();
                                    }
                                });
                            },
                        }
                        select {
                            class: "lpb-ed-select",
                            onchange: move |event| {
                                let picked = NOTE_OS_OPTIONS
                                    .iter()
                                    .find(|(_, name)| *name == event.value())
                                    .map(|(os, _)| *os)
                                    .unwrap_or(None);
                                doc.write().edit(|b| {
                                    if let Some(entry) = b.notes.get_mut(index) {
                                        entry.os = picked;
                                        if picked.is_none() {
                                            entry.os_version = None;
                                        }
                                    }
                                });
                            },
                            for (os, name) in NOTE_OS_OPTIONS {
                                option { value: "{name}", selected: *os == note.os, "{name}" }
                            }
                        }
                        if note.os.is_some() {
                            input {
                                r#type: "text",
                                class: "lpb-ed-input lpb-ed-row-label",
                                placeholder: "version (Sequoia+)",
                                value: note.os_version.clone().unwrap_or_default(),
                                oninput: move |event| {
                                    let text = event.value();
                                    doc.write().edit(|b| {
                                        if let Some(entry) = b.notes.get_mut(index) {
                                            entry.os_version =
                                                (!text.trim().is_empty()).then_some(text);
                                        }
                                    });
                                },
                            }
                        }
                        button {
                            class: "lpb-ed-chip-x",
                            title: "remove note",
                            onclick: move |_| {
                                doc.write().edit(|b| {
                                    if index < b.notes.len() {
                                        b.notes.remove(index);
                                    }
                                });
                            },
                            "×"
                        }
                    }
                }
                button {
                    class: "lpb-ed-btn lpb-ed-btn--add",
                    onclick: move |_| {
                        doc.write().edit(|b| {
                            b.notes.push(BoardNote {
                                text: String::new(),
                                os: None,
                                os_version: None,
                            });
                        });
                    },
                    "+ note"
                }
            }
        }
    }
}
