//! Docs-embed stories: every state an article can be in, with **no live
//! workers anywhere**.
//!
//! The seam that makes this possible is that each embed splits in two: a
//! resolver component that reads page context (`SimCanvasEmbed`,
//! `PanelEmbed`) and a presentational one that takes the `Ui*` data as
//! props (`DocsSimCanvas`, `DocsPanelSurface`). The stories render the
//! presentational half with the same hand-built fixtures the node and
//! module stories use, so a docs figure is judged against the identical
//! shapes Studio derives — and no `PreviewHost`, `DocsSimHost`, or browser
//! Worker is booted to draw one.
//!
//! `open-in-studio` needs no fixture at all: with no `DocsStudioActions`
//! in context it renders its inert state, which is exactly the state a
//! story should capture (the settings-chip precedent).
//!
//! Not covered here, deliberately: the hero preview, which is a
//! `PreviewHost` lease and would boot a worker under capture. Its states
//! are the gallery thumb's, and those already have static-injection
//! stories.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::docs::embeds::docs_sims::DocsStudioActions;
use crate::app::docs::embeds::{
    DocsPanelSurface, DocsSimCanvas, OpenInStudioButton, PanelMode, SimCanvasView,
};
use crate::app::module::module_fixtures::plasma_read_panel;
use crate::app::node::node_story_fixtures::{control_preview_product, visual_preview_product};

/// The article column: embeds are article-width, so the stories judge them
/// at one.
const ARTICLE: &str = "tw:w-[640px] tw:max-w-full";

/// The panel the docs sims present — one plasma scope's channels, in Read.
fn docs_panel() -> lpa_studio_core::UiPanelGroup {
    plasma_read_panel("/aurora.module/plasma_1.module")
}

#[story(
    description = "The sim is still booting: a calm reserved-height box that says what is coming, so the article never lurches when the first frame lands."
)]
fn sim_canvas_loading() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsSimCanvas { view: SimCanvasView::Map, caption: "A 16x16 grid".to_string() }
        }
    }
}

#[story(
    description = "`view=map` live: the REAL lamp-layout renderer (the same one the fixture card wears) over a docs sim's control product."
)]
fn sim_canvas_map() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsSimCanvas {
                product: control_preview_product("output"),
                view: SimCanvasView::Map,
                caption: "A 16x16 grid".to_string(),
            }
        }
    }
}

#[story(
    description = "`view=product` live: the rendered visual buffer — what the shader drew, before a mapping decides where it lands."
)]
fn sim_canvas_product() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsSimCanvas {
                product: visual_preview_product("visual.out"),
                view: SimCanvasView::Product,
                caption: "What the shader drew".to_string(),
            }
        }
    }
}

#[story(
    description = "The panel before its sims have synced: the same reserved box, sized for a row of knobs."
)]
fn panel_loading() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsPanelSurface { mode: PanelMode::Interactive }
        }
    }
}

#[story(
    description = "`mode=interactive`: the real ModulePanel at play density, with the Reset chip in the chrome. Dragging a knob here fans the write out to every sim the fence named."
)]
fn panel_interactive() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsPanelSurface {
                panel: docs_panel(),
                mode: PanelMode::Interactive,
                on_action: move |_| {},
                on_reset: move |()| {},
            }
        }
    }
}

#[story(
    description = "`mode=readonly`: the identical surface with no dispatchers and pointer events off — a picture of a panel that still looks like the real thing, with the mode said out loud and no Reset chip."
)]
fn panel_readonly() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsPanelSurface {
                panel: docs_panel(),
                mode: PanelMode::Readonly,
                on_action: move |_| {},
                on_reset: move |()| {},
            }
        }
    }
}

#[story(
    description = "`open-in-studio` with no app dispatcher in context (the story book, any non-app host): inert, same footprint, and the tooltip says why."
)]
fn open_in_studio_inert() -> Element {
    rsx! {
        div { class: ARTICLE,
            OpenInStudioButton { example_id: "examples/plasma".to_string() }
        }
    }
}

#[story(
    description = "`open-in-studio` in the running app: the page's call to action, in the brand accent, with the fence's own label."
)]
fn open_in_studio_live() -> Element {
    // The context the app provides, faked with a handler that goes
    // nowhere: what is being judged is the live rendering, not the open.
    let actions = use_context_provider(DocsStudioActions::empty);
    let handler = use_hook(|| EventHandler::new(move |_action| {}));
    *actions.0.borrow_mut() = Some(handler);
    rsx! {
        div { class: ARTICLE,
            OpenInStudioButton {
                example_id: "examples/plasma".to_string(),
                label: "Open the plasma shader".to_string(),
            }
        }
    }
}
