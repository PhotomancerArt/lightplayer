//! The Graph view: the mapping lab's canvas on the open project
//! (`lab-mapping-web`'s components, over core's editor).
//!
//! Core owns the editor (`lpa_studio_core::app::graph`) and shares it
//! through a [`GraphHandle`] the app provides as context. This page wraps
//! that handle as the lab's [`GraphEditor`]: the components read the
//! editor through it and hand every pointer, key and layout fact to core
//! through it, synchronously. What core changes on its own (it opened the
//! project, or rebuilt it for an edit made elsewhere) arrives as the
//! Graph's `generation` on the studio view, and the page draws again.
//!
//! The center is the canvas, its toolbar and the hint bar; the workbench's
//! own docks carry the lab's tree (left) and inspector (Props, right). The
//! colours are the board's own frames (`UiGraphView::frame`), except while
//! a drawing is in progress: nothing of it has reached the board yet, so
//! its lamps light from the lab's simulated board.

use std::rc::Rc;

use dioxus::prelude::*;
use lab_mapping_model::{BoardFrame, GraphInput, LayoutKind, SizeClass};
use lab_mapping_web::{
    Canvas, GRAPH_CSS, GraphEditor, HintBar, Inspector, TreePane, View, use_view,
};
use lpa_studio_core::app::graph::{GraphHandle, UiGraphState, UiGraphView};

/// The Graph's editor, its view parts and the board's frame: built once
/// by the workbench, shared by the center and the docks.
#[derive(Clone, PartialEq)]
pub struct GraphPage {
    pub ed: GraphEditor,
    pub view: View,
    pub frame: Memo<BoardFrame>,
}

/// The Graph's page state, over the app's [`GraphHandle`] (a fresh, empty
/// one where none is provided: stories that don't show the Graph).
pub fn use_graph_page(graph: Option<UiGraphView>) -> GraphPage {
    let tick = use_signal(|| 0_u64);
    let ed = use_hook(|| {
        let handle = try_consume_context::<GraphHandle>().unwrap_or_default();
        let sink = handle.clone();
        GraphEditor::new(
            handle.editor().clone(),
            tick,
            Rc::new(move |input| sink.input(input)),
        )
    });
    // Core changed the editor itself: draw again.
    let generation = graph.as_ref().map(|g| g.generation);
    {
        let ed = ed.clone();
        use_effect(use_reactive!(|generation| {
            let _ = generation;
            ed.changed();
        }));
    }
    // The board's frame, as core last published it.
    let mut published = use_signal(|| None::<lpa_studio_core::app::graph::UiGraphFrame>);
    let latest = graph.map(|g| g.frame);
    use_effect(use_reactive!(|latest| published.set(latest)));
    let view = use_view(ed.clone());
    let drawing = use_memo({
        let ed = ed.clone();
        move || ed.read().drawing()
    });
    let frame = use_memo({
        let ed = ed.clone();
        move || {
            // A drawing in progress hasn't reached the board: light it
            // from the simulated board, as the lab does — again as the
            // draft moves, and only then.
            if drawing() {
                let _ = ed.read();
                return ed.peek().board_frame(0.0);
            }
            published
                .read()
                .as_ref()
                .map(|f| (*f.0).clone())
                .unwrap_or_default()
        }
    });
    GraphPage { ed, view, frame }
}

/// The Graph's center: its toolbar, the canvas, and the hint bar.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn GraphCenter(page: GraphPage, state: Option<UiGraphState>) -> Element {
    let body = match state {
        None | Some(UiGraphState::Opening) => rsx! {
            div { class: "graph-note", "Opening the project on the Graph…" }
        },
        Some(UiGraphState::Refused { why }) => rsx! {
            div { class: "graph-note",
                div { class: "graph-note-title", "The Graph can't open this project yet" }
                div { "{why}" }
                div { class: "graph-note-help", "Map and Patch still open it." }
            }
        },
        Some(UiGraphState::Open { live }) => rsx! {
            GraphCanvas { page: page.clone(), live }
        },
    };
    rsx! {
        style { {GRAPH_CSS} }
        div { class: "graph graph-center tw:flex tw:min-h-0 tw:flex-1 tw:flex-col", {body} }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn GraphCanvas(page: GraphPage, live: bool) -> Element {
    let GraphPage { ed, view, frame } = page;
    let layout = view.toolbar.read().layout;
    let (on_resize, on_down, on_up) = (ed.clone(), ed.clone(), ed.clone());
    rsx! {
        div {
            class: "graph-keys tw:flex tw:min-h-0 tw:flex-1 tw:flex-col tw:outline-none",
            tabindex: "0",
            // The size class, from the window's width, as in the lab.
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
            // A press on the canvas keeps its keys here (the canvas
            // stops the browser moving focus).
            onmousedown: move |_| focus_keys(),
            onkeydown: move |evt| lab_mapping_web::input::key_down(&on_down, evt),
            onkeyup: move |evt| lab_mapping_web::input::key_up(&on_up, evt),
            div { class: "graph-bar",
                div { class: "tools layout-switch",
                    for (kind, word) in [(LayoutKind::Canvas, "Canvas"), (LayoutKind::Stack, "Stack")] {
                        {
                            let ed = ed.clone();
                            rsx! {
                                button {
                                    key: "{word}",
                                    class: if layout == kind { "tool on" } else { "tool" },
                                    tabindex: "-1",
                                    title: "The layout: a canvas you pan and zoom, or one column in tree order",
                                    onmousedown: move |evt| evt.prevent_default(),
                                    onclick: move |_| {
                                        ed.input(GraphInput::SetLayout(Some(kind)));
                                    },
                                    "{word}"
                                }
                            }
                        }
                    }
                }
                span { class: "graph-live",
                    if live { "Colours from the board" } else { "Waiting for the board's first frame" }
                }
            }
            div { class: "canvas-wrap tw:relative tw:min-h-0 tw:flex-1",
                Canvas { ed: ed.clone(), view, frame }
            }
            footer { class: "hints", HintBar { hints: view.hints } }
        }
    }
}

/// The Tree dock's body on the Graph: the lab's outline.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn GraphTreePanel(page: GraphPage) -> Element {
    rsx! {
        div { class: "graph graph-panel", TreePane { ed: page.ed, tree: page.view.tree } }
    }
}

/// The Props dock's body on the Graph: the lab's inspector.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn GraphPropsPanel(page: GraphPage) -> Element {
    rsx! {
        div { class: "graph graph-panel inspector", Inspector { ed: page.ed, inspector: page.view.inspector } }
    }
}

/// Give the Graph's keys back their element.
fn focus_keys() {
    use wasm_bindgen::JsCast;
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.query_selector(".graph-keys").ok().flatten())
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}
