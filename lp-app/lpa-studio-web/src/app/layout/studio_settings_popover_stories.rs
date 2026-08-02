//! Stories for the AI settings popover content.
//!
//! `AgentSettingsSection` is pure, so these fixtures exercise the provider
//! selection states (per-provider fields + guidance) and the layering
//! states (defaults only, host-provided, user overrides) without a store
//! or network fetch.

use dioxus::prelude::*;
use lpa_studio_core::app::settings::local_model_probe::{self as probe, ProbeOutcome};
use lpa_studio_core::{
    AgentProvider, BrowserFacts, LocalModelProbeState, SettingsLayer, UiAgentSettingsView,
    UiModelOption, UiSettingsView, provider_guidance,
};
use lpa_studio_web_story_macros::story;

use crate::app::layout::studio_settings_popover::AgentSettingsSection;

#[story(
    description = "No host layer, nothing saved: Anthropic selected by default with its onboarding guidance, empty key, default model placeholder."
)]
pub(crate) fn defaults_only() -> Element {
    panel(UiSettingsView::default().agent)
}

#[story(description = "Key and model supplied by dev settings, with provenance hints.")]
pub(crate) fn host_provided() -> Element {
    let mut agent = UiSettingsView::default().agent;
    agent.api_key_masked = Some("•••••h3Fk".to_string());
    agent.api_key_layer = SettingsLayer::Host;
    agent.model_placeholder = "claude-opus-5".to_string();
    agent.model_layer = SettingsLayer::Host;
    panel(agent)
}

#[story(
    description = "User overrides riding over a host-provided key: clear affordances shown, plus cost-rate overrides."
)]
pub(crate) fn user_overrides() -> Element {
    let mut agent = UiSettingsView::default().agent;
    agent.api_key_masked = Some("•••••q8Zw".to_string());
    agent.api_key_layer = SettingsLayer::User;
    agent.api_key_overridden = true;
    agent.model_placeholder = "claude-haiku-4-5".to_string();
    agent.model_override = Some("claude-haiku-4-5".to_string());
    agent.model_layer = SettingsLayer::User;
    agent.price_input_override = Some("1".to_string());
    agent.price_output_override = Some("5".to_string());
    panel(agent)
}

#[story(
    description = "OpenAI selected before onboarding completes: billing guidance, empty key, and the required-model warning (no default model id is guessed)."
)]
pub(crate) fn openai_needs_setup() -> Element {
    let mut agent = UiSettingsView::default().agent;
    agent.provider = AgentProvider::OpenAi;
    agent.provider_overridden = true;
    agent.provider_layer = SettingsLayer::User;
    agent.guidance = provider_guidance(AgentProvider::OpenAi);
    agent.model_default = None;
    agent.model_placeholder = "model id from your provider — see its docs".to_string();
    agent.model_missing = true;
    panel(agent)
}

#[story(
    description = "Custom provider configured for a local Ollama: base URL field, optional key, CORS note in the guidance."
)]
pub(crate) fn custom_local_server() -> Element {
    let mut agent = UiSettingsView::default().agent;
    agent.provider = AgentProvider::Custom;
    agent.provider_overridden = true;
    agent.provider_layer = SettingsLayer::User;
    agent.guidance = provider_guidance(AgentProvider::Custom);
    agent.api_key_optional = true;
    agent.base_url_effective = Some("http://localhost:11434/v1".to_string());
    agent.base_url_override = Some("http://localhost:11434/v1".to_string());
    agent.base_url_layer = SettingsLayer::User;
    agent.model_default = None;
    agent.model_override = Some("llama3.2".to_string());
    agent.model_placeholder = "llama3.2".to_string();
    agent.model_layer = SettingsLayer::User;
    panel(agent)
}

#[story(
    description = "A scan that found a local Ollama: the summary leads, and each served model id is a one-click adopt (address + model together)."
)]
pub(crate) fn custom_scan_found_a_server() -> Element {
    let findings = vec![diagnosed(
        "http://localhost:11434/v1",
        ProbeOutcome::Models(vec![
            "qwen3-coder:30b".to_string(),
            "qwen3.5:9b".to_string(),
            "llama3.2".to_string(),
        ]),
    )];
    probe_panel(LocalModelProbeState {
        running: false,
        running_label: None,
        summary: Some(probe::scan_summary(&findings, &story_facts())),
        findings,
    })
}

#[story(
    description = "The CORS case: the server answered but the browser dropped the response. Reported as reachable, with the exact copy-pasteable fix for the recognized server, plus a dead port for contrast."
)]
pub(crate) fn custom_scan_blocked_by_cors() -> Element {
    let findings = vec![
        diagnosed("http://localhost:11434/v1", ProbeOutcome::CorsBlocked),
        diagnosed(
            "http://localhost:1234/v1",
            ProbeOutcome::Status {
                status: 401,
                body: r#"{"error":{"message":"API key required"}}"#.to_string(),
            },
        ),
    ];
    probe_panel(LocalModelProbeState {
        running: false,
        running_label: None,
        summary: Some(probe::scan_summary(&findings, &story_facts())),
        findings,
    })
}

#[story(
    description = "A scan that found nothing: every common port silent, with the browser-policy hint the summary adds when a page is served over https."
)]
pub(crate) fn custom_scan_found_nothing() -> Element {
    let findings: Vec<_> = probe::COMMON_LOCAL_SERVERS
        .iter()
        .map(|server| {
            diagnosed(
                server.base_url,
                ProbeOutcome::Unreachable {
                    detail: "TypeError: Failed to fetch".to_string(),
                },
            )
        })
        .collect();
    probe_panel(LocalModelProbeState {
        running: false,
        running_label: None,
        summary: Some(probe::scan_summary(&findings, &story_facts())),
        // Dead ports are dropped from the list; the summary counts them.
        findings: Vec::new(),
    })
}

#[story(
    description = "A scan in flight: both probe buttons disabled while the working-status line names what is being tried."
)]
pub(crate) fn custom_scan_running() -> Element {
    probe_panel(crate::local_model_probe::running_state(
        &crate::local_model_probe::ProbeRequest::ScanCommonPorts,
    ))
}

#[story(
    description = "OpenRouter selected, not yet connected: the one-click Connect button replaces the key field, with a sample exchange-failure warning underneath."
)]
pub(crate) fn openrouter_needs_connect() -> Element {
    let mut agent = openrouter_agent();
    agent.api_key_masked = None;
    rsx! {
        div { class: "tw:w-[340px] tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card",
            AgentSettingsSection {
                agent,
                on_settings: move |_| {},
                connect_error: Some("Connect failed: OpenRouter rejected the exchange (HTTP 403)".to_string()),
            }
        }
    }
}

#[story(
    description = "OpenRouter connected: masked key with the Disconnect affordance and the baked default model."
)]
pub(crate) fn openrouter_connected() -> Element {
    panel(openrouter_agent())
}

#[story(
    description = "Model discovery populated (P8): the model field is a dropdown over the fetched /models ids with display names, the override selecting one of them; Custom… is the free-text escape."
)]
pub(crate) fn model_dropdown_populated() -> Element {
    let mut agent = UiSettingsView::default().agent;
    agent.api_key_masked = Some("•••••h3Fk".to_string());
    agent.api_key_layer = SettingsLayer::User;
    agent.api_key_overridden = true;
    agent.model_options = vec![
        UiModelOption {
            id: "claude-sonnet-5".to_string(),
            label: Some("Claude Sonnet 5".to_string()),
            detail: None,
        },
        UiModelOption {
            id: "claude-opus-5".to_string(),
            label: Some("Claude Opus 5".to_string()),
            detail: None,
        },
        UiModelOption {
            id: "claude-haiku-4-5".to_string(),
            label: Some("Claude Haiku 4.5".to_string()),
            detail: None,
        },
    ];
    agent.model_override = Some("claude-opus-5".to_string());
    agent.model_layer = SettingsLayer::User;
    panel(agent)
}

#[story(
    description = "Model discovery failed (P8): a local server fetch error keeps the free-text field, with the mapped error line pointing back at the guidance's CORS note."
)]
pub(crate) fn model_fetch_error() -> Element {
    let mut agent = UiSettingsView::default().agent;
    agent.provider = AgentProvider::Custom;
    agent.provider_overridden = true;
    agent.provider_layer = SettingsLayer::User;
    agent.guidance = provider_guidance(AgentProvider::Custom);
    agent.api_key_optional = true;
    agent.base_url_effective = Some("http://localhost:11434/v1".to_string());
    agent.base_url_override = Some("http://localhost:11434/v1".to_string());
    agent.base_url_layer = SettingsLayer::User;
    agent.model_default = None;
    agent.model_effective = Some("llama3.2".to_string());
    agent.model_override = Some("llama3.2".to_string());
    agent.model_placeholder = "llama3.2".to_string();
    agent.model_layer = SettingsLayer::User;
    agent.models_error = Some(
        "model list unavailable — server unreachable (check the base URL and the CORS note above)"
            .to_string(),
    );
    panel(agent)
}

#[story(
    description = "Model discovery with a custom entry (P8): the fetched list is present but the override is a hand-typed id outside it, so the dropdown sits on Custom… with the text input open underneath."
)]
pub(crate) fn model_custom_entry() -> Element {
    let mut agent = UiSettingsView::default().agent;
    agent.provider = AgentProvider::OpenAi;
    agent.provider_overridden = true;
    agent.provider_layer = SettingsLayer::User;
    agent.guidance = provider_guidance(AgentProvider::OpenAi);
    agent.api_key_masked = Some("•••••q8Zw".to_string());
    agent.api_key_layer = SettingsLayer::User;
    agent.api_key_overridden = true;
    agent.model_options = vec![
        UiModelOption {
            id: "gpt-5.2".to_string(),
            label: None,
            detail: None,
        },
        UiModelOption {
            id: "gpt-5.2-mini".to_string(),
            label: None,
            detail: None,
        },
    ];
    agent.model_default = None;
    agent.model_effective = Some("ft:gpt-5.2:acme:leds:9k3".to_string());
    agent.model_override = Some("ft:gpt-5.2:acme:leds:9k3".to_string());
    agent.model_placeholder = "ft:gpt-5.2:acme:leds:9k3".to_string();
    agent.model_layer = SettingsLayer::User;
    panel(agent)
}

/// The OpenRouter fixture base: connected unless a story clears the key.
fn openrouter_agent() -> UiAgentSettingsView {
    let mut agent = UiSettingsView::default().agent;
    agent.provider = AgentProvider::OpenRouter;
    agent.provider_overridden = true;
    agent.provider_layer = SettingsLayer::User;
    agent.guidance = provider_guidance(AgentProvider::OpenRouter);
    agent.api_key_masked = Some("•••••r7Kp".to_string());
    agent.api_key_layer = SettingsLayer::User;
    agent.api_key_overridden = true;
    agent.model_default = Some("anthropic/claude-sonnet-5".to_string());
    agent.model_placeholder = "anthropic/claude-sonnet-5".to_string();
    agent
}

fn panel(agent: UiAgentSettingsView) -> Element {
    rsx! {
        div { class: "tw:w-[340px] tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card",
            AgentSettingsSection { agent, on_settings: move |_| {} }
        }
    }
}

/// The Custom-provider panel carrying a discovery result.
fn probe_panel(probe: LocalModelProbeState) -> Element {
    let agent = custom_agent();
    rsx! {
        div { class: "tw:w-[340px] tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card",
            AgentSettingsSection { agent, on_settings: move |_| {}, probe }
        }
    }
}

/// The Custom-provider fixture base: local server selected, nothing chosen.
fn custom_agent() -> UiAgentSettingsView {
    let mut agent = UiSettingsView::default().agent;
    agent.provider = AgentProvider::Custom;
    agent.provider_overridden = true;
    agent.provider_layer = SettingsLayer::User;
    agent.guidance = provider_guidance(AgentProvider::Custom);
    agent.api_key_optional = true;
    agent.model_default = None;
    agent.model_placeholder = "model id from your provider — see its docs".to_string();
    agent.model_missing = true;
    agent
}

/// One finding, diagnosed through the same core path the browser glue uses.
fn diagnosed(base_url: &str, outcome: ProbeOutcome) -> probe::ProbeFinding {
    probe::diagnose(base_url, outcome, &story_facts(), None)
}

/// The deployed-site case these findings describe: an https page reaching
/// for plain-http localhost.
fn story_facts() -> BrowserFacts {
    BrowserFacts {
        page_origin: "https://lightplayer.app".to_string(),
        page_is_https: true,
        is_safari: false,
    }
}
