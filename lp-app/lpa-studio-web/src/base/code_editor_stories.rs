//! Base code-editor stories.
//!
//! Content is fixed and small so PNG captures stay deterministic; the
//! component's `data-story-wait` contract holds capture until CodeMirror
//! has initialized.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{CodeEditor, CodeEditorDiagnostic, CodeEditorLanguage};

const STORY_GLSL: &str = "\
// Studio code editor story fixture.
uniform float time;

vec4 render(vec2 pos) {
    float wave = 0.5 + 0.5 * sin(time + pos.x * 6.0);
    vec3 color = mix(vec3(0.05, 0.1, 0.2), vec3(0.2, 0.9, 0.6), wave);
    return vec4(color * pos.y, 1.0);
}
";

#[story(description = "Editable GLSL editor with an apply counter wired to Cmd/Ctrl+Enter.")]
fn glsl_editable() -> Element {
    let mut apply_count = use_signal(|| 0_u32);
    let mut modified = use_signal(|| false);

    rsx! {
        div { class: "tw:grid tw:w-full tw:max-w-xl tw:gap-2",
            div { class: "tw:flex tw:gap-3 tw:text-xs tw:text-muted-foreground",
                span { "applies: {apply_count()}" }
                span { if modified() { "modified" } else { "clean" } }
            }
            div { class: "tw:h-64 tw:rounded-md tw:border tw:border-border tw:overflow-hidden",
                CodeEditor {
                    doc: STORY_GLSL.to_string(),
                    language: CodeEditorLanguage::Glsl,
                    on_modified: move |value| modified.set(value),
                    on_apply: move |_text| {
                        apply_count.set(apply_count() + 1);
                    },
                }
            }
        }
    }
}

#[story(description = "Read-only GLSL editor, as used for non-editable asset bodies.")]
fn glsl_read_only() -> Element {
    rsx! {
        div { class: "tw:h-64 tw:w-full tw:max-w-xl tw:rounded-md tw:border tw:border-border tw:overflow-hidden",
            CodeEditor {
                doc: STORY_GLSL.to_string(),
                language: CodeEditorLanguage::Glsl,
                read_only: true,
            }
        }
    }
}

#[story(
    description = "Editor rendering a positioned diagnostic (line 5) as strip-driven lint chrome."
)]
fn glsl_diagnostic() -> Element {
    rsx! {
        div { class: "tw:h-64 tw:w-full tw:max-w-xl tw:rounded-md tw:border tw:border-border tw:overflow-hidden",
            CodeEditor {
                doc: STORY_GLSL.to_string(),
                language: CodeEditorLanguage::Glsl,
                diagnostics: vec![CodeEditorDiagnostic {
                    line: 5,
                    col: 18,
                    message: "expected ';', found '}'".to_string(),
                }],
            }
        }
    }
}
