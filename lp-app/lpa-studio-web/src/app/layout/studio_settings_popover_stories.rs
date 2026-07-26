//! Stories for the Studio settings popover content.
//!
//! `AgentSettingsSection` is pure, so these fixtures exercise the provider
//! selection states (per-provider fields + guidance) and the layering
//! states (defaults only, host-provided, user overrides) without a store
//! or network fetch.

use dioxus::prelude::*;
use lpa_studio_core::{
    AgentProvider, SettingsLayer, UiAgentSettingsView, UiSettingsView, provider_guidance,
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

fn panel(agent: UiAgentSettingsView) -> Element {
    rsx! {
        div { class: "tw:w-[340px] tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card",
            AgentSettingsSection { agent, on_settings: move |_| {} }
        }
    }
}
