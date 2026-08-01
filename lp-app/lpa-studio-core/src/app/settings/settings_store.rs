//! The pure layered settings store: per-field merge + provenance.

use lpa_agent::provider::openai_compat::DEFAULT_BASE_URL as OPENAI_DEFAULT_BASE_URL;
use lpa_agent::{AnthropicConfig, ListModelsError, ModelInfo, OpenAiCompatConfig};

use crate::app::agent::agent_pricing::AgentCostRates;
use crate::app::agent::agent_provider_config::AgentProviderConfig;
use crate::app::settings::agent_models::{AgentModelsFetch, AgentModelsState, models_error_copy};
use crate::app::settings::agent_provider::{AgentProvider, provider_guidance};
use crate::app::settings::settings_layer::SettingsLayer;
use crate::app::settings::studio_settings::{
    DEFAULT_AGENT_MODEL, DEFAULT_OPENROUTER_MODEL, OPENROUTER_BASE_URL, StudioSettings,
};
use crate::app::settings::ui_settings_view::{
    UiAgentSettingsView, UiModelOption, UiSettingsView, masked_key_preview,
};

/// Two overlays over the baked defaults, merged per-field with
/// **user > host > default** precedence. Pure state (sans-IO): the platform
/// edges load the layers and persist the user layer; the store only merges.
/// Also carries the transient per-provider discovered-model state (P8) —
/// never persisted, fed by the controller's spawned fetches.
#[derive(Clone, Debug, Default)]
pub struct SettingsStore {
    host: StudioSettings,
    user: StudioSettings,
    /// Per-provider `/models` fetch state (at most one entry per provider;
    /// linear scan — four providers exist).
    models: Vec<(AgentProvider, AgentModelsState)>,
}

impl SettingsStore {
    /// Install the host-provided layer (the `dev-settings.json` document, or
    /// a future Electron preload's). Replaces any previous host layer.
    pub fn set_host_layer(&mut self, settings: StudioSettings) {
        self.host = normalized_settings(settings);
    }

    /// Install the persisted user layer wholesale (the boot localStorage
    /// read). Replaces any previous user layer.
    pub fn set_user_layer(&mut self, settings: StudioSettings) {
        self.user = normalized_settings(settings);
    }

    /// The user layer, for persistence. Only this layer is ever written
    /// back; host and default layers are never the user's to save.
    pub fn user_layer(&self) -> &StudioSettings {
        &self.user
    }

    // -- user-override setters --------------------------------------------

    /// Set or clear the user's provider selection.
    pub fn set_agent_provider(&mut self, provider: Option<AgentProvider>) {
        self.user.agent.provider = provider;
    }

    /// Set or clear the user's Anthropic API-key override (trimmed; empty ⇒
    /// clear).
    pub fn set_agent_anthropic_api_key(&mut self, key: Option<String>) {
        self.user.agent.anthropic_api_key = normalized(key);
    }

    /// Set or clear the user's OpenAI API-key override.
    pub fn set_agent_openai_api_key(&mut self, key: Option<String>) {
        self.user.agent.openai_api_key = normalized(key);
    }

    /// Set or clear the user's custom-server base URL override.
    pub fn set_agent_custom_base_url(&mut self, base_url: Option<String>) {
        self.user.agent.custom_base_url = normalized(base_url);
    }

    /// Set or clear the user's custom-server API-key override.
    pub fn set_agent_custom_api_key(&mut self, key: Option<String>) {
        self.user.agent.custom_api_key = normalized(key);
    }

    /// Set or clear the OpenRouter key (written by the Connect flow's
    /// exchange, cleared by Disconnect).
    pub fn set_agent_openrouter_api_key(&mut self, key: Option<String>) {
        self.user.agent.openrouter_api_key = normalized(key);
    }

    /// Set or clear the user's model override (trimmed; empty ⇒ clear).
    pub fn set_agent_model(&mut self, model: Option<String>) {
        self.user.agent.model = normalized(model);
    }

    /// Set or clear the input-rate override from its text-field value
    /// (unparseable or non-positive input clears).
    pub fn set_agent_price_input_per_mtok(&mut self, value: Option<String>) {
        self.user.agent.price_input_per_mtok = parsed_rate(value);
    }

    /// Set or clear the output-rate override from its text-field value.
    pub fn set_agent_price_output_per_mtok(&mut self, value: Option<String>) {
        self.user.agent.price_output_per_mtok = parsed_rate(value);
    }

    // -- effective values --------------------------------------------------

    /// The selected provider (user > host > Anthropic).
    pub fn agent_provider(&self) -> AgentProvider {
        self.user
            .agent
            .provider
            .or(self.host.agent.provider)
            .unwrap_or_default()
    }

    /// The effective Anthropic API key (user > host).
    pub fn agent_anthropic_api_key(&self) -> Option<&str> {
        self.user.agent.anthropic_api_key.as_deref().or(self
            .host
            .agent
            .anthropic_api_key
            .as_deref())
    }

    /// The effective API key for the SELECTED provider.
    pub fn agent_selected_api_key(&self) -> Option<&str> {
        match self.agent_provider() {
            AgentProvider::Anthropic => self.agent_anthropic_api_key(),
            AgentProvider::OpenAi => self.user.agent.openai_api_key.as_deref().or(self
                .host
                .agent
                .openai_api_key
                .as_deref()),
            AgentProvider::Custom => self.user.agent.custom_api_key.as_deref().or(self
                .host
                .agent
                .custom_api_key
                .as_deref()),
            AgentProvider::OpenRouter => self.user.agent.openrouter_api_key.as_deref().or(self
                .host
                .agent
                .openrouter_api_key
                .as_deref()),
        }
    }

    /// The effective custom-server base URL (user > host).
    pub fn agent_custom_base_url(&self) -> Option<&str> {
        self.user
            .agent
            .custom_base_url
            .as_deref()
            .or(self.host.agent.custom_base_url.as_deref())
    }

    /// The model override (user > host), without the Anthropic default.
    pub fn agent_model_override(&self) -> Option<&str> {
        self.user
            .agent
            .model
            .as_deref()
            .or(self.host.agent.model.as_deref())
    }

    /// The effective model for the selected provider: Anthropic and
    /// OpenRouter fall back to their baked defaults (so Connect alone makes
    /// the agent ready); OpenAI/Custom have no default (model ids are
    /// provider-specific and never guessed).
    pub fn agent_model(&self) -> Option<&str> {
        match self.agent_provider() {
            AgentProvider::Anthropic => {
                Some(self.agent_model_override().unwrap_or(DEFAULT_AGENT_MODEL))
            }
            AgentProvider::OpenRouter => Some(
                self.agent_model_override()
                    .unwrap_or(DEFAULT_OPENROUTER_MODEL),
            ),
            AgentProvider::OpenAi | AgentProvider::Custom => self.agent_model_override(),
        }
    }

    /// The readiness rule: the agent is available exactly when the selected
    /// provider resolves to a runnable config.
    pub fn agent_ready(&self) -> bool {
        self.agent_provider_config().is_some()
    }

    /// Resolve the selected provider into connection settings, or `None`
    /// while it is not sufficiently configured (Anthropic: key; OpenAI:
    /// key + model; Custom: base URL + model, key optional).
    pub fn agent_provider_config(&self) -> Option<AgentProviderConfig> {
        let model = self.agent_model()?.to_string();
        self.provider_config_with_model(model)
    }

    /// Connection settings for model DISCOVERY (P8): the same
    /// credential/endpoint resolution as [`Self::agent_provider_config`],
    /// but a missing model id does not block — discovery exists precisely
    /// to find one (the config's model field is blank then, unused by the
    /// listing GET).
    pub fn agent_discovery_config(&self) -> Option<AgentProviderConfig> {
        let model = self.agent_model().unwrap_or_default().to_string();
        self.provider_config_with_model(model)
    }

    fn provider_config_with_model(&self, model: String) -> Option<AgentProviderConfig> {
        match self.agent_provider() {
            AgentProvider::Anthropic => {
                let api_key = self.agent_anthropic_api_key()?;
                Some(AgentProviderConfig::Anthropic(AnthropicConfig {
                    api_key: api_key.to_string(),
                    model,
                    base_url: lpa_agent::provider::anthropic::DEFAULT_BASE_URL.to_string(),
                }))
            }
            AgentProvider::OpenAi => {
                let api_key = self.agent_selected_api_key()?;
                Some(AgentProviderConfig::OpenAiCompat(OpenAiCompatConfig {
                    base_url: OPENAI_DEFAULT_BASE_URL.to_string(),
                    api_key: Some(api_key.to_string()),
                    model,
                    extra_headers: Vec::new(),
                }))
            }
            AgentProvider::Custom => {
                let base_url = self.agent_custom_base_url()?;
                Some(AgentProviderConfig::OpenAiCompat(OpenAiCompatConfig {
                    base_url: base_url.to_string(),
                    api_key: self.agent_selected_api_key().map(str::to_string),
                    model,
                    extra_headers: Vec::new(),
                }))
            }
            AgentProvider::OpenRouter => {
                let api_key = self.agent_selected_api_key()?;
                Some(AgentProviderConfig::OpenAiCompat(OpenAiCompatConfig {
                    base_url: OPENROUTER_BASE_URL.to_string(),
                    api_key: Some(api_key.to_string()),
                    model,
                    // App attribution in OpenRouter's rankings; harmless
                    // elsewhere and never sent to other providers.
                    extra_headers: vec![
                        (
                            "HTTP-Referer".to_string(),
                            "https://lightplayer.app".to_string(),
                        ),
                        ("X-Title".to_string(), "LightPlayer Studio".to_string()),
                    ],
                }))
            }
        }
    }

    // -- discovered models (P8) --------------------------------------------

    /// Begin (or debounce) a model-list fetch for `provider` under
    /// `fingerprint`. Returns whether the caller should actually spawn the
    /// fetch: an entry already carrying this fingerprint — in flight, or
    /// resolved and not `force`d — debounces to `false`. A `true` return
    /// installs the `Loading` marker.
    pub fn request_agent_models(
        &mut self,
        provider: AgentProvider,
        fingerprint: String,
        force: bool,
    ) -> bool {
        if let Some(state) = self.agent_models(provider)
            && state.fingerprint == fingerprint
        {
            let in_flight = state.fetch == AgentModelsFetch::Loading;
            if in_flight || !force {
                return false;
            }
        }
        self.set_agent_models(
            provider,
            AgentModelsState {
                fingerprint,
                fetch: AgentModelsFetch::Loading,
            },
        );
        true
    }

    /// Land a fetch result. `fetched_at` is the controller clock's stamp.
    /// A result whose fingerprint no longer matches the stored entry —
    /// the credentials changed while it was in flight — is dropped.
    pub fn agent_models_loaded(
        &mut self,
        provider: AgentProvider,
        fingerprint: &str,
        result: Result<Vec<ModelInfo>, ListModelsError>,
        fetched_at: f64,
    ) {
        let Some(state) = self.agent_models(provider) else {
            return;
        };
        if state.fingerprint != fingerprint {
            return;
        }
        let fetch = match result {
            Ok(models) => AgentModelsFetch::Loaded { models, fetched_at },
            Err(error) => AgentModelsFetch::Failed { error },
        };
        self.set_agent_models(
            provider,
            AgentModelsState {
                fingerprint: fingerprint.to_string(),
                fetch,
            },
        );
    }

    /// Drop a provider's discovered-model state (its credentials became
    /// insufficient, or no platform fetcher exists to serve a request).
    pub fn clear_agent_models(&mut self, provider: AgentProvider) {
        self.models.retain(|(entry, _)| *entry != provider);
    }

    /// A provider's discovered-model state, when any fetch has run.
    pub fn agent_models(&self, provider: AgentProvider) -> Option<&AgentModelsState> {
        self.models
            .iter()
            .find(|(entry, _)| *entry == provider)
            .map(|(_, state)| state)
    }

    fn set_agent_models(&mut self, provider: AgentProvider, state: AgentModelsState) {
        match self.models.iter_mut().find(|(entry, _)| *entry == provider) {
            Some((_, existing)) => *existing = state,
            None => self.models.push((provider, state)),
        }
    }

    /// Cost rates for the usage estimate: per-field settings overrides win
    /// over the built-in table; both rates must resolve or there is no
    /// estimate (unknown model, no overrides ⇒ tokens only). Overrides
    /// stay two fields (input/output) — the cache-write/read rates always
    /// derive from the resolved input rate by the standard multipliers.
    pub fn agent_cost_rates(&self) -> Option<AgentCostRates> {
        let table = self.agent_model().and_then(AgentCostRates::for_model);
        let input = self
            .price_override(|agent| agent.price_input_per_mtok)
            .or(table.map(|rates| rates.input_per_mtok))?;
        let output = self
            .price_override(|agent| agent.price_output_per_mtok)
            .or(table.map(|rates| rates.output_per_mtok))?;
        Some(AgentCostRates::from_io(input, output))
    }

    fn price_override(
        &self,
        field: impl Fn(&crate::app::settings::AgentSettings) -> Option<f64>,
    ) -> Option<f64> {
        field(&self.user.agent).or(field(&self.host.agent))
    }

    // -- provenance ---------------------------------------------------------

    /// Which layer supplies a field, given its two overlay values.
    fn layer_of<T>(user: &Option<T>, host: &Option<T>) -> SettingsLayer {
        if user.is_some() {
            SettingsLayer::User
        } else if host.is_some() {
            SettingsLayer::Host
        } else {
            SettingsLayer::Default
        }
    }

    /// The view DTO the settings UI renders: effective values for the
    /// SELECTED provider (keys already masked here in core), per-field
    /// provenance, the user-override state driving "clear override"
    /// affordances, and the provider's onboarding guidance.
    pub fn ui_view(&self) -> UiSettingsView {
        let provider = self.agent_provider();
        let (key_user, key_host) = match provider {
            AgentProvider::Anthropic => (
                &self.user.agent.anthropic_api_key,
                &self.host.agent.anthropic_api_key,
            ),
            AgentProvider::OpenAi => (
                &self.user.agent.openai_api_key,
                &self.host.agent.openai_api_key,
            ),
            AgentProvider::Custom => (
                &self.user.agent.custom_api_key,
                &self.host.agent.custom_api_key,
            ),
            AgentProvider::OpenRouter => (
                &self.user.agent.openrouter_api_key,
                &self.host.agent.openrouter_api_key,
            ),
        };
        // The model input's placeholder: the effective model when one
        // resolves (default or lower-layer value), or the required-id hint
        // for providers without a default.
        let model_placeholder = match self.agent_model() {
            Some(model) => model.to_string(),
            None => "model id from your provider — see its docs".to_string(),
        };
        // The discovered-model slice (P8): options only from a Loaded
        // fetch; Loading and Failed render as flags on the free-text path.
        let (model_options, models_loading, models_error) =
            match self.agent_models(provider).map(|state| &state.fetch) {
                Some(AgentModelsFetch::Loading) => (Vec::new(), true, None),
                Some(AgentModelsFetch::Loaded { models, .. }) => (
                    models
                        .iter()
                        .map(|model| UiModelOption {
                            id: model.id.clone(),
                            label: model.display_name.clone(),
                        })
                        .collect(),
                    false,
                    None,
                ),
                Some(AgentModelsFetch::Failed { error }) => {
                    (Vec::new(), false, Some(models_error_copy(provider, error)))
                }
                None => (Vec::new(), false, None),
            };
        UiSettingsView {
            agent: UiAgentSettingsView {
                provider,
                provider_layer: Self::layer_of(
                    &self.user.agent.provider,
                    &self.host.agent.provider,
                ),
                provider_overridden: self.user.agent.provider.is_some(),
                guidance: provider_guidance(provider),
                api_key_masked: self.agent_selected_api_key().map(masked_key_preview),
                api_key_layer: Self::layer_of(key_user, key_host),
                api_key_overridden: key_user.is_some(),
                api_key_optional: provider == AgentProvider::Custom,
                base_url_effective: (provider == AgentProvider::Custom)
                    .then(|| self.agent_custom_base_url().map(str::to_string))
                    .flatten(),
                base_url_override: self.user.agent.custom_base_url.clone(),
                base_url_layer: Self::layer_of(
                    &self.user.agent.custom_base_url,
                    &self.host.agent.custom_base_url,
                ),
                model_placeholder,
                model_default: match provider {
                    AgentProvider::Anthropic => Some(DEFAULT_AGENT_MODEL.to_string()),
                    AgentProvider::OpenRouter => Some(DEFAULT_OPENROUTER_MODEL.to_string()),
                    AgentProvider::OpenAi | AgentProvider::Custom => None,
                },
                model_effective: self.agent_model().map(str::to_string),
                model_override: self.user.agent.model.clone(),
                model_layer: Self::layer_of(&self.user.agent.model, &self.host.agent.model),
                model_missing: self.agent_model().is_none(),
                model_options,
                models_loading,
                models_error,
                price_input_override: self.user.agent.price_input_per_mtok.map(format_rate),
                price_output_override: self.user.agent.price_output_per_mtok.map(format_rate),
            },
        }
    }
}

/// Trim a settings value; a blank entry means "no value", never an
/// empty-string override that would shadow the layer below.
fn normalized(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == value.len() {
        Some(value)
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse a rate text-field value: unparseable, non-finite, or non-positive
/// input clears the override.
fn parsed_rate(value: Option<String>) -> Option<f64> {
    let parsed = normalized(value)?.parse::<f64>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then_some(parsed)
}

/// Display form of a stored rate (`3` not `3.0`; `f64` Display trims).
fn format_rate(rate: f64) -> String {
    format!("{rate}")
}

/// Normalize every field of an incoming layer document.
fn normalized_settings(mut settings: StudioSettings) -> StudioSettings {
    let agent = &mut settings.agent;
    agent.anthropic_api_key = normalized(agent.anthropic_api_key.take());
    agent.openai_api_key = normalized(agent.openai_api_key.take());
    agent.custom_base_url = normalized(agent.custom_base_url.take());
    agent.custom_api_key = normalized(agent.custom_api_key.take());
    agent.openrouter_api_key = normalized(agent.openrouter_api_key.take());
    agent.model = normalized(agent.model.take());
    settings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::settings::studio_settings::AgentSettings;

    #[test]
    fn defaults_apply_with_no_layers() {
        let store = SettingsStore::default();
        assert_eq!(store.agent_provider(), AgentProvider::Anthropic);
        assert_eq!(store.agent_anthropic_api_key(), None);
        assert_eq!(store.agent_model(), Some(DEFAULT_AGENT_MODEL));
        assert!(!store.agent_ready());
        let view = store.ui_view().agent;
        assert_eq!(view.provider, AgentProvider::Anthropic);
        assert_eq!(view.api_key_layer, SettingsLayer::Default);
        assert_eq!(view.model_placeholder, DEFAULT_AGENT_MODEL);
    }

    #[test]
    fn host_layer_overrides_defaults() {
        let mut store = SettingsStore::default();
        store.set_host_layer(layer(|agent| {
            agent.anthropic_api_key = Some("sk-ant-host".into());
            agent.model = Some("host-model".into());
        }));
        assert_eq!(store.agent_anthropic_api_key(), Some("sk-ant-host"));
        assert_eq!(store.agent_model(), Some("host-model"));
        assert!(store.agent_ready());
        let view = store.ui_view().agent;
        assert_eq!(view.api_key_layer, SettingsLayer::Host);
        assert_eq!(view.model_layer, SettingsLayer::Host);
    }

    #[test]
    fn user_layer_wins_over_host_per_field() {
        let mut store = SettingsStore::default();
        store.set_host_layer(layer(|agent| {
            agent.anthropic_api_key = Some("sk-ant-host".into());
            agent.model = Some("host-model".into());
        }));
        store.set_agent_model(Some("user-model".to_string()));
        // model: user wins; key: host still supplies it
        assert_eq!(store.agent_model(), Some("user-model"));
        assert_eq!(store.agent_anthropic_api_key(), Some("sk-ant-host"));
        let view = store.ui_view().agent;
        assert_eq!(view.model_layer, SettingsLayer::User);
        assert_eq!(view.api_key_layer, SettingsLayer::Host);
    }

    #[test]
    fn clearing_a_user_override_falls_back_to_host_then_default() {
        let mut store = SettingsStore::default();
        store.set_host_layer(layer(|agent| agent.model = Some("host-model".into())));
        store.set_agent_model(Some("user-model".to_string()));
        store.set_agent_model(None);
        assert_eq!(store.agent_model(), Some("host-model"));
        store.set_host_layer(StudioSettings::default());
        assert_eq!(store.agent_model(), Some(DEFAULT_AGENT_MODEL));
    }

    #[test]
    fn blank_values_normalize_to_absent() {
        let mut store = SettingsStore::default();
        store.set_host_layer(layer(|agent| agent.anthropic_api_key = Some("  ".into())));
        assert_eq!(store.agent_anthropic_api_key(), None);
        store.set_agent_anthropic_api_key(Some("  sk-ant-user  ".to_string()));
        assert_eq!(store.agent_anthropic_api_key(), Some("sk-ant-user"));
        store.set_agent_anthropic_api_key(Some("   ".to_string()));
        assert_eq!(store.agent_anthropic_api_key(), None);
    }

    #[test]
    fn user_layer_persists_only_user_values() {
        let mut store = SettingsStore::default();
        store.set_host_layer(layer(|agent| {
            agent.anthropic_api_key = Some("sk-ant-host".into());
        }));
        store.set_agent_model(Some("user-model".to_string()));
        let persisted = store.user_layer();
        assert_eq!(persisted.agent.anthropic_api_key, None);
        assert_eq!(persisted.agent.model.as_deref(), Some("user-model"));
    }

    #[test]
    fn anthropic_readiness_needs_only_the_key() {
        let mut store = SettingsStore::default();
        assert!(!store.agent_ready());
        store.set_agent_anthropic_api_key(Some("sk-ant-x".to_string()));
        assert!(store.agent_ready());
        let Some(AgentProviderConfig::Anthropic(config)) = store.agent_provider_config() else {
            panic!("expected anthropic config");
        };
        assert_eq!(config.api_key, "sk-ant-x");
        assert_eq!(config.model, DEFAULT_AGENT_MODEL);
    }

    #[test]
    fn openai_readiness_needs_key_and_model() {
        let mut store = SettingsStore::default();
        store.set_agent_provider(Some(AgentProvider::OpenAi));
        store.set_agent_openai_api_key(Some("sk-oai-x".to_string()));
        // Key alone is not enough: no default model for OpenAI.
        assert!(!store.agent_ready());
        assert!(store.ui_view().agent.model_missing);
        store.set_agent_model(Some("some-model".to_string()));
        assert!(store.agent_ready());
        let Some(AgentProviderConfig::OpenAiCompat(config)) = store.agent_provider_config() else {
            panic!("expected openai-compat config");
        };
        assert_eq!(config.base_url, OPENAI_DEFAULT_BASE_URL);
        assert_eq!(config.api_key.as_deref(), Some("sk-oai-x"));
        assert_eq!(config.model, "some-model");
    }

    #[test]
    fn openrouter_key_alone_makes_the_agent_ready() {
        let mut store = SettingsStore::default();
        store.set_agent_provider(Some(AgentProvider::OpenRouter));
        assert!(!store.agent_ready());
        // The Connect flow writes exactly one field; everything else bakes.
        store.set_agent_openrouter_api_key(Some("sk-or-v1-abc".to_string()));
        assert!(store.agent_ready());
        let Some(AgentProviderConfig::OpenAiCompat(config)) = store.agent_provider_config() else {
            panic!("expected openai-compat config");
        };
        assert_eq!(config.base_url, OPENROUTER_BASE_URL);
        assert_eq!(config.api_key.as_deref(), Some("sk-or-v1-abc"));
        assert_eq!(config.model, DEFAULT_OPENROUTER_MODEL);
        assert!(
            config
                .extra_headers
                .iter()
                .any(|(k, _)| k == "HTTP-Referer")
        );
        let view = store.ui_view().agent;
        assert_eq!(
            view.model_default.as_deref(),
            Some(DEFAULT_OPENROUTER_MODEL)
        );
        assert!(!view.api_key_optional);
        // Disconnect = clear the one field.
        store.set_agent_openrouter_api_key(None);
        assert!(!store.agent_ready());
    }

    #[test]
    fn custom_readiness_needs_base_url_and_model_but_no_key() {
        let mut store = SettingsStore::default();
        store.set_agent_provider(Some(AgentProvider::Custom));
        store.set_agent_custom_base_url(Some("http://localhost:11434/v1".to_string()));
        assert!(!store.agent_ready());
        store.set_agent_model(Some("llama3.2".to_string()));
        assert!(store.agent_ready());
        let Some(AgentProviderConfig::OpenAiCompat(config)) = store.agent_provider_config() else {
            panic!("expected openai-compat config");
        };
        assert_eq!(config.base_url, "http://localhost:11434/v1");
        assert_eq!(config.api_key, None);
        store.set_agent_custom_api_key(Some("local".to_string()));
        let Some(AgentProviderConfig::OpenAiCompat(config)) = store.agent_provider_config() else {
            panic!("expected openai-compat config");
        };
        assert_eq!(config.api_key.as_deref(), Some("local"));
    }

    #[test]
    fn selecting_a_provider_switches_the_key_field_and_guidance() {
        let mut store = SettingsStore::default();
        store.set_agent_anthropic_api_key(Some("sk-ant-x".to_string()));
        store.set_agent_provider(Some(AgentProvider::OpenAi));
        let view = store.ui_view().agent;
        // The Anthropic key does not leak into the OpenAI key field.
        assert_eq!(view.api_key_masked, None);
        assert_eq!(view.guidance.provider, AgentProvider::OpenAi);
        assert!(view.provider_overridden);
        // Switching back finds the Anthropic key again.
        store.set_agent_provider(None);
        let view = store.ui_view().agent;
        assert!(view.api_key_masked.is_some());
        assert_eq!(view.guidance.provider, AgentProvider::Anthropic);
    }

    #[test]
    fn cost_rates_prefer_overrides_and_fall_back_to_the_table() {
        let mut store = SettingsStore::default();
        // Default model (claude-sonnet-5) resolves from the table.
        let rates = store.agent_cost_rates().expect("table rates");
        assert_eq!(rates.input_per_mtok, 3.0);
        assert_eq!(rates.output_per_mtok, 15.0);
        // A per-field override wins; the other side keeps the table value.
        store.set_agent_price_input_per_mtok(Some("4.5".to_string()));
        let rates = store.agent_cost_rates().expect("mixed rates");
        assert_eq!(rates.input_per_mtok, 4.5);
        assert_eq!(rates.output_per_mtok, 15.0);
    }

    #[test]
    fn unknown_model_without_overrides_has_no_rates() {
        let mut store = SettingsStore::default();
        store.set_agent_model(Some("mystery-model".to_string()));
        assert_eq!(store.agent_cost_rates(), None);
        // One override alone is still insufficient.
        store.set_agent_price_input_per_mtok(Some("2".to_string()));
        assert_eq!(store.agent_cost_rates(), None);
        store.set_agent_price_output_per_mtok(Some("6".to_string()));
        let rates = store.agent_cost_rates().expect("override rates");
        assert_eq!((rates.input_per_mtok, rates.output_per_mtok), (2.0, 6.0));
    }

    #[test]
    fn rate_inputs_parse_leniently_and_reject_junk() {
        let mut store = SettingsStore::default();
        store.set_agent_price_input_per_mtok(Some(" 3.5 ".to_string()));
        assert_eq!(store.user_layer().agent.price_input_per_mtok, Some(3.5));
        store.set_agent_price_input_per_mtok(Some("cheap".to_string()));
        assert_eq!(store.user_layer().agent.price_input_per_mtok, None);
        store.set_agent_price_input_per_mtok(Some("-1".to_string()));
        assert_eq!(store.user_layer().agent.price_input_per_mtok, None);
    }

    #[test]
    fn ui_view_reports_masked_key_provenance_and_overrides() {
        let mut store = SettingsStore::default();
        store.set_host_layer(layer(|agent| {
            agent.anthropic_api_key = Some("sk-ant-api03-abcdefgh".into());
        }));
        store.set_agent_model(Some("user-model".to_string()));
        let agent = store.ui_view().agent;
        let masked = agent.api_key_masked.as_deref().unwrap();
        assert!(!masked.contains("api03-abcd"), "masked key leaks: {masked}");
        assert_eq!(agent.api_key_layer, SettingsLayer::Host);
        assert!(!agent.api_key_overridden);
        assert_eq!(agent.model_default.as_deref(), Some(DEFAULT_AGENT_MODEL));
        assert_eq!(agent.model_override.as_deref(), Some("user-model"));
        assert_eq!(agent.model_layer, SettingsLayer::User);
    }

    fn layer(build: impl FnOnce(&mut AgentSettings)) -> StudioSettings {
        let mut settings = StudioSettings::default();
        build(&mut settings.agent);
        settings
    }

    // -- discovered models (P8) --------------------------------------------

    fn model(id: &str, display_name: Option<&str>) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            display_name: display_name.map(str::to_string),
        }
    }

    #[test]
    fn discovery_config_resolves_without_a_model() {
        let mut store = SettingsStore::default();
        store.set_agent_provider(Some(AgentProvider::OpenAi));
        store.set_agent_openai_api_key(Some("sk-oai-x".to_string()));
        // No model: the run config is unavailable, discovery is not.
        assert!(store.agent_provider_config().is_none());
        let Some(AgentProviderConfig::OpenAiCompat(config)) = store.agent_discovery_config() else {
            panic!("expected discovery config");
        };
        assert_eq!(config.model, "");
        assert_eq!(config.api_key.as_deref(), Some("sk-oai-x"));
        // Without the key, discovery is unavailable too.
        store.set_agent_openai_api_key(None);
        assert!(store.agent_discovery_config().is_none());
    }

    #[test]
    fn model_fetch_lifecycle_reaches_the_view() {
        let mut store = SettingsStore::default();
        store.set_agent_anthropic_api_key(Some("sk-ant-x".to_string()));
        let provider = AgentProvider::Anthropic;

        // Request → Loading (and the view flags it).
        assert!(store.request_agent_models(provider, "fp1".into(), false));
        assert!(store.ui_view().agent.models_loading);
        // A second request under the same fingerprint debounces, forced or
        // not — a fetch is in flight.
        assert!(!store.request_agent_models(provider, "fp1".into(), false));
        assert!(!store.request_agent_models(provider, "fp1".into(), true));

        // Loaded → options in the view, labels preferred.
        store.agent_models_loaded(
            provider,
            "fp1",
            Ok(vec![
                model("claude-sonnet-5", Some("Claude Sonnet 5")),
                model("claude-haiku-4-5", None),
            ]),
            42.0,
        );
        let agent = store.ui_view().agent;
        assert!(!agent.models_loading);
        assert_eq!(
            agent.model_options,
            vec![
                UiModelOption {
                    id: "claude-sonnet-5".into(),
                    label: Some("Claude Sonnet 5".into()),
                },
                UiModelOption {
                    id: "claude-haiku-4-5".into(),
                    label: None,
                },
            ]
        );
        assert!(matches!(
            store.agent_models(provider).map(|s| &s.fetch),
            Some(AgentModelsFetch::Loaded { fetched_at, .. }) if *fetched_at == 42.0
        ));

        // Same fingerprint, resolved: only `force` refetches.
        assert!(!store.request_agent_models(provider, "fp1".into(), false));
        assert!(store.request_agent_models(provider, "fp1".into(), true));
    }

    #[test]
    fn stale_results_are_dropped_and_new_fingerprints_refetch() {
        let mut store = SettingsStore::default();
        let provider = AgentProvider::Anthropic;
        assert!(store.request_agent_models(provider, "fp-old".into(), false));
        // Credentials change mid-flight: a new fingerprint starts fresh...
        assert!(store.request_agent_models(provider, "fp-new".into(), false));
        // ...and the old fetch's landing is dropped as stale.
        store.agent_models_loaded(provider, "fp-old", Ok(vec![model("stale", None)]), 1.0);
        assert!(matches!(
            store.agent_models(provider).map(|s| &s.fetch),
            Some(AgentModelsFetch::Loading)
        ));
        store.agent_models_loaded(provider, "fp-new", Ok(vec![model("fresh", None)]), 2.0);
        assert_eq!(store.ui_view().agent.model_options[0].id, "fresh");
    }

    #[test]
    fn failed_fetches_surface_mapped_error_copy_per_provider() {
        let mut store = SettingsStore::default();
        store.set_agent_anthropic_api_key(Some("sk-bad".to_string()));
        let provider = AgentProvider::Anthropic;
        assert!(store.request_agent_models(provider, "fp".into(), false));
        store.agent_models_loaded(
            provider,
            "fp",
            Err(ListModelsError::Auth {
                message: "401".into(),
            }),
            1.0,
        );
        let agent = store.ui_view().agent;
        assert!(agent.model_options.is_empty());
        assert!(
            agent
                .models_error
                .as_deref()
                .is_some_and(|copy| copy.contains("key was rejected")),
            "{:?}",
            agent.models_error
        );
        // The error state debounces like a resolved fetch; force retries.
        assert!(!store.request_agent_models(provider, "fp".into(), false));
        assert!(store.request_agent_models(provider, "fp".into(), true));
    }

    #[test]
    fn model_state_is_per_provider_and_clearable() {
        let mut store = SettingsStore::default();
        assert!(store.request_agent_models(AgentProvider::Anthropic, "a".into(), false));
        assert!(store.request_agent_models(AgentProvider::Custom, "c".into(), false));
        store.agent_models_loaded(
            AgentProvider::Anthropic,
            "a",
            Ok(vec![model("claude-sonnet-5", None)]),
            1.0,
        );
        // The view follows the SELECTED provider: Anthropic sees its list,
        // Custom still shows its in-flight fetch.
        store.set_agent_anthropic_api_key(Some("sk".to_string()));
        assert_eq!(store.ui_view().agent.model_options.len(), 1);
        store.set_agent_provider(Some(AgentProvider::Custom));
        assert!(store.ui_view().agent.models_loading);
        // Clearing drops one provider's state only.
        store.clear_agent_models(AgentProvider::Custom);
        assert!(store.agent_models(AgentProvider::Custom).is_none());
        assert!(store.agent_models(AgentProvider::Anthropic).is_some());
    }
}
