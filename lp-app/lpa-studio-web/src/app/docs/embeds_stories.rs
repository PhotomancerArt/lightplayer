//! Docs-embed stories: every state an article can be in, with **no live
//! workers anywhere**.
//!
//! The seam that makes this possible is that each embed splits in two: a
//! resolver component that reads page context (`SimCanvasEmbed`,
//! `PanelEmbed`, `EditorEmbed`) and a presentational one that takes the
//! `Ui*` data as props (`DocsSimCanvas`, `DocsPanelSurface`,
//! `DocsEditorSurface`). The stories render the presentational half with
//! the same hand-built fixtures the node and module stories use, so a docs
//! figure is judged against the identical shapes Studio derives — and no
//! `PreviewHost`, `DocsSimHost`, or browser Worker is booted to draw one.
//!
//! The editor stories are the same seam one level deeper: `AssetEditor`
//! only ever *dispatches* (content fetch, apply, revert), so a fixture
//! with resolved content and a handler that goes nowhere renders the live
//! surface with nothing behind it. What the fixtures cannot show is the
//! auto-apply loop itself; that is verified against a running sim.
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
use lpa_studio_core::{
    ArtifactLocation, UiAssetContent, UiAssetEditor as UiAssetEditorData, UiAssetEditorKind,
    UiShaderError, UiShaderUniform,
};
use lpa_studio_web_story_macros::story;

use crate::app::docs::embeds::docs_sims::DocsStudioActions;
use crate::app::docs::embeds::{
    DocsEditorSurface, DocsPanelSurface, DocsSimCanvas, OpenInStudioButton, PanelMode,
    SimCanvasView,
};
use crate::app::module::module_fixtures::plasma_read_panel;
use crate::app::node::node_story_fixtures::{control_preview_product, visual_preview_product};
use crate::base::Platform;

/// The article column: embeds are article-width, so the stories judge them
/// at one.
const ARTICLE: &str = "tw:w-[640px] tw:max-w-full";

/// The panel the docs sims present — one plasma scope's channels, in Read.
fn docs_panel() -> lpa_studio_core::UiPanelGroup {
    plasma_read_panel("/aurora.module/plasma_1.module")
}

/// The source the article's editor embed shows, trimmed to the lines the
/// article points at (the whole file is `examples/plasma-duo/shader.glsl`).
const DOCS_GLSL: &str = "\
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;
layout(binding = 2) uniform float scale;

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    float v = sin((uv.x * scale + phase * 13.0) * 6.2831853)
        + sin((uv.y * scale + phase * 9.0) * 6.2831853);
    float hue = v * 0.125 + phase * 5.0;
    vec3 rgb = 0.5 + 0.5 * cos(6.2831853 * (hue + vec3(0.0, 0.33, 0.67)));
    return vec4(rgb, 1.0);
}
";

/// The editor data the docs sim's shader face carries, hand-built in the
/// controller's shape — no host, no worker, no dispatch.
fn docs_editor(dirty: bool, shader_error: Option<UiShaderError>) -> UiAssetEditorData {
    UiAssetEditorData {
        artifact: ArtifactLocation::file("/shader.glsl"),
        kind: UiAssetEditorKind::Glsl,
        source: "shader.glsl".to_string(),
        content: Some(UiAssetContent::from_bytes(DOCS_GLSL.as_bytes(), dirty, 7)),
        in_flight: false,
        failure: None,
        shader_error,
        uniforms: vec![
            UiShaderUniform {
                name: "phase".to_string(),
                glsl_type: "float".to_string(),
            },
            UiShaderUniform {
                name: "scale".to_string(),
                glsl_type: "float".to_string(),
            },
        ],
        // Docs never carry the studio-level agent decoration: an article is
        // a shader being explained, not a workspace.
        agent: None,
    }
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
    description = "`view=product` as the article's HERO: the same real preview widened past the 320px map cap, so an article opens on the thing it is about instead of a thumbnail."
)]
fn sim_canvas_hero() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsSimCanvas {
                product: visual_preview_product("visual.out"),
                view: SimCanvasView::Product,
                hero: true,
            }
        }
    }
}

#[story(
    description = "`view=map fixture=<node>`: with two fixtures in one project the fence names which node's product to draw. The embed itself looks identical either way — the selection happens before the surface — so this is the caption's job to say."
)]
fn sim_canvas_fixture() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsSimCanvas {
                product: control_preview_product("control.disc"),
                view: SimCanvasView::Map,
                caption: "fixture=disc".to_string(),
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
    description = "The editor before its sim has synced: the same reserved box the live editor fills, so the article does not lurch when the source arrives."
)]
fn editor_loading() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsEditorSurface {}
        }
    }
}

#[story(
    description = "The `editor` embed live: the REAL inline GLSL editor (same CodeMirror, same gentle two-half bar, same 500 ms auto-apply) over the docs sim's shader source, with the page's Reset chip in the chrome."
)]
fn editor_live() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsEditorSurface {
                editor: docs_editor(false, None),
                on_action: move |_| {},
                on_reset: move |()| {},
                platform: Platform::Mac,
            }
        }
    }
}

#[story(
    description = "The article's promise that breaking it is fine: a typo shows as a compile error in the bar (with its line:col reveal) while the lamps upstream keep the last good frame. The right half still tells the truth about the applied edit — an error never hides it."
)]
fn editor_compile_error() -> Element {
    rsx! {
        div { class: ARTICLE,
            DocsEditorSurface {
                editor: docs_editor(
                    true,
                    Some(UiShaderError {
                        message: "expected ';', found '}'".to_string(),
                        line_col: Some((8, 42)),
                        raw: "shader compile: expected ';', found '}'\n --> shader.glsl:8:42"
                            .to_string(),
                    }),
                ),
                on_action: move |_| {},
                on_reset: move |()| {},
                platform: Platform::Mac,
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
