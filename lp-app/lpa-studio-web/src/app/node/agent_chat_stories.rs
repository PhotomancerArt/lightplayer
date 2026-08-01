//! Stories for the shader agent chat (the editor region's Agent tab).
//!
//! Fixed transcripts for deterministic PNGs. Coverage: the needs-setup
//! empty state per provider (onboarding guidance), an idle conversation
//! with a completed tool row, a markdown-rich reply, a streaming run
//! (working cursor + Stop button), a tool row expanded to its summary
//! detail, and the provider-error strip. The `Agent | Code` strip itself
//! rides the tabbed stories (`ShaderEditorTabs` over the same fixtures),
//! including the dirty dot on the Code tab.

use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_core::{
    AgentProvider, ArtifactLocation, UiAgentAvailability, UiAgentHistoryEntry, UiAgentModelView,
    UiAgentStatus, UiAgentToolRow, UiAgentTurn, UiAgentUsage, UiAgentView, UiAssetContent,
    UiAssetEditor as UiAssetEditorData, UiAssetEditorKind, UiModelOption, UiNoticeLevel,
    UiProductPreview, UiShaderUniform, provider_guidance,
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
            ..UiAgentUsage::default()
        },
        // 2841 in × $3 + 512 out × $15 per MTok (claude-sonnet-5 rates).
        estimated_cost: Some("~$0.0162".to_string()),
        history: Vec::new(),
        history_dropped: 0,
        // No fetched list by default: the footer chip renders its
        // fallback-label form (just the effective id).
        model: UiAgentModelView {
            effective: Some("claude-sonnet-5".to_string()),
            options: Vec::new(),
            loading: false,
        },
        debug: None,
    }
}

fn done_tool_row() -> UiAgentToolRow {
    UiAgentToolRow {
        id: "tu_1".to_string(),
        note: Some("slow the rings down".to_string()),
        phase: None,
        done: true,
        staged: true,
        edit_turn: Some(1),
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
            // The inert connect handler makes the needs-setup stories render
            // the OpenRouter CTA exactly as the product does.
            AgentChatPane { view, tool_rows_expanded, on_connect: move |_| {} }
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
    description = "A run cut off by the output-token limit: the dangling tool row resolved as interrupted (error dot) and the warning-toned truncation notice — the state that used to end silently."
)]
fn run_truncated() -> Element {
    let turns = vec![
        UiAgentTurn::User {
            text: "Replace the shader with a bouncing ball simulation".to_string(),
        },
        UiAgentTurn::Tool(UiAgentToolRow {
            note: Some("write the bouncing-ball shader".to_string()),
            done: true,
            staged: false,
            shader_ok: None,
            probes: 0,
            error: Some("cut off by the output-token limit".to_string()),
            detail: String::new(),
            ..done_tool_row()
        }),
        UiAgentTurn::Notice {
            text: "Run stopped: the response hit the output-token limit while writing the \
                   edit — try again or ask for something smaller."
                .to_string(),
            level: UiNoticeLevel::Warning,
        },
    ];
    rsx! {
        ChatStoryCard { view: agent_fixture(UiAgentStatus::Idle, turns) }
    }
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
    description = "Thinking streams live: the dim strip under the pulsing 'Thinking…' label shows the model's reasoning text as it arrives, before any visible reply."
)]
fn thinking_streaming() -> Element {
    let turns = vec![
        UiAgentTurn::User {
            text: "Why does the center flicker?".to_string(),
        },
        UiAgentTurn::Thinking {
            text: "The flicker is probably aliasing near length(pos - 0.5) ≈ 0 — \
                   the ring frequency of 40.0 exceeds what a 32-pixel grid can \
                   sample. I should check whether smoothing the sin term or"
                .to_string(),
            done: false,
        },
    ];
    rsx! {
        ChatStoryCard { view: agent_fixture(UiAgentStatus::Streaming, turns) }
    }
}

#[story(
    description = "Thinking collapsed after the turn: a one-line 'Thought for a bit' expander sits above the reply; clicking it reveals the retained reasoning text."
)]
fn thinking_collapsed() -> Element {
    let turns = vec![
        UiAgentTurn::User {
            text: "Why does the center flicker?".to_string(),
        },
        UiAgentTurn::Thinking {
            text: "The flicker is aliasing near the center: ring frequency 40.0 \
                   outruns the fixture's sampling density, so adjacent LEDs land \
                   on opposite phases of the sin."
                .to_string(),
            done: true,
        },
        UiAgentTurn::Assistant {
            text: "The center flickers because the ring frequency is too high for \
                   the LED density there — I can smooth it with a radial falloff."
                .to_string(),
        },
    ];
    rsx! {
        ChatStoryCard { view: agent_fixture(UiAgentStatus::Idle, turns) }
    }
}

#[story(
    description = "Mid-tool live progress: the running row carries the model's note plus the current phase (probe 2/5) from ToolProgress events, with the working dot pulsing."
)]
fn tool_running_with_phase() -> Element {
    let mut running = UiAgentToolRow::started("tu_2");
    running.note = Some("sweep the ring speed".to_string());
    running.phase = Some("probe 2/5".to_string());
    let turns = vec![
        UiAgentTurn::User {
            text: "Try a few ring speeds and keep the calmest".to_string(),
        },
        UiAgentTurn::Assistant {
            text: "Sweeping speed over four values.".to_string(),
        },
        UiAgentTurn::Tool(running),
    ];
    rsx! {
        ChatStoryCard { view: agent_fixture(UiAgentStatus::RunningTool, turns) }
    }
}

#[story(
    description = "Mid-tool live progress while the engine verdict is awaited: the running row reads 'waiting for engine' after the staged edit reached the live project."
)]
fn tool_running_awaiting_engine() -> Element {
    let mut running = UiAgentToolRow::started("tu_3");
    running.note = Some("stage the warmer palette".to_string());
    running.phase = Some("waiting for engine".to_string());
    let turns = vec![
        UiAgentTurn::User {
            text: "Warm the palette up".to_string(),
        },
        UiAgentTurn::Tool(running),
    ];
    rsx! {
        ChatStoryCard { view: agent_fixture(UiAgentStatus::RunningTool, turns) }
    }
}

/// A deterministic 32×32 thumb: diagonal color bands, hue seeded per edit
/// so the filmstrip's chips read as distinct looks.
fn history_thumb(seed: u8) -> UiProductPreview {
    let (width, height) = (32u32, 32u32);
    let mut bytes = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let band = ((x + y) / 8) as u8;
            bytes.push(
                40u8.wrapping_add(seed.wrapping_mul(70))
                    .wrapping_add(band * 18),
            );
            bytes.push(
                180u8
                    .wrapping_sub(seed.wrapping_mul(50))
                    .wrapping_sub(band * 12),
            );
            bytes.push(90u8.wrapping_add(band * 24));
        }
    }
    UiProductPreview::VisualSrgb8 {
        width,
        height,
        revision: i64::from(seed),
        bytes: Rc::from(bytes.as_slice()),
    }
}

/// Three staged edits: two verified ok with thumbs, the middle one an
/// engine error (no thumb, error dot).
fn history_entries() -> Vec<UiAgentHistoryEntry> {
    vec![
        UiAgentHistoryEntry {
            turn: 1,
            note: Some("slow the rings down".to_string()),
            thumb: Some(history_thumb(0)),
            engine_ok: Some(true),
        },
        UiAgentHistoryEntry {
            turn: 2,
            note: Some("add a speed uniform".to_string()),
            thumb: None,
            engine_ok: Some(false),
        },
        UiAgentHistoryEntry {
            turn: 3,
            note: Some("warm the palette".to_string()),
            thumb: Some(history_thumb(2)),
            engine_ok: Some(true),
        },
    ]
}

#[story(
    description = "Edit history filmstrip above the composer: thumb chips for verified edits (good dot), a numbered placeholder chip for the engine-errored edit (error dot), and the dropped-count label; each chip is a one-click revert."
)]
fn history_with_thumbs() -> Element {
    let mut view = agent_fixture(UiAgentStatus::Idle, idle_transcript());
    view.history = history_entries();
    view.history_dropped = 2;
    rsx! {
        ChatStoryCard { view }
    }
}

#[story(
    description = "After a revert: the transcript carries the 'Reverted to turn 1' notice while the filmstrip keeps every record for further hops."
)]
fn history_reverted() -> Element {
    let mut turns = idle_transcript();
    turns.push(UiAgentTurn::Notice {
        text: "Reverted to turn 1 — that edit's source is staged again (Save keeps it)."
            .to_string(),
        level: UiNoticeLevel::Info,
    });
    let mut view = agent_fixture(UiAgentStatus::Idle, turns);
    view.history = history_entries();
    rsx! {
        ChatStoryCard { view }
    }
}

#[story(
    description = "No staged edits yet: the history strip stays absent entirely — the composer sits directly under the transcript."
)]
fn history_empty() -> Element {
    rsx! {
        ChatStoryCard { view: agent_fixture(UiAgentStatus::Idle, idle_transcript()) }
    }
}

#[story(
    description = "Staged-edit tool rows carry their snapshot inline in the transcript: the 32-px thumb at the right edge of the verified row, a dim numbered placeholder on the engine-errored one; the filmstrip below stays for one-click reverts."
)]
fn tool_row_inline_thumb() -> Element {
    let mut errored = done_tool_row();
    errored.id = "tu_2".to_string();
    errored.note = Some("add a speed uniform".to_string());
    errored.shader_ok = Some(false);
    errored.edit_turn = Some(2);
    errored.detail =
        "{\n  \"note\": \"add a speed uniform\",\n  \"shader_ok\": false,\n  \"staged\": true\n}"
            .to_string();
    let turns = vec![
        UiAgentTurn::User {
            text: "Slow the rings, then add a speed uniform".to_string(),
        },
        UiAgentTurn::Tool(done_tool_row()),
        UiAgentTurn::Tool(errored),
        UiAgentTurn::Assistant {
            text: "The slowdown is staged and verified; the uniform edit hit a compile error, \
                   so I left the working version staged."
                .to_string(),
        },
    ];
    let mut view = agent_fixture(UiAgentStatus::Idle, turns);
    view.history = history_entries();
    rsx! {
        ChatStoryCard { view }
    }
}

#[story(
    description = "Model chip populated: the footnote's compact selector carries the provider's fetched model list (display names), with the session's model selected; switching applies to the next run."
)]
fn model_chip_populated() -> Element {
    let mut view = agent_fixture(UiAgentStatus::Idle, idle_transcript());
    view.model = UiAgentModelView {
        effective: Some("claude-sonnet-5".to_string()),
        options: vec![
            UiModelOption {
                id: "claude-sonnet-5".to_string(),
                label: Some("Claude Sonnet 5".to_string()),
            },
            UiModelOption {
                id: "claude-haiku-4".to_string(),
                label: Some("Claude Haiku 4".to_string()),
            },
            UiModelOption {
                id: "claude-opus-5".to_string(),
                label: Some("Claude Opus 5".to_string()),
            },
        ],
        loading: false,
    };
    rsx! {
        ChatStoryCard { view }
    }
}

#[story(
    description = "Model chip fallback label: no fetched list (custom/local server) — the chip still names the session's model; custom ids stay a Settings affair."
)]
fn model_chip_fallback_label() -> Element {
    let mut view = agent_fixture(UiAgentStatus::Idle, idle_transcript());
    view.model = UiAgentModelView {
        effective: Some("qwen3-coder:30b".to_string()),
        options: Vec::new(),
        loading: false,
    };
    rsx! {
        ChatStoryCard { view }
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
            level: UiNoticeLevel::Info,
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
