//! Stories for the Studio settings popover content.
//!
//! `AgentSettingsSection` is pure, so these fixtures exercise the provider
//! selection states (per-provider fields + guidance) and the layering
//! states (defaults only, host-provided, user overrides) without a store
//! or network fetch.

use dioxus::prelude::*;
use lpa_studio_core::app::settings::local_model_probe::{self as probe, ProbeOutcome};
use lpa_studio_core::app::settings::model_catalog::{CatalogModel, CatalogPrice, ModelCatalog};
use lpa_studio_core::{
    AgentProvider, BrowserFacts, LocalModelProbeState, ModelCatalogState, SettingsLayer,
    UiAgentSettingsView, UiSettingsView, provider_guidance,
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
    description = "The model picker over OpenRouter's catalog: published $/MTok rates per row, a filter box because the list is long, and the configured model marked as chosen."
)]
pub(crate) fn model_picker_openrouter() -> Element {
    let mut agent = openrouter_agent();
    agent.model_override = Some("anthropic/claude-sonnet-5".to_string());
    agent.model_placeholder = "anthropic/claude-sonnet-5".to_string();
    catalog_panel(
        agent,
        ModelCatalogState {
            open: true,
            loading: false,
            error: None,
            catalog: Some(ModelCatalog {
                models: vec![
                    priced(
                        "anthropic/claude-opus-4-8",
                        "Anthropic: Claude Opus 4.8",
                        5.0,
                        25.0,
                    ),
                    priced(
                        "anthropic/claude-sonnet-5",
                        "Anthropic: Claude Sonnet 5",
                        3.0,
                        15.0,
                    ),
                    priced("google/gemini-3-pro", "Google: Gemini 3 Pro", 1.25, 10.0),
                    priced(
                        "meta-llama/llama-3.3-70b-instruct",
                        "Llama 3.3 70B",
                        0.12,
                        0.3,
                    ),
                    priced("openai/gpt-5", "OpenAI: GPT-5", 1.25, 10.0),
                    priced("qwen/qwen3-coder-30b", "Qwen3 Coder 30B", 0.07, 0.28),
                    priced("z-ai/glm-5", "Z.AI: GLM-5", 0.6, 2.2),
                    priced("deepseek/deepseek-v4", "DeepSeek V4", 0.28, 1.1),
                    priced("mistralai/mistral-large-3", "Mistral Large 3", 2.0, 6.0),
                ],
                hidden: 14,
            }),
            loaded_for: Some("OpenRouter|".to_string()),
        },
    )
}

#[story(
    description = "The picker over a local server's short list: no filter box needed, no prices to show, and one non-chat model (an embedding model) accounted for underneath."
)]
pub(crate) fn model_picker_local() -> Element {
    let mut agent = custom_agent();
    agent.model_override = Some("qwen3-coder:30b".to_string());
    agent.model_placeholder = "qwen3-coder:30b".to_string();
    agent.model_missing = false;
    catalog_panel(
        agent,
        ModelCatalogState {
            open: true,
            loading: false,
            error: None,
            catalog: Some(ModelCatalog {
                models: vec![
                    plain("llama3.2"),
                    plain("qwen3-coder:30b"),
                    plain("qwen3.5:9b"),
                ],
                hidden: 1,
            }),
            loaded_for: Some("Custom|http://localhost:11434/v1".to_string()),
        },
    )
}

#[story(
    description = "The picker with nothing to show yet: the provider needs a credential before it can be asked, so the reason replaces the list and Try again is the only affordance."
)]
pub(crate) fn model_picker_needs_key() -> Element {
    let mut agent = UiSettingsView::default().agent;
    agent.provider = AgentProvider::OpenAi;
    agent.provider_overridden = true;
    agent.provider_layer = SettingsLayer::User;
    agent.guidance = provider_guidance(AgentProvider::OpenAi);
    agent.model_default = None;
    agent.model_placeholder = "model id from your provider — see its docs".to_string();
    agent.model_missing = true;
    catalog_panel(
        agent,
        ModelCatalogState {
            open: true,
            loading: false,
            error: Some("Add your OpenAI API key first, then browse models.".to_string()),
            catalog: None,
            loaded_for: None,
        },
    )
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

/// A panel with the model picker expanded.
fn catalog_panel(agent: UiAgentSettingsView, catalog: ModelCatalogState) -> Element {
    rsx! {
        div { class: "tw:w-[340px] tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card",
            AgentSettingsSection { agent, on_settings: move |_| {}, catalog }
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

/// A catalog row with published rates ($/MTok).
fn priced(id: &str, label: &str, input: f64, output: f64) -> CatalogModel {
    CatalogModel {
        id: id.to_string(),
        label: Some(label.to_string()),
        price: Some(CatalogPrice {
            input_per_mtok: input,
            output_per_mtok: output,
        }),
    }
}

/// A catalog row from a server that publishes no names or prices.
fn plain(id: &str) -> CatalogModel {
    CatalogModel {
        id: id.to_string(),
        label: None,
        price: None,
    }
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
