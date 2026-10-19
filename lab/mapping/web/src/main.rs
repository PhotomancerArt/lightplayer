//! The mapping design lab's page: the library's canvas and panels
//! (`lib.rs`) around the lab's own editor, with the lab's toolbar, its
//! project picker and its simulated board. It owns the editor outright, so
//! every input the components say is applied to it at once.
//!
//! The project picker opens the lab's own desk, or one of the real
//! projects baked in by `lab-mapping-files` (the PLAYFUL choker, the zook
//! dome, the radiance dome), and fits the view to it.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use lab_mapping_files::{Opened, embedded};
use lab_mapping_model::{Editor, GraphInput, LayoutKind, Notice, Project, SizeClass, Tool};
use lab_mapping_web::{
    Canvas, GRAPH_CSS, GraphEditor, HintBar, Inspector, TreePane, input, use_simulated_frame,
    use_view,
};

const LAB_CSS: &str = include_str!("lab.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let tick = use_signal(|| 0_u64);
    let ed =
        use_hook(|| GraphEditor::owned(Rc::new(RefCell::new(Editor::new(Project::seed()))), tick));
    let mut project = use_signal(|| DESK.to_string());
    let view = use_view(ed.clone());
    let frame = use_simulated_frame(ed.clone());
    let toolbar = view.toolbar.read().clone();
    let tool = toolbar.tool;
    let move_draw = toolbar.move_draw;
    let (can_undo, can_redo) = (toolbar.can_undo, toolbar.can_redo);
    let class = toolbar.size_class;
    // Page-local view state: the md tree drawer, the sm sheet.
    let mut tree_open = use_signal(|| false);
    let mut sheet = use_signal(|| Sheet::Half);
    let has_selection = use_memo({
        let ed = ed.clone();
        move || !ed.read().selection.is_empty()
    });
    // The layout you chose last time, in this browser.
    use_hook({
        let ed = ed.clone();
        move || {
            if let Some(kind) = remembered_layout() {
                ed.input(GraphInput::SetLayout(Some(kind)));
            }
        }
    });
    let e = || ed.clone();
    let (on_resize, on_down, on_up, on_pick) = (e(), e(), e(), e());
    let lab_class = format!(
        "lab graph size-{}{}",
        class.word(),
        if toolbar.forced_size_class.is_some() {
            " forced"
        } else {
            ""
        }
    );

    rsx! {
        style { {LAB_CSS} }
        style { {GRAPH_CSS} }
        div {
            class: "{lab_class}",
            tabindex: "0",
            // The size class, from the window's width: the chrome follows
            // it, and the canvas's own width moves with the chrome. Not this
            // box's width: a size forced in Debug frames it narrower.
            onresize: move |evt| {
                if let Ok(size) = evt.get_content_box_size() {
                    let width = web_sys::window()
                        .and_then(|w| w.inner_width().ok()?.as_f64())
                        .unwrap_or(size.width);
                    on_resize.input(GraphInput::SizeClass(SizeClass::of_width(width)));
                }
            },
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: move |evt| input::key_down(&on_down, evt),
            onkeyup: move |evt| input::key_up(&on_up, evt),
            header { class: "top",
                span { class: "brand", "Mapping lab" }
                label { class: "fixture-picker",
                    "Project "
                    select {
                        tabindex: "-1",
                        value: "{project}",
                        onchange: move |evt| {
                            let key = evt.value();
                            open_project(&on_pick, &key);
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
                        onclick: {
                            let ed = e();
                            move |_| {
                                let on = ed.peek().move_draw;
                                ed.input(GraphInput::MoveDraw(!on));
                            }
                        },
                        span { class: "toggle-box", if move_draw { "✓" } else { "" } }
                        " Move & draw "
                        kbd { "D" }
                    }
                }
                div { class: "tools",
                    ToolButton { ed: e(), tool: Tool::Select, key_label: "V", current: tool, enabled: true }
                    ToolButton { ed: e(), tool: Tool::Path, key_label: "P", current: tool, enabled: move_draw }
                    ToolButton { ed: e(), tool: Tool::Circle, key_label: "O", current: tool, enabled: move_draw }
                }
                div { class: "tools",
                    button {
                        tabindex: "-1",
                        disabled: !can_undo,
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: {
                            let ed = e();
                            move |_| {
                                ed.input(GraphInput::Undo);
                            }
                        },
                        "Undo " kbd { "⌘Z" }
                    }
                    button {
                        tabindex: "-1",
                        disabled: !can_redo,
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: {
                            let ed = e();
                            move |_| {
                                ed.input(GraphInput::Redo);
                            }
                        },
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
                            onclick: {
                                let ed = e();
                                move |_| {
                                    ed.input(GraphInput::SetLayout(Some(kind)));
                                    remember_layout(kind);
                                }
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
                DebugMenu { ed: e(), pinning: toolbar.pinning, forced: toolbar.forced_size_class }
            }
            match class {
                SizeClass::Lg => rsx! {
                    aside { class: "tree", TreePane { ed: e(), tree: view.tree } }
                    main { class: "canvas-wrap", Canvas { ed: e(), view, frame } }
                    aside { class: "inspector", Inspector { ed: e(), inspector: view.inspector } }
                },
                SizeClass::Md => rsx! {
                    main { class: "canvas-wrap",
                        Canvas { ed: e(), view, frame }
                        if tree_open() {
                            aside { class: "drawer drawer-left tree", TreePane { ed: e(), tree: view.tree } }
                        }
                        if has_selection() {
                            aside { class: "drawer drawer-right inspector", Inspector { ed: e(), inspector: view.inspector } }
                        }
                    }
                },
                SizeClass::Sm => rsx! {
                    main { class: "canvas-wrap",
                        Canvas { ed: e(), view, frame }
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
                                    Inspector { ed: e(), inspector: view.inspector }
                                }
                            }
                        }
                    }
                },
            }
            footer { class: if class == SizeClass::Sm { "hints compact" } else { "hints" },
                HintBar { hints: view.hints }
            }
        }
    }
}

/// The picker's key for the lab's own starting project.
const DESK: &str = "desk";

/// Open a project in the editor and fit the view to it. A project that
/// won't open leaves the current one and says why.
fn open_project(ed: &GraphEditor, key: &str) {
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
            *ed.editor().borrow_mut() = e;
        }
        Err(why) => {
            ed.editor().borrow_mut().notice = Some(Notice::refused(
                format!("Couldn't open {key}"),
                why,
                "pick another project; this one is unchanged",
            ));
        }
    }
    ed.changed();
}

#[component]
fn ToolButton(
    ed: GraphEditor,
    tool: Tool,
    key_label: &'static str,
    current: Tool,
    enabled: bool,
) -> Element {
    let class = if tool == current { "tool on" } else { "tool" };
    rsx! {
        button {
            class,
            tabindex: "-1",
            disabled: !enabled,
            onmousedown: move |evt| evt.prevent_default(),
            onclick: move |_| {
                ed.input(GraphInput::Tool(tool));
            },
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
fn DebugMenu(ed: GraphEditor, pinning: bool, forced: Option<SizeClass>) -> Element {
    let pin = ed.clone();
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
                        onclick: move |_| {
                            pin.input(GraphInput::Pinning(!pinning));
                        },
                        span { class: "toggle-box", if pinning { "✓" } else { "" } }
                        " Pinning"
                    }
                    div { class: "debug-row",
                        span { class: "debug-label", "Size" }
                        for (class, word) in [(None, "Auto"), (Some(SizeClass::Sm), "sm"), (Some(SizeClass::Md), "md"), (Some(SizeClass::Lg), "lg")] {
                            {
                                let ed = ed.clone();
                                rsx! {
                                    button {
                                        key: "{word}",
                                        class: if forced == class { "tool on" } else { "tool" },
                                        tabindex: "-1",
                                        title: "Draw as if the screen were this size",
                                        onmousedown: move |evt| evt.prevent_default(),
                                        onclick: move |_| {
                                            ed.input(GraphInput::ForceSizeClass(class));
                                        },
                                        "{word}"
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
