//! The mapping design lab's page. It forwards input to the model
//! (`lab-mapping-model`) and draws what the model says — no editor logic
//! lives here.
//!
//! The project picker opens the lab's own desk, or one of the real
//! projects baked in by `lab-mapping-files` (the PLAYFUL choker, the zook
//! dome, the radiance dome), and fits the view to it.

mod canvas;
mod input;
mod panels;
mod text_measure;

use dioxus::prelude::*;
use lab_mapping_files::{Opened, embedded};
use lab_mapping_model::{Editor, Notice, Project, Tool};

const LAB_CSS: &str = include_str!("lab.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut ed = use_signal(|| Editor::new(Project::seed()));
    let mut project = use_signal(|| DESK.to_string());
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
                    select {
                        tabindex: "-1",
                        value: "{project}",
                        onchange: move |evt| {
                            let key = evt.value();
                            open_project(&mut ed, &key);
                            project.set(key);
                        },
                        option { value: DESK, "desk" }
                        for p in embedded::ALL {
                            option { value: p.key, "{p.title}" }
                        }
                    }
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

/// The picker's key for the lab's own starting project.
const DESK: &str = "desk";

/// Open a project in the editor and fit the view to it. A project that
/// won't open leaves the current one and says why.
fn open_project(ed: &mut Signal<Editor>, key: &str) {
    let project = if key == DESK {
        Ok(Project::seed())
    } else {
        match embedded::ALL.iter().find(|p| p.key == key) {
            Some(p) => Opened::open(&p.files())
                .map(|o| o.project)
                .map_err(|e| e.to_string()),
            None => Err(format!("no project called {key}")),
        }
    };
    match project {
        Ok(project) => {
            let viewport = ed.peek().viewport;
            let mut e = Editor::new(project);
            e.resize(viewport);
            e.fit();
            ed.set(e);
        }
        Err(why) => {
            ed.write().notice = Some(Notice::refused(
                format!("Couldn't open {key}"),
                why,
                "pick another project; this one is unchanged",
            ));
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
