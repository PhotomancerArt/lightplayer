//! Stories for the shader agent chat (the editor region's Agent tab).
//!
//! Fixed transcripts for deterministic PNGs. Coverage: the needs-setup
//! empty state per provider (onboarding guidance), an idle conversation
//! with a completed tool row, a markdown-rich reply, a streaming run
//! (working cursor + Stop button), a tool row expanded to its summary
//! detail, and the provider-error strip. The `Agent | Code` strip itself
//! rides the tabbed stories (`ShaderEditorTabs` over the same fixtures),
//! including the dirty dot on the Code tab.

use dioxus::prelude::*;
use lpa_studio_core::{
    AgentProvider, ArtifactLocation, UiAgentAvailability, UiAgentStatus, UiAgentToolRow,
    UiAgentTurn, UiAgentUsage, UiAgentView, UiAssetContent, UiAssetEditor as UiAssetEditorData,
    UiAssetEditorKind, UiShaderUniform, provider_guidance,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::{AgentChatPane, ShaderEditorTab, ShaderEditorTabs};
use crate::base::Platform;

fn agent_fixture(status: UiAgentStatus, turns: Vec<UiAgentTurn>) -> UiAgentView {
    UiAgentView {
        artifact: ArtifactLocation::file("/blast.glsl"),
        availability: UiAgentAvailability::Ready,
        setup: None,
        status,
        turns,
        usage: UiAgentUsage {
            input_tokens: 2841,
            output_tokens: 512,
        },
        // 2841 in × $3 + 512 out × $15 per MTok (claude-sonnet-5 rates).
        estimated_cost: Some("~$0.0162".to_string()),
    }
}

fn done_tool_row() -> UiAgentToolRow {
    UiAgentToolRow {
        id: "tu_1".to_string(),
        note: Some("slow the rings down".to_string()),
        done: true,
        staged: true,
        shader_ok: Some(true),
        probes: 2,
        warnings: 0,
        error: None,
        detail: "{\n  \"note\": \"slow the rings down\",\n  \"probes\": 2,\n  \"shader_ok\": true,\n  \"staged\": true,\n  \"warnings\": 0\n}".to_string(),
    }
}

fn idle_transcript() -> Vec<UiAgentTurn> {
    vec![
        UiAgentTurn::User {
            text: "Make the rings pulse more slowly".to_string(),
        },
        UiAgentTurn::Assistant {
            text: "I'll slow the ring motion and check the result.".to_string(),
        },
        UiAgentTurn::Tool(done_tool_row()),
        UiAgentTurn::Assistant {
            text: "Done — the rings now breathe at about a third of the old speed. \
                   The edit is staged in your editor; Save keeps it, Revert undoes it."
                .to_string(),
        },
    ]
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChatStoryCard(view: UiAgentView, #[props(default = false)] tool_rows_expanded: bool) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-2xl tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            AgentChatPane { view, tool_rows_expanded }
        }
    }
}

#[story(
    description = "Anthropic selected but no API key configured: the Agent tab shows the provider's onboarding guidance pointing at the settings gear; Code stays the default tab."
)]
fn needs_key() -> Element {
    rsx! {
        ChatStoryCard { view: needs_setup_view(AgentProvider::Anthropic) }
    }
}

#[story(
    description = "OpenAI selected before setup completes: billing guidance and the platform.openai.com link in the empty state."
)]
fn needs_key_openai() -> Element {
    rsx! {
        ChatStoryCard { view: needs_setup_view(AgentProvider::OpenAi) }
    }
}

#[story(
    description = "Custom provider before setup completes: the local-server guidance (Ollama base URL, CORS note) in the empty state."
)]
fn needs_key_custom() -> Element {
    rsx! {
        ChatStoryCard { view: needs_setup_view(AgentProvider::Custom) }
    }
}

#[story(
    description = "A markdown-rich reply: demoted heading, list, inline and fenced code, and a link render through the safe subset (no raw HTML)."
)]
fn markdown_reply() -> Element {
    let turns = vec![
        UiAgentTurn::User {
            text: "What does this shader do?".to_string(),
        },
        UiAgentTurn::Assistant {
            text: "## What the lights do\n\n\
                   The rings **pulse outward** from the center, in two phases:\n\n\
                   1. A bright ring expands over ~2 seconds\n\
                   2. It fades while the *next* ring starts\n\n\
                   The speed comes from `time * 0.35` in this line:\n\n\
                   ```glsl\nfloat ring = sin(length(pos - 0.5) * 40.0 - time * 0.35);\n```\n\n\
                   > Tip: smaller multipliers slow every ring down together.\n\n\
                   More on the dialect in the [naga docs](https://docs.rs/naga)."
                .to_string(),
        },
    ];
    rsx! {
        ChatStoryCard { view: agent_fixture(UiAgentStatus::Idle, turns) }
    }
}

fn needs_setup_view(provider: AgentProvider) -> UiAgentView {
    let mut view = UiAgentView::empty(
        ArtifactLocation::file("/blast.glsl"),
        UiAgentAvailability::NeedsKey,
    );
    view.setup = Some(provider_guidance(provider));
    view
}

#[story(
    description = "Idle after a completed run: user/assistant bubbles, a collapsed tool row (good dot = valid compile), usage footnote, enabled composer."
)]
fn idle_with_transcript() -> Element {
    rsx! {
        ChatStoryCard { view: agent_fixture(UiAgentStatus::Idle, idle_transcript()) }
    }
}

#[story(
    description = "Mid-run streaming: working cursor on the trailing assistant text and the Stop button replacing Send."
)]
fn streaming() -> Element {
    let turns = vec![
        UiAgentTurn::User {
            text: "Give it a warmer palette".to_string(),
        },
        UiAgentTurn::Assistant {
            text: "Shifting the base color toward amber and re-checking the".to_string(),
        },
    ];
    rsx! {
        ChatStoryCard { view: agent_fixture(UiAgentStatus::Streaming, turns) }
    }
}

#[story(
    description = "Tool row expanded: the compact one-liner opens to the experiment's summary detail."
)]
fn tool_row_expanded() -> Element {
    rsx! {
        ChatStoryCard {
            view: agent_fixture(UiAgentStatus::Idle, idle_transcript()),
            tool_rows_expanded: true,
        }
    }
}

#[story(
    description = "Provider failure: the error strip names the failure (401 here) with the retry hint; the composer stays usable."
)]
fn provider_error() -> Element {
    let turns = vec![
        UiAgentTurn::User {
            text: "hello".to_string(),
        },
        UiAgentTurn::Notice {
            text: "Provider error: HTTP 401: authentication_error: invalid x-api-key".to_string(),
        },
    ];
    rsx! {
        ChatStoryCard {
            view: agent_fixture(
                UiAgentStatus::Error {
                    message: "HTTP 401: authentication_error: invalid x-api-key".to_string(),
                    retryable: false,
                },
                turns,
            ),
        }
    }
}

#[story(
    description = "The Agent | Code strip over the editor region, Code tab active: the plain editor renders under the strip; the dirty content wears the warning dot on the Code tab."
)]
fn tabs_code_active() -> Element {
    let mut editor = tabbed_editor_fixture();
    editor.agent = Some(agent_fixture(UiAgentStatus::Idle, idle_transcript()));
    rsx! {
        div { class: "tw:w-full tw:max-w-2xl tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            ShaderEditorTabs {
                editor,
                initial_tab: Some(ShaderEditorTab::Code),
                platform: Platform::Mac,
            }
        }
    }
}

#[story(
    description = "Agent tab active over dirty (unsaved) content: the Code tab's warning dot keeps the unsaved edit visible without leaving the chat."
)]
fn tabs_dirty_from_agent() -> Element {
    let mut editor = tabbed_editor_fixture();
    editor.agent = Some(agent_fixture(UiAgentStatus::Idle, idle_transcript()));
    rsx! {
        div { class: "tw:w-full tw:max-w-2xl tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            ShaderEditorTabs {
                editor,
                initial_tab: Some(ShaderEditorTab::Agent),
                platform: Platform::Mac,
            }
        }
    }
}

const STORY_GLSL: &str = "\
layout(binding = 0) uniform float time;

vec4 render(vec2 pos) {
    float ring = sin(length(pos - 0.5) * 40.0 - time * 0.35);
    vec3 base = vec3(0.9, 0.3, 0.1);
    return vec4(base * ring, 1.0);
}
";

fn tabbed_editor_fixture() -> UiAssetEditorData {
    UiAssetEditorData {
        artifact: ArtifactLocation::file("/blast.glsl"),
        kind: UiAssetEditorKind::Glsl,
        source: "blast.glsl".to_string(),
        content: Some(UiAssetContent::from_bytes(STORY_GLSL.as_bytes(), true, 4)),
        in_flight: false,
        failure: None,
        shader_error: None,
        uniforms: vec![UiShaderUniform {
            name: "time".to_string(),
            glsl_type: "float".to_string(),
        }],
        agent: None,
    }
}
