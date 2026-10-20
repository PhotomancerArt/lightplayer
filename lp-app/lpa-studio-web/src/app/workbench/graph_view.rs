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
use lab_mapping_model::{BoardFrame, GraphInput, GraphVerb, LayoutKind, SizeClass, Tool};
use lab_mapping_web::{
    Canvas, GRAPH_CSS, GraphEditor, HintBar, Inspector, TreePane, View, use_view,
};
use lpa_studio_core::app::graph::{GraphHandle, UiGraphState, UiGraphView, graph_verb_path};
use lpa_studio_core::{OfferPath, UiAction, UiOffer};

use crate::base::{InlineButtonTone, inline_text_button_class};
use crate::core::offer::offers_context::use_verbs_of;

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
    let handle = use_hook(|| try_consume_context::<GraphHandle>().unwrap_or_default());
    use_graph_page_with(handle, graph)
}

/// The Graph's page state over `handle` (a story's own editor).
pub fn use_graph_page_with(handle: GraphHandle, graph: Option<UiGraphView>) -> GraphPage {
    let tick = use_signal(|| 0_u64);
    let ed = use_hook(|| {
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
pub fn GraphCenter(
    page: GraphPage,
    state: Option<UiGraphState>,
    /// The Map view's address, the way forward when the Graph can't open
    /// the project (docs/ux P7). `None` off a lens.
    #[props(default)]
    map_href: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let body = match state {
        None | Some(UiGraphState::Opening) => rsx! {
            div { class: "tw:m-auto tw:text-sm tw:text-muted-foreground", "Opening the project on the Graph…" }
        },
        Some(UiGraphState::Refused { why }) => rsx! {
            div { class: "tw:m-auto tw:grid tw:max-w-md tw:gap-2 tw:text-center tw:text-sm tw:text-muted-foreground",
                div { class: "tw:font-semibold tw:text-strong-foreground", "The Graph can't open this project" }
                div { "Open it in Map to see and edit its mapping." }
                if let Some(href) = map_href {
                    div {
                        a {
                            class: inline_text_button_class(InlineButtonTone::Action, false),
                            href: "{href}",
                            "Open in Map"
                        }
                    }
                }
                div { class: "tw:text-xs tw:text-subtle-foreground", "{why}" }
            }
        },
        Some(UiGraphState::Open { live }) => rsx! {
            GraphCanvas { page: page.clone(), live, on_action }
        },
    };
    rsx! {
        style { {GRAPH_CSS} }
        div { class: "graph graph-center tw:flex tw:min-h-0 tw:flex-1 tw:flex-col", {body} }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn GraphCanvas(page: GraphPage, live: bool, on_action: EventHandler<UiAction>) -> Element {
    let GraphPage { ed, view, frame } = page;
    let toolbar = view.toolbar.read().clone();
    let layout = toolbar.layout;
    // The toolbar's verbs are core's offers (`project/graph/…`): the same
    // ones the keys and the hint bar name.
    let verbs = use_verbs_of(Some(
        OfferPath::project().child(lpa_studio_core::app::graph::GRAPH_GROUP),
    ));
    let offered = verbs();
    let tool_on = |verb: GraphVerb| match verb {
        GraphVerb::SelectTool => toolbar.tool == Tool::Select,
        GraphVerb::PathTool => toolbar.tool == Tool::Path,
        GraphVerb::CircleTool => toolbar.tool == Tool::Circle,
        GraphVerb::MoveDraw => toolbar.move_draw,
        _ => false,
    };
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
            div { class: "graph-bar top tw:flex tw:flex-wrap tw:items-center tw:gap-3",
                for group in TOOLBAR {
                    div { class: "tools",
                        for verb in group.iter().copied() {
                            OfferTool {
                                key: "{verb.name()}",
                                verb,
                                offer: offered.iter().find(|o| o.path == graph_verb_path(verb)).cloned(),
                                on: tool_on(verb),
                                on_action,
                            }
                        }
                    }
                }
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
                span { class: "tw:ml-auto tw:text-xs tw:text-subtle-foreground",
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

/// The toolbar's verbs, in groups: the tools, Move & draw, undo and redo.
const TOOLBAR: [&[GraphVerb]; 3] = [
    &[
        GraphVerb::SelectTool,
        GraphVerb::PathTool,
        GraphVerb::CircleTool,
    ],
    &[GraphVerb::MoveDraw],
    &[GraphVerb::Undo, GraphVerb::Redo],
];

/// One verb on the toolbar: its offer's button, or the verb greyed while
/// it isn't on offer, so the bar stays still.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OfferTool(
    verb: GraphVerb,
    offer: Option<UiOffer>,
    on: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let label = offer
        .as_ref()
        .map_or(verb.label(), |o| o.label())
        .to_string();
    let enabled = offer.as_ref().is_some_and(UiOffer::is_enabled);
    let class = if on { "tool on" } else { "tool" };
    rsx! {
        button {
            class,
            tabindex: "-1",
            disabled: !enabled,
            onmousedown: move |evt| evt.prevent_default(),
            onclick: move |_| {
                if let Some(offer) = offer.as_ref() {
                    on_action.call(offer.action.clone());
                }
            },
            "{label} "
            kbd { "{verb.keys()}" }
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
