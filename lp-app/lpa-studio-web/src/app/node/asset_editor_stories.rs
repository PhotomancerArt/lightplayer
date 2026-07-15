//! Stories for the inline asset editor.
//!
//! Fixed content and sizes for deterministic PNGs; the embedded
//! [`crate::base::CodeEditor`] holds capture via its `data-story-wait`
//! contract until CodeMirror has initialized. Each story renders the
//! `AssetEditor` over a controller-shaped `UiAssetEditor` fixture — the same
//! DTO the project controller embeds on an asset slot.
//!
//! These cover the fixed-height status bar's states. The **no-reflow**
//! guarantee is what to look for: the editor body sits at the same geometry
//! in every state (clean, unsaved, applying, compile error, apply failed) —
//! the bar changes tone/content without moving the editor. The bar's
//! `Modified` state is editor-local (driven by typing), so it is covered by
//! unit tests + live sim rather than a fixture story.

use dioxus::prelude::*;
use lpa_studio_core::{
    ArtifactLocation, UiAssetContent, UiAssetEditor as UiAssetEditorData, UiAssetEditorKind,
    UiShaderError,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::AssetEditor;
use crate::base::Platform;

const STORY_GLSL: &str = "\
uniform float time;

vec4 render(vec2 pos) {
    float ring = sin(length(pos - 0.5) * 40.0 - time);
    vec3 base = vec3(0.9, 0.3, 0.1);
    return vec4(base * ring, 1.0);
}
";

fn editor_fixture(content: Option<UiAssetContent>) -> UiAssetEditorData {
    UiAssetEditorData {
        artifact: ArtifactLocation::file("/blast.glsl"),
        kind: UiAssetEditorKind::Glsl,
        source: "blast.glsl".to_string(),
        content,
        in_flight: false,
        failure: None,
        shader_error: None,
    }
}

fn resolved(dirty: bool) -> Option<UiAssetContent> {
    Some(UiAssetContent::from_bytes(STORY_GLSL.as_bytes(), dirty, 4))
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditorStoryCard(
    editor: UiAssetEditorData,
    // Pinned (not detected) so the shortcut hints render identically on
    // every capture host; Mac is the default story platform.
    #[props(default = Platform::Mac)] platform: Platform,
    // The Auto toggle's initial state (on by default, like the app).
    #[props(default = true)] auto_apply: bool,
) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-2xl tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            AssetEditor { editor, platform, auto_apply_default: auto_apply }
        }
    }
}

#[story(description = "Clean, compiling: the status bar shows only the identity; editor at rest.")]
fn clean() -> Element {
    rsx! {
        EditorStoryCard { editor: editor_fixture(resolved(false)) }
    }
}

#[story(
    description = "The Auto toggle off: the pill goes muted and edits wait for a manual Apply (the M2 flow)."
)]
fn auto_apply_off() -> Element {
    rsx! {
        EditorStoryCard { editor: editor_fixture(resolved(false)), auto_apply: false }
    }
}

#[story(
    description = "Applied but not yet saved: the bar wears the amber Unsaved tone with a Save affordance and its ⌘S hint."
)]
fn unsaved() -> Element {
    rsx! {
        EditorStoryCard { editor: editor_fixture(resolved(true)) }
    }
}

#[story(
    description = "The unsaved bar on a non-Mac platform: the Save hint spells out Ctrl+S instead of ⌘S."
)]
fn unsaved_non_mac() -> Element {
    rsx! {
        EditorStoryCard { editor: editor_fixture(resolved(true)), platform: Platform::Other }
    }
}

#[story(description = "An apply awaiting its ack: the bar shows the working Applying… state.")]
fn applying() -> Element {
    let mut editor = editor_fixture(resolved(true));
    editor.in_flight = true;
    rsx! {
        EditorStoryCard { editor }
    }
}

#[story(
    description = "A failed apply (size guard): the bar goes error-toned with a full-error popup; editor unmoved."
)]
fn apply_failed() -> Element {
    let mut editor = editor_fixture(resolved(true));
    editor.failure = Some("shader too large to send (limit 10 KB)".to_string());
    rsx! {
        EditorStoryCard { editor }
    }
}

#[story(
    description = "A located compile error: the bar shows message + clickable line:col + full-error popup; the editor does not move, and the errored line gets a gutter marker."
)]
fn compile_error() -> Element {
    let mut editor = editor_fixture(resolved(true));
    editor.shader_error = Some(UiShaderError::parse(
        "shader compile: parse: error: expected ';', found '}'\n --> <shader>:4:34\n  |\n 4 |     float ring = sin(length(pos - 0.5) * 40.0 - time)\n  |                                  ^",
    ));
    rsx! {
        EditorStoryCard { editor }
    }
}

#[story(
    description = "A location-less compile error (recovery-blocked): the bar carries the message with no line:col; full-error popup available."
)]
fn compile_error_no_location() -> Element {
    let mut editor = editor_fixture(resolved(true));
    editor.shader_error = Some(UiShaderError::parse(
        "shader compile: compilation blocked after repeated crashes",
    ));
    rsx! {
        EditorStoryCard { editor }
    }
}

#[story(description = "A binary asset body: read-only note under a clean identity bar.")]
fn binary_read_only() -> Element {
    let editor = editor_fixture(Some(UiAssetContent::from_bytes(
        &[0xff, 0xfe, 0x00, 0x01],
        false,
        4,
    )));
    rsx! {
        EditorStoryCard { editor }
    }
}
