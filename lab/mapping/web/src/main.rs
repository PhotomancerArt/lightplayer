//! The mapping design lab's page. It forwards input to the model
//! (`lab-mapping-model`), lays the nodes out (`lab-layout`), and draws the
//! view tree the model emits (`Editor::view_tree`) — no editor logic and
//! no detail decisions live here.
//!
//! The project picker opens the lab's own desk, or one of the real
//! projects baked in by `lab-mapping-files` (the PLAYFUL choker, the zook
//! dome, the radiance dome), and fits the view to it.

mod canvas_container;
mod dense_canvas;
mod dots_layer;
mod edge_layer;
mod faces;
mod input;
mod node_card;
mod overlay;
mod panels;
mod text_measure;
mod view;
mod world_layer;

use dioxus::prelude::*;
use lab_mapping_files::{Opened, embedded};
use lab_mapping_model::{Editor, LayoutKind, Notice, Project, SizeClass, Tool};

const LAB_CSS: &str = include_str!("lab.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut ed = use_signal(|| Editor::new(Project::seed()));
    let mut project = use_signal(|| DESK.to_string());
    let view = view::use_view(ed);
    let toolbar = view.toolbar.read().clone();
    let tool = toolbar.tool;
    let move_draw = toolbar.move_draw;
    let (can_undo, can_redo) = (toolbar.can_undo, toolbar.can_redo);
    let class = toolbar.size_class;
    // Page-local view state: the md tree drawer, the sm sheet.
    let mut tree_open = use_signal(|| false);
    let mut sheet = use_signal(|| Sheet::Half);
    let has_selection = use_memo(move || !ed.read().selection.is_empty());
    // The layout you chose last time, in this browser.
    use_hook(move || {
        if let Some(kind) = remembered_layout() {
            ed.write().set_layout(Some(kind));
        }
    });
    let lab_class = format!(
        "lab size-{}{}",
        class.word(),
        if toolbar.forced_size_class.is_some() {
            " forced"
        } else {
            ""
        }
    );

    rsx! {
        style { {LAB_CSS} }
        div {
            class: "{lab_class}",
            tabindex: "0",
            // The size class, from the whole app's width: the chrome follows
            // it, and the canvas's own width moves with the chrome.
            onresize: move |evt| {
                if let Ok(size) = evt.get_content_box_size() {
                    ed.write().report_size_class(SizeClass::of_width(size.width));
                }
            },
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
                div { class: "tools layout-switch",
                    for (kind, word) in [(LayoutKind::Canvas, "Canvas"), (LayoutKind::Stack, "Stack")] {
                        button {
                            key: "{word}",
                            class: if toolbar.layout == kind { "tool on" } else { "tool" },
                            tabindex: "-1",
                            title: "The layout: a canvas you pan and zoom, or one column in tree order",
                            onmousedown: move |evt| evt.prevent_default(),
                            onclick: move |_| {
                                ed.write().set_layout(Some(kind));
                                remember_layout(kind);
                            },
                            "{word}"
                        }
                    }
                }
                if class == SizeClass::Md {
                    button {
                        class: if tree_open() { "tool on" } else { "tool" },
                        tabindex: "-1",
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| tree_open.toggle(),
                        "Tree"
                    }
                }
                DebugMenu { ed, pinning: toolbar.pinning, forced: toolbar.forced_size_class }
            }
            match class {
                SizeClass::Lg => rsx! {
                    aside { class: "tree", panels::TreePane { ed, tree: view.tree } }
                    main { class: "canvas-wrap", canvas_container::Canvas { ed, view } }
                    aside { class: "inspector", panels::Inspector { ed, inspector: view.inspector } }
                },
                SizeClass::Md => rsx! {
                    main { class: "canvas-wrap",
                        canvas_container::Canvas { ed, view }
                        if tree_open() {
                            aside { class: "drawer drawer-left tree", panels::TreePane { ed, tree: view.tree } }
                        }
                        if has_selection() {
                            aside { class: "drawer drawer-right inspector", panels::Inspector { ed, inspector: view.inspector } }
                        }
                    }
                },
                SizeClass::Sm => rsx! {
                    main { class: "canvas-wrap",
                        canvas_container::Canvas { ed, view }
                        if has_selection() {
                            div { class: "sheet sheet-{sheet().word()}",
                                button {
                                    class: "sheet-handle",
                                    tabindex: "-1",
                                    title: "Peek, half, full",
                                    onmousedown: move |evt| evt.prevent_default(),
                                    onclick: move |_| sheet.set(sheet().next()),
                                    span { class: "sheet-grip" }
                                }
                                div { class: "sheet-body inspector",
                                    panels::Inspector { ed, inspector: view.inspector }
                                }
                            }
                        }
                    }
                },
            }
            footer { class: if class == SizeClass::Sm { "hints compact" } else { "hints" },
                panels::HintBar { hints: view.hints }
            }
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
            let mut e = Editor::new(project).with_view_of(&ed.peek());
            e.set_placement(&lab_layout::canvas_layout(&e.layout_input()));
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

/// The bottom sheet's heights on a phone: its handle steps through them.
#[derive(Clone, Copy, PartialEq)]
enum Sheet {
    Peek,
    Half,
    Full,
}

impl Sheet {
    fn next(self) -> Sheet {
        match self {
            Sheet::Peek => Sheet::Half,
            Sheet::Half => Sheet::Full,
            Sheet::Full => Sheet::Peek,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Sheet::Peek => "peek",
            Sheet::Half => "half",
            Sheet::Full => "full",
        }
    }
}

/// Where this browser remembers the layout you chose; never the project.
const LAYOUT_KEY: &str = "lab-mapping.layout";

fn remembered_layout() -> Option<LayoutKind> {
    let storage = web_sys::window()?.local_storage().ok()??;
    match storage.get_item(LAYOUT_KEY).ok()??.as_str() {
        "canvas" => Some(LayoutKind::Canvas),
        "stack" => Some(LayoutKind::Stack),
        _ => None,
    }
}

fn remember_layout(kind: LayoutKind) {
    let word = match kind {
        LayoutKind::Canvas => "canvas",
        LayoutKind::Stack => "stack",
    };
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(LAYOUT_KEY, word);
    }
}

/// Debug, at the toolbar's right end: switches for things being tried,
/// out of the way (D64). Open or shut is the page's own state.
#[component]
fn DebugMenu(ed: Signal<Editor>, pinning: bool, forced: Option<SizeClass>) -> Element {
    let mut ed = ed;
    let mut open = use_signal(|| false);
    rsx! {
        div { class: "debug",
            button {
                class: if open() { "tool on" } else { "tool" },
                tabindex: "-1",
                onmousedown: move |evt| evt.prevent_default(),
                onclick: move |_| open.toggle(),
                "Debug ▾"
            }
            if open() {
                div { class: "debug-menu",
                    button {
                        class: if pinning { "tool toggle on" } else { "tool toggle" },
                        tabindex: "-1",
                        title: "On: dragging a node pins it, U unpins it. Off: the layout places every node.",
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| ed.write().set_pinning(!pinning),
                        span { class: "toggle-box", if pinning { "✓" } else { "" } }
                        " Pinning"
                    }
                    div { class: "debug-row",
                        span { class: "debug-label", "Size" }
                        for (class, word) in [(None, "Auto"), (Some(SizeClass::Sm), "sm"), (Some(SizeClass::Md), "md"), (Some(SizeClass::Lg), "lg")] {
                            button {
                                key: "{word}",
                                class: if forced == class { "tool on" } else { "tool" },
                                tabindex: "-1",
                                title: "Draw as if the screen were this size",
                                onmousedown: move |evt| evt.prevent_default(),
                                onclick: move |_| ed.write().force_size_class(class),
                                "{word}"
                            }
                        }
                    }
                }
            }
        }
    }
}
