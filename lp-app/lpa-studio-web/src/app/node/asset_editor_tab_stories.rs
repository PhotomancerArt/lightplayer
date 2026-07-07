//! Stories for the node pane's asset editor tab.
//!
//! Fixed content and sizes for deterministic PNGs; the embedded
//! [`crate::base::CodeEditor`] holds capture via its `data-story-wait`
//! contract until CodeMirror has initialized.

use dioxus::prelude::*;
use lpa_studio_core::{
    ArtifactLocation, UiAssetContent, UiAssetEditorKind, UiAssetEditorTab, UiConfigSlot,
    UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeTabBody, UiNodeView, UiShaderError, UiSlotAsset,
    UiStatus,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::{ConfigSlotRow, NodePane, NodePaneActiveTab, NodePaneTab};

const STORY_GLSL: &str = "\
uniform float time;

vec4 render(vec2 pos) {
    float ring = sin(length(pos - 0.5) * 40.0 - time);
    vec3 base = vec3(0.9, 0.3, 0.1);
    return vec4(base * ring, 1.0);
}
";

fn editor_tab_fixture(content: Option<UiAssetContent>) -> UiAssetEditorTab {
    UiAssetEditorTab {
        artifact: ArtifactLocation::file("/blast.glsl"),
        kind: UiAssetEditorKind::Glsl,
        source: "blast.glsl".to_string(),
        content,
        in_flight: false,
        failure: None,
        shader_error: None,
    }
}

fn shader_node_view(editor: UiAssetEditorTab) -> UiNodeView {
    UiNodeView::new(
        UiNodeHeader::new("Blast", "Shader", "/fyeah_sign.show/blast.shader")
            .with_source("blast.json")
            .with_status(UiStatus::good("Running")),
        vec![
            UiNodeTab::main(vec![UiNodeSection::AssetSlots(vec![UiConfigSlot::asset(
                "source",
                "Source",
                UiSlotAsset::new("blast.glsl", UiAssetEditorKind::Glsl)
                    .with_detail("artifact, rev 19"),
            )])]),
            UiNodeTab::new("editor", UiNodeTabBody::AssetEditor(editor)),
        ],
    )
    .with_node_id("shader-blast")
}

#[story(description = "Editor tab with clean resolved content: no chips, Apply disabled.")]
fn editor_tab_clean() -> Element {
    let view = shader_node_view(editor_tab_fixture(Some(UiAssetContent::from_bytes(
        STORY_GLSL.as_bytes(),
        false,
        4,
    ))));

    rsx! {
        div { class: "tw:w-full tw:max-w-2xl",
            NodePane { view, initial_tab: 1 }
        }
    }
}

#[story(
    description = "Editor tab in the modified (unapplied) state: local chip shown, Apply enabled."
)]
fn editor_tab_modified() -> Element {
    let view = shader_node_view(editor_tab_fixture(Some(UiAssetContent::from_bytes(
        STORY_GLSL.as_bytes(),
        false,
        4,
    ))));

    rsx! {
        div { class: "tw:w-full tw:max-w-2xl",
            NodePane { view, initial_tab: 1, initially_editor_modified: true }
        }
    }
}

#[story(description = "Editor tab with a failed apply: error strip carries the parked reason.")]
fn editor_tab_failed_send() -> Element {
    let mut editor = editor_tab_fixture(Some(UiAssetContent::from_bytes(
        STORY_GLSL.as_bytes(),
        true,
        5,
    )));
    editor.failure = Some("shader too large to send (limit 10 KB)".to_string());
    let view = shader_node_view(editor);

    rsx! {
        div { class: "tw:w-full tw:max-w-2xl",
            NodePane { view, initial_tab: 1 }
        }
    }
}

#[story(description = "Editor tab while an apply awaits its ack: the in-flight chip shows.")]
fn editor_tab_in_flight() -> Element {
    let mut editor = editor_tab_fixture(Some(UiAssetContent::from_bytes(
        STORY_GLSL.as_bytes(),
        true,
        5,
    )));
    editor.in_flight = true;
    let view = shader_node_view(editor);

    rsx! {
        div { class: "tw:w-full tw:max-w-2xl",
            NodePane { view, initial_tab: 1 }
        }
    }
}

#[story(
    description = "Editor tab with a located compile error: strip with clickable line:col, gutter marker."
)]
fn editor_tab_compile_error() -> Element {
    let mut editor = editor_tab_fixture(Some(UiAssetContent::from_bytes(
        STORY_GLSL.as_bytes(),
        true,
        6,
    )));
    editor.shader_error = Some(UiShaderError::parse(
        "shader compile: parse: error: expected ';', found '}'\n --> <shader>:4:34\n  |\n 4 |     float ring = sin(length(pos - 0.5) * 40.0 - time)\n  |                                  ^",
    ));
    let view = shader_node_view(editor);

    rsx! {
        div { class: "tw:w-full tw:max-w-2xl",
            NodePane { view, initial_tab: 1 }
        }
    }
}

#[story(
    description = "Editor tab with a location-less compile error (recovery-blocked): strip only, no marker."
)]
fn editor_tab_compile_error_no_location() -> Element {
    let mut editor = editor_tab_fixture(Some(UiAssetContent::from_bytes(
        STORY_GLSL.as_bytes(),
        true,
        6,
    )));
    editor.shader_error = Some(UiShaderError::parse(
        "shader compile: compilation blocked after repeated crashes",
    ));
    let view = shader_node_view(editor);

    rsx! {
        div { class: "tw:w-full tw:max-w-2xl",
            NodePane { view, initial_tab: 1 }
        }
    }
}

#[story(description = "Asset slot row inside an editor-tab pane: the row offers Open in editor.")]
fn editor_tab_slot_row_affordance() -> Element {
    // Provide the pane context the row's affordance keys on, as `NodePane`
    // does, so the expanded row shows the button in isolation.
    let active_tab = use_signal(|| NodePaneTab::Index(0));
    use_context_provider(|| NodePaneActiveTab(active_tab));
    let slot = UiConfigSlot::asset(
        "source",
        "Source",
        UiSlotAsset::new("blast.glsl", UiAssetEditorKind::Glsl).with_detail("artifact, rev 19"),
    );

    rsx! {
        div { class: "tw:w-full tw:max-w-2xl tw:rounded-md tw:border tw:border-border tw:bg-card",
            ConfigSlotRow {
                slot,
                depth: 0,
                index: 0,
                initially_expanded: Some(true),
            }
        }
    }
}
