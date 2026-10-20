//! The Graph view's stories: the mapping lab's canvas on real catalog
//! projects (their own files, opened by `lab-mapping-files` as core opens
//! them), with the lab's tree and inspector beside it. No board is
//! attached in a story, so the lab's simulated board stands in for the
//! board's frames.

use dioxus::prelude::*;
use lab_mapping_files::{MemFiles, Opened};
use lab_mapping_model::{Editor, GraphInput, LayoutKind, NodeId, Target, Vec2};
use lpa_studio_core::app::graph::{GraphHandle, UiGraphFrame, UiGraphState, UiGraphView};
use lpa_studio_web_story_macros::story;

use super::graph_view::{GraphCenter, GraphPropsPanel, GraphTreePanel, use_graph_page_with};

#[story(
    description = "The PLAYFUL choker on the Graph, nothing selected: every node a card laid out by role — clock and power button on top, the shader feeding the fixture over visual.out, the fixture feeding its output over control.out — and the fixture in Studio's live view, its lamps as cells of light."
)]
fn graph_choker_live() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::Nothing } }
}

#[story(
    description = "The choker's fixture selected: it opens as the mapping editor in place — its twelve paths, where data enters and which way it runs — and the inspector shows its size and patch rule."
)]
fn graph_choker_fixture_selected() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::Fixture } }
}

#[story(
    description = "One of the choker's paths selected: its span on the output's port is lit, the inspector says which pixels it takes, and the rest of the fixture dims."
)]
fn graph_choker_span_selected() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::FirstPath } }
}

#[story(
    description = "A catalog pattern's fixture selected: a one-lamp grid and a disc of eight rings on one strand, each drawn whole (the grid as its box, the disc as its rings), its lamps where the product puts them."
)]
fn graph_pattern_whole_shapes() -> Element {
    rsx! { GraphStory { example: "catalog/plasma", setup: Setup::Fixture } }
}

#[story(
    description = "A fixture mapped with the product's older point lists: the Graph opens the project round it and draws the fixture as its box alone, saying where to edit it (Nodes)."
)]
fn graph_point_list_box() -> Element {
    rsx! { GraphStory { example: "catalog/fiber-headband", setup: Setup::Nothing } }
}

#[story(
    description = "A project the Graph can't open at all: the reason, small, under the way forward, which opens the project in Map (docs/ux P7, no dead ends)."
)]
fn graph_refused() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::Refused } }
}

#[story(
    description = "The choker in the stack layout: one column in tree order at a fixed zoom, scrolling natively — the Graph's other layout, switched in its toolbar."
)]
fn graph_choker_stack() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::Stack } }
}

/// What a story does to the editor before it draws.
#[derive(Clone, Copy, PartialEq)]
enum Setup {
    Nothing,
    Fixture,
    FirstPath,
    Stack,
    /// The project refused whole (the state a broken patch gives).
    Refused,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn GraphStory(example: &'static str, setup: Setup) -> Element {
    let handle = use_hook(|| story_handle(example, setup));
    let view = {
        let e = handle.editor().borrow();
        UiGraphView {
            state: match setup {
                Setup::Refused => UiGraphState::Refused {
                    why: "row 3 of fixture.patch.json: no output is named \"porch\"".to_string(),
                },
                _ => UiGraphState::Open { live: true },
            },
            generation: 1,
            frame: UiGraphFrame(std::rc::Rc::new(e.board_frame(0.0))),
        }
    };
    let page = use_graph_page_with(handle, Some(view.clone()));
    rsx! {
        div { class: "tw:flex tw:h-[640px] tw:overflow-hidden tw:rounded-md tw:border tw:border-border-strong",
            div { class: "tw:w-[240px] tw:flex-none tw:overflow-y-auto tw:bg-card-subtle tw:p-2.5",
                GraphTreePanel { page: page.clone() }
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-1 tw:flex-col tw:border-x tw:border-border-subtle",
                GraphCenter {
                    page: page.clone(),
                    state: Some(view.state.clone()),
                    map_href: Some("#".to_string()),
                    on_action: move |_| {},
                }
            }
            div { class: "tw:w-[280px] tw:flex-none tw:overflow-y-auto tw:bg-card-subtle tw:p-2.5",
                GraphPropsPanel { page }
            }
        }
    }
}

/// A handle on an editor holding the embedded example, with its layout
/// pinned and the story's selection.
///
/// Every story chooses its layout: the canvas, or the stack for the one
/// that is about it. Left unchosen, the layout follows the size class the
/// canvas reports on its first resize, and which one a capture sees then
/// depends on whether that report lands before the shot. A chosen layout
/// is never undone by a size report (`Editor::layout`).
fn story_handle(example: &str, setup: Setup) -> GraphHandle {
    let example = lpa_studio_core::app::home::embedded_example(example).expect("embedded");
    let files = MemFiles::new(example.files.iter().filter_map(|(path, bytes)| {
        Some((path.to_string(), String::from_utf8(bytes.to_vec()).ok()?))
    }));
    let opened = Opened::open(&files).expect("the example opens on the Graph");
    let handle = GraphHandle::default();
    {
        let mut e = handle.editor().borrow_mut();
        *e = Editor::new(opened.project);
        e.resize(Vec2::new(720.0, 560.0));
        let placement = lab_layout::canvas_layout(&e.layout_input());
        e.set_placement(&placement);
        e.fit();
    }
    let layout = match setup {
        Setup::Stack => LayoutKind::Stack,
        _ => LayoutKind::Canvas,
    };
    handle.input(GraphInput::SetLayout(Some(layout)));
    let fixture = NodeId::new("fixture");
    match setup {
        Setup::Nothing => {}
        Setup::Fixture => {
            handle.input(GraphInput::Select(vec![Target::Node(fixture)]));
        }
        Setup::FirstPath => {
            let first = handle
                .editor()
                .borrow()
                .project
                .fixture(&fixture)
                .map(|f| f.components[0].id.clone());
            if let Some(id) = first {
                handle.input(GraphInput::Select(vec![Target::Component(id)]));
            }
        }
        Setup::Stack | Setup::Refused => {}
    }
    handle
}
