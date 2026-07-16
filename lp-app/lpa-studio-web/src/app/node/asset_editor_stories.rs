//! Stories for the inline asset editor.
//!
//! Fixed content and sizes for deterministic PNGs; the embedded
//! [`crate::base::CodeEditor`] holds capture via its `data-story-wait`
//! contract until CodeMirror has initialized. Each story renders the
//! `AssetEditor` over a controller-shaped `UiAssetEditor` fixture — the same
//! DTO the project controller embeds on an asset slot.
//!
//! These cover the gentle two-half status bar. What to look for: the bar's
//! **geometry is identical in every state** — same plain background, the
//! Revert/Save buttons always present (disabled vs enabled in place), the
//! applying dot's slot always reserved — and the error state (left half)
//! coexists with the Unsaved state (right half) instead of hiding it. The
//! applying dot's fade and the editor-local modified window are covered by
//! unit tests + live sim rather than fixture stories.

use dioxus::prelude::*;
use lpa_studio_core::{
    ArtifactLocation, UiAssetContent, UiAssetEditor as UiAssetEditorData, UiAssetEditorKind,
    UiShaderError, UiShaderUniform,
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
        // Matches STORY_GLSL's uniform block; also exercises the completion
        // assembly (the popup itself is interaction-only, not captured).
        uniforms: vec![UiShaderUniform {
            name: "time".to_string(),
            glsl_type: "float".to_string(),
        }],
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
) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-2xl tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            AssetEditor { editor, platform }
        }
    }
}

#[story(
    description = "Saved and compiling: identity on the left, muted Saved + disabled Revert/Save on the right — the buttons are present in every state."
)]
fn saved() -> Element {
    rsx! {
        EditorStoryCard { editor: editor_fixture(resolved(false)) }
    }
}

#[story(
    description = "Applied but not yet saved: amber Unsaved with live Revert and Save (⌘S) — same geometry as the saved state."
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

#[story(
    description = "An apply awaiting its ack: the subtle applying dot is lit; nothing else about the bar changes."
)]
fn applying() -> Element {
    let mut editor = editor_fixture(resolved(true));
    editor.in_flight = true;
    rsx! {
        EditorStoryCard { editor }
    }
}

#[story(
    description = "A failed apply (size guard): the left half carries the reason + full-error popup; the right half keeps Unsaved/Revert/Save live."
)]
fn apply_failed() -> Element {
    let mut editor = editor_fixture(resolved(true));
    editor.failure = Some("shader too large to send (limit 10 KB)".to_string());
    rsx! {
        EditorStoryCard { editor }
    }
}

#[story(
    description = "A located compile error: error text + clickable line:col + popup on the LEFT while Unsaved/Revert/Save stay live on the RIGHT — the error does not hide the persistence state, and Revert works from here."
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
    description = "A location-less compile error (recovery-blocked): the message with no line:col; full-error popup available."
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

#[story(
    description = "A binary asset body: read-only note under a plain identity bar; the persistence buttons stay mounted but disabled."
)]
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
