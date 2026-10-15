//! The mapping design lab's page. It forwards input to the model
//! (`lab-mapping-model`) and draws what the model says — no editor logic
//! lives here.

mod canvas;
mod input;
mod panels;

use dioxus::prelude::*;
use lab_mapping_model::{Editor, Project, Tool};

const LAB_CSS: &str = include_str!("lab.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut ed = use_signal(|| Editor::new(Project::seed()));
    let tool = ed.read().tool;
    let move_draw = ed.read().move_draw;
    let (can_undo, can_redo) = {
        let e = ed.read();
        (e.can_undo(), e.can_redo())
    };

    rsx! {
        style { {LAB_CSS} }
        div {
            class: "lab",
            tabindex: "0",
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: move |evt| input::key_down(&mut ed, evt),
            onkeyup: move |evt| input::key_up(&mut ed, evt),
            header { class: "top",
                span { class: "brand", "Mapping lab" }
                label { class: "fixture-picker",
                    "Project "
                    select { tabindex: "-1", option { "desk" } }
                }
                div { class: "tools",
                    // Named for what's ON (D50): moving, drawing, reshaping.
                    button {
                        class: if move_draw { "tool toggle on" } else { "tool toggle" },
                        tabindex: "-1",
                        title: if move_draw { "On: drag moves and pins, P and O draw. Off: select and patch only." } else { "Off: select and patch only. Turn on to move, draw and reshape." },
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| {
                            let on = ed.read().move_draw;
                            ed.write().set_move_draw(!on);
                        },
                        span { class: "toggle-box", if move_draw { "✓" } else { "" } }
                        " Move & draw "
                        kbd { "M" }
                    }
                }
                div { class: "tools",
                    ToolButton { ed, tool: Tool::Select, key_label: "V", current: tool, enabled: true }
                    ToolButton { ed, tool: Tool::Path, key_label: "P", current: tool, enabled: move_draw }
                    ToolButton { ed, tool: Tool::Circle, key_label: "O", current: tool, enabled: move_draw }
                }
                div { class: "tools",
                    button {
                        tabindex: "-1",
                        disabled: !can_undo,
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| ed.write().undo(),
                        "Undo " kbd { "⌘Z" }
                    }
                    button {
                        tabindex: "-1",
                        disabled: !can_redo,
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| ed.write().redo(),
                        "Redo " kbd { "⇧⌘Z" }
                    }
                }
            }
            aside { class: "tree", panels::TreePane { ed } }
            main { class: "canvas-wrap", canvas::Canvas { ed } }
            aside { class: "inspector", panels::Inspector { ed } }
            footer { class: "hints", panels::HintBar { ed } }
        }
    }
}

#[component]
fn ToolButton(
    ed: Signal<Editor>,
    tool: Tool,
    key_label: &'static str,
    current: Tool,
    enabled: bool,
) -> Element {
    let mut ed = ed;
    let class = if tool == current { "tool on" } else { "tool" };
    rsx! {
        button {
            class,
            tabindex: "-1",
            disabled: !enabled,
            onmousedown: move |evt| evt.prevent_default(),
            onclick: move |_| ed.write().set_tool(tool),
            "{tool.word()} "
            kbd { "{key_label}" }
        }
    }
}
