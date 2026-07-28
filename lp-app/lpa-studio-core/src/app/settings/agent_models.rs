//! Discovered-model state for the settings model dropdowns (P8).
//!
//! Transient runtime state living beside the layered settings: per
//! provider, the latest `/models` fetch outcome keyed by a **config
//! fingerprint** (credentials + endpoint). The fingerprint is the
//! staleness guard both ways — a credential change starts a fresh fetch,
//! and a late response from the old credentials can never land on top of
//! the new ones.

use lpa_agent::{ListModelsError, ModelInfo};

use crate::app::agent::agent_provider_config::AgentProviderConfig;
use crate::app::settings::agent_provider::AgentProvider;

/// One provider's model-listing fetch state.
#[derive(Clone, Debug, PartialEq)]
pub enum AgentModelsFetch {
    /// A fetch is in flight.
    Loading,
    /// The listed models (server order), stamped with the controller
    /// clock's seconds-since-epoch at arrival.
    Loaded {
        models: Vec<ModelInfo>,
        fetched_at: f64,
    },
    /// The fetch failed (typed for guidance mapping).
    Failed { error: ListModelsError },
}

/// The per-provider entry: which config identity the fetch belongs to,
/// plus its state.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentModelsState {
    /// [`discovery_fingerprint`] of the config the fetch used.
    pub fingerprint: String,
    pub fetch: AgentModelsFetch,
}

/// The config identity a model listing is valid for: endpoint +
/// credentials. The model field is deliberately excluded — picking a model
/// from the fetched list must not invalidate that very list.
pub fn discovery_fingerprint(config: &AgentProviderConfig) -> String {
    match config {
        AgentProviderConfig::Anthropic(config) => {
            format!("{}\n{}", config.base_url, config.api_key)
        }
        AgentProviderConfig::OpenAiCompat(config) => format!(
            "{}\n{}",
            config.base_url,
            config.api_key.as_deref().unwrap_or_default()
        ),
    }
}

/// Display copy for a failed fetch. The remedies themselves live in the
/// provider guidance block the popover already renders (key setup links,
/// the CORS note for local servers); this line names the failure and
/// points at the right field.
pub fn models_error_copy(provider: AgentProvider, error: &ListModelsError) -> String {
    match error {
        ListModelsError::Auth { .. } => {
            "model list unavailable — the API key was rejected".to_string()
        }
        ListModelsError::Network { .. } => match provider {
            AgentProvider::Custom => "model list unavailable — server unreachable \
                                      (check the base URL and the CORS note above)"
                .to_string(),
            _ => format!(
                "model list unavailable — couldn't reach {}",
                provider.label()
            ),
        },
        ListModelsError::Http { status, .. } => {
            format!("model list unavailable (HTTP {status})")
        }
        ListModelsError::Parse { .. } => {
            "model list unavailable — unexpected response from the server".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_agent::{AnthropicConfig, OpenAiCompatConfig};

    use super::*;

    #[test]
    fn fingerprint_covers_credentials_and_endpoint_but_not_model() {
        let config = |model: &str, key: &str| {
            AgentProviderConfig::OpenAiCompat(OpenAiCompatConfig {
                base_url: "http://localhost:11434/v1".into(),
                api_key: Some(key.into()),
                model: model.into(),
                extra_headers: vec![],
            })
        };
        assert_eq!(
            discovery_fingerprint(&config("a", "k1")),
            discovery_fingerprint(&config("b", "k1")),
            "model choice must not invalidate the list"
        );
        assert_ne!(
            discovery_fingerprint(&config("a", "k1")),
            discovery_fingerprint(&config("a", "k2")),
            "credential change must invalidate the list"
        );
    }

    #[test]
    fn anthropic_and_keyless_fingerprints_are_stable() {
        let anthropic = AgentProviderConfig::Anthropic(AnthropicConfig::new("sk-x"));
        assert!(discovery_fingerprint(&anthropic).contains("sk-x"));
        let keyless = AgentProviderConfig::OpenAiCompat(OpenAiCompatConfig {
            base_url: "http://box/v1".into(),
            api_key: None,
            model: String::new(),
            extra_headers: vec![],
        });
        assert_eq!(discovery_fingerprint(&keyless), "http://box/v1\n");
    }

    #[test]
    fn error_copy_maps_auth_network_and_parse_distinctly() {
        let auth = models_error_copy(
            AgentProvider::Anthropic,
            &ListModelsError::Auth {
                message: "401".into(),
            },
        );
        assert!(auth.contains("key was rejected"), "{auth}");

        let cors = models_error_copy(
            AgentProvider::Custom,
            &ListModelsError::Network {
                message: "Failed to fetch".into(),
            },
        );
        assert!(cors.contains("CORS"), "{cors}");

        let network = models_error_copy(
            AgentProvider::OpenAi,
            &ListModelsError::Network {
                message: "refused".into(),
            },
        );
        assert!(network.contains("OpenAI"), "{network}");

        let parse = models_error_copy(
            AgentProvider::OpenAi,
            &ListModelsError::Parse {
                message: "bad".into(),
            },
        );
        assert!(parse.contains("unexpected response"), "{parse}");

        let http = models_error_copy(
            AgentProvider::OpenAi,
            &ListModelsError::Http {
                status: 502,
                message: "gateway".into(),
            },
        );
        assert!(http.contains("502"), "{http}");
    }
}
