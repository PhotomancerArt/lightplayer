//! The settings slice of the studio view: everything the settings UI
//! renders, precomputed here so the view constructs no domain state.

use crate::app::settings::agent_provider::{
    AgentProvider, AgentProviderGuidance, provider_guidance,
};
use crate::app::settings::settings_layer::SettingsLayer;
use crate::app::settings::studio_settings::DEFAULT_AGENT_MODEL;

/// The settings view DTO carried on
/// [`UiStudioView`](crate::UiStudioView).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSettingsView {
    pub agent: UiAgentSettingsView,
}

impl Default for UiSettingsView {
    /// The no-layers view (baked defaults only) — what the shell renders
    /// before the store's first snapshot arrives.
    fn default() -> Self {
        Self {
            agent: UiAgentSettingsView {
                provider: AgentProvider::Anthropic,
                provider_layer: SettingsLayer::Default,
                provider_overridden: false,
                guidance: provider_guidance(AgentProvider::Anthropic),
                api_key_masked: None,
                api_key_layer: SettingsLayer::Default,
                api_key_overridden: false,
                api_key_optional: false,
                base_url_effective: None,
                base_url_override: None,
                base_url_layer: SettingsLayer::Default,
                model_placeholder: DEFAULT_AGENT_MODEL.to_string(),
                model_default: Some(DEFAULT_AGENT_MODEL.to_string()),
                model_effective: Some(DEFAULT_AGENT_MODEL.to_string()),
                model_override: None,
                model_layer: SettingsLayer::Default,
                model_missing: false,
                model_options: Vec::new(),
                models_loading: false,
                models_error: None,
                price_input_override: None,
                price_output_override: None,
            },
        }
    }
}

/// The agent section of the settings UI. Every field describes the
/// SELECTED provider (the store re-derives the whole view on a provider
/// switch).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAgentSettingsView {
    /// The selected provider (drives which fields the popover shows).
    pub provider: AgentProvider,
    /// Which layer supplies the selection (`Default` = baked Anthropic).
    pub provider_layer: SettingsLayer,
    /// A user provider selection exists (drives "use default provider").
    pub provider_overridden: bool,
    /// Onboarding copy for the selected provider (shared with the Agent
    /// tab's needs-setup state — the copy lives in core, once).
    pub guidance: AgentProviderGuidance,
    /// Masked preview of the selected provider's effective API key (never
    /// the raw key), or `None` while no layer supplies one.
    pub api_key_masked: Option<String>,
    /// Which layer supplies the effective key (`Default` = unset).
    pub api_key_layer: SettingsLayer,
    /// A user override for the key exists (drives "clear saved key").
    pub api_key_overridden: bool,
    /// The key is optional for this provider (Custom: local servers).
    pub api_key_optional: bool,
    /// The effective custom-server base URL (`Some` only while the Custom
    /// provider is selected and a layer supplies one).
    pub base_url_effective: Option<String>,
    /// The user's base-URL override — the input's value.
    pub base_url_override: Option<String>,
    /// Which layer supplies the effective base URL.
    pub base_url_layer: SettingsLayer,
    /// The model input's placeholder: the effective model when one
    /// resolves, or the "model id from your provider" hint.
    pub model_placeholder: String,
    /// The baked default model id, when the provider has one (Anthropic
    /// only) — for "Use default (…)" copy.
    pub model_default: Option<String>,
    /// The effective model id when one resolves (any layer or the baked
    /// default) — what the discovery dropdown selects.
    pub model_effective: Option<String>,
    /// The user's model override, when one exists — the input's value.
    pub model_override: Option<String>,
    /// Which layer supplies the effective model id.
    pub model_layer: SettingsLayer,
    /// The provider requires a model id and none is configured (OpenAI /
    /// Custom before onboarding completes).
    pub model_missing: bool,
    /// Discovered models for the dropdown (P8), in server order. Empty ⇒
    /// the model field renders as free text.
    pub model_options: Vec<UiModelOption>,
    /// A `/models` fetch is in flight for the selected provider.
    pub models_loading: bool,
    /// Display copy for a failed fetch (mapped in core; the guidance block
    /// above the field carries the per-provider remedies).
    pub models_error: Option<String>,
    /// The user's $/MTok input-rate override, formatted for its input.
    pub price_input_override: Option<String>,
    /// The user's $/MTok output-rate override, formatted for its input.
    pub price_output_override: Option<String>,
}

/// One entry of the model discovery dropdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiModelOption {
    /// The id the provider expects (the option's value).
    pub id: String,
    /// Human-readable name when the provider supplies one (the option's
    /// text; the id renders otherwise).
    pub label: Option<String>,
    /// The published facts that put this option where it is — its coding
    /// score and rates, when the provider publishes them (`code 78 · $5/$25`).
    /// Rendered after the name so the ordering explains itself.
    pub detail: Option<String>,
}

/// Mask an API key for display: enough of the tail to recognize which key
/// it is, never enough to reconstruct it. Short keys mask entirely.
pub(crate) fn masked_key_preview(key: &str) -> String {
    const TAIL: usize = 4;
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= TAIL * 3 {
        return "•••••".to_string();
    }
    let tail: String = chars[chars.len() - TAIL..].iter().collect();
    format!("•••••{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_preview_keeps_only_the_tail() {
        let masked = masked_key_preview("sk-ant-api03-abcdefghijklmnop");
        assert_eq!(masked, "•••••mnop");
    }

    #[test]
    fn short_keys_mask_entirely() {
        assert_eq!(masked_key_preview("sk-test"), "•••••");
        assert_eq!(masked_key_preview(""), "•••••");
    }

    #[test]
    fn default_view_is_the_anthropic_baseline() {
        let view = UiSettingsView::default().agent;
        assert_eq!(view.provider, AgentProvider::Anthropic);
        assert_eq!(view.guidance.provider, AgentProvider::Anthropic);
        assert!(!view.api_key_optional);
        assert!(!view.model_missing);
        assert_eq!(view.model_placeholder, DEFAULT_AGENT_MODEL);
    }
}
