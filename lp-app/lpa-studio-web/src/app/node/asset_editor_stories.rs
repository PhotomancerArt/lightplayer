//! Stories for the inline asset editor.
//!
//! Fixed content and sizes for deterministic PNGs; the embedded
//! [`crate::base::CodeEditor`] holds capture via its `data-story-wait`
//! contract until CodeMirror has initialized. Each story renders the
//! `AssetEditor` over a controller-shaped `UiAssetEditor` fixture — the same
//! DTO the project controller embeds on an asset slot.

use dioxus::prelude::*;
use lpa_studio_core::{
    ArtifactLocation, UiAssetContent, UiAssetEditor as UiAssetEditorData, UiAssetEditorKind,
    UiShaderError,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::AssetEditor;

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
fn EditorStoryCard(editor: UiAssetEditorData) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-2xl tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            AssetEditor { editor }
        }
    }
}

#[story(description = "Inline editor over resolved GLSL: header with source, kind, and Apply.")]
fn editable() -> Element {
    rsx! {
        EditorStoryCard { editor: editor_fixture(resolved(false)) }
    }
}

#[story(description = "A failed apply: the size-guard reason shows as an error strip.")]
fn failed_send() -> Element {
    let mut editor = editor_fixture(resolved(true));
    editor.failure = Some("shader too large to send (limit 10 KB)".to_string());
    rsx! {
        EditorStoryCard { editor }
    }
}

#[story(description = "An apply awaiting its ack: the in-flight chip shows.")]
fn in_flight() -> Element {
    let mut editor = editor_fixture(resolved(true));
    editor.in_flight = true;
    rsx! {
        EditorStoryCard { editor }
    }
}

#[story(
    description = "A located compile error: strip with clickable line:col plus a gutter marker."
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

#[story(description = "A location-less compile error (recovery-blocked): strip only, no marker.")]
fn compile_error_no_location() -> Element {
    let mut editor = editor_fixture(resolved(true));
    editor.shader_error = Some(UiShaderError::parse(
        "shader compile: compilation blocked after repeated crashes",
    ));
    rsx! {
        EditorStoryCard { editor }
    }
}

#[story(description = "A binary asset body: read-only, no editor.")]
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
