//! The inspector's Debug switches (lean Q82): which presentation of a
//! node's start against its flow, and whether the patcher's own steps show
//! under it. View state: this browser remembers them, never the project,
//! and neither is an offer. Any host's Debug menu mounts
//! [`InspectorDebug`]; [`use_remembered_inspector_debug`] hands the
//! remembered choice to the editor once, at start.

use dioxus::prelude::*;
use lab_mapping_model::{GraphInput, Presentation};

use crate::graph_editor::GraphEditor;

/// Where this browser remembers the presentation; never the project.
const PRESENTATION_KEY: &str = "lab-mapping.presentation";
/// …and whether the cursor ops show.
const CURSOR_OPS_KEY: &str = "lab-mapping.cursor-ops";

/// The two switches, as rows of a Debug menu.
#[component]
pub fn InspectorDebug(ed: GraphEditor, presentation: Presentation, cursor_ops: bool) -> Element {
    let ops = ed.clone();
    rsx! {
        div { class: "debug-row",
            span { class: "debug-label", "Inspector" }
            for p in Presentation::ALL {
                {
                    let ed = ed.clone();
                    rsx! {
                        button {
                            key: "{p.word()}",
                            class: if presentation == p { "tool on" } else { "tool" },
                            tabindex: "-1",
                            title: "{p.words()}",
                            onmousedown: move |evt| evt.prevent_default(),
                            onclick: move |_| {
                                remember(PRESENTATION_KEY, p.word());
                                ed.input(GraphInput::Presentation(p));
                            },
                            "{p.word().to_uppercase()}"
                        }
                    }
                }
            }
        }
        button {
            class: if cursor_ops { "tool toggle on" } else { "tool toggle" },
            tabindex: "-1",
            title: "Under the inspector: the patcher's own step for each field (Keep, Set, Add), and where it came from",
            onmousedown: move |evt| evt.prevent_default(),
            onclick: move |_| {
                remember(CURSOR_OPS_KEY, if cursor_ops { "0" } else { "1" });
                ops.input(GraphInput::CursorOps(!cursor_ops));
            },
            span { class: "toggle-box", if cursor_ops { "✓" } else { "" } }
            " Cursor ops"
        }
    }
}

/// Hand this browser's remembered switches to the editor, once.
pub fn use_remembered_inspector_debug(ed: GraphEditor) {
    use_hook(move || {
        if let Some(word) = remembered(PRESENTATION_KEY) {
            ed.input(GraphInput::Presentation(Presentation::from_word(&word)));
        }
        if remembered(CURSOR_OPS_KEY).as_deref() == Some("1") {
            ed.input(GraphInput::CursorOps(true));
        }
    });
}

fn remembered(key: &str) -> Option<String> {
    let storage = web_sys::window()?.local_storage().ok()??;
    storage.get_item(key).ok()?
}

fn remember(key: &str, value: &str) {
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(key, value);
    }
}
