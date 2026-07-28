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
use crate::app::settings::ui_settings_view::UiModelOption;

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

/// Turn one provider's listing into the dropdown's options: drop what the
/// agent cannot drive, then order best-first.
///
/// Server order is unusable at OpenRouter's scale (~340 models) and
/// alphabetical is worse — it leads with `ai21/…` and buries every model
/// worth picking. There is no popularity signal in any provider's API
/// (`/models` publishes none, and the endpoint openrouter.ai's rankings
/// page uses is CORS-locked to their own origin), so the ranking uses what
/// the payload itself carries:
///
/// 1. published coding score, best first;
/// 2. then newest first, for the models scored by nobody;
/// 3. then by id, so the tail is stable.
///
/// Providers that publish neither (Anthropic, OpenAI, local servers) keep
/// their listing order, which is already meaningful.
pub fn model_options(models: &[ModelInfo]) -> Vec<UiModelOption> {
    let mut offerable: Vec<&ModelInfo> =
        models.iter().filter(|model| is_offerable(model)).collect();
    if offerable.iter().any(|model| model.coding_score.is_some()) {
        offerable.sort_by(|a, b| {
            descending(a.coding_score, b.coding_score)
                .then_with(|| descending(a.created, b.created))
                .then_with(|| a.id.cmp(&b.id))
        });
    }
    offerable
        .into_iter()
        .map(|model| UiModelOption {
            id: model.id.clone(),
            label: model.display_name.clone(),
            detail: option_detail(model),
        })
        .collect()
}

/// How many of a listing's models the picker declines to offer, for the
/// "N hidden" line — a missing id must read as a filter, not a bug.
pub fn hidden_model_count(models: &[ModelInfo]) -> usize {
    models.iter().filter(|model| !is_offerable(model)).count()
}

/// Whether a model belongs in the dropdown at all.
///
/// Two exclusions, both about the agent being unable to use the model
/// rather than taste: ids that name a non-chat modality (embeddings,
/// speech, images), and models the provider marks tool-incapable — the
/// agent is a single-tool loop, so those fail on the first turn. Silence
/// about tool support is not a no; only OpenRouter publishes the field.
fn is_offerable(model: &ModelInfo) -> bool {
    is_chat_model_id(&model.id) && model.supports_tools != Some(false)
}

/// Whether an id names something this agent can drive. A provider's list
/// mixes in embedding, speech, image and moderation models; the model
/// field still accepts anything typed, so this only trims the menu.
pub fn is_chat_model_id(id: &str) -> bool {
    const UNUSABLE: &[&str] = &[
        "embed",
        "whisper",
        "tts",
        "-audio",
        "transcribe",
        "speech",
        "dall-e",
        "moderation",
        "-image",
        "image-",
        "rerank",
        "stable-diffusion",
        "davinci",
        "babbage",
    ];
    let lowered = id.to_ascii_lowercase();
    !UNUSABLE.iter().any(|marker| lowered.contains(marker))
}

/// The trailing detail on an option: the score that put it where it is,
/// and what it costs. Absent for providers that publish neither.
fn option_detail(model: &ModelInfo) -> Option<String> {
    let score = model.coding_score.map(|score| format!("code {score:.0}"));
    let price = model.price.map(|price| {
        format!(
            "${}/${}",
            format_rate(price.input_per_mtok),
            format_rate(price.output_per_mtok)
        )
    });
    match (score, price) {
        (Some(score), Some(price)) => Some(format!("{score} · {price}")),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// Trim a rate for display: whole dollars bare, cents to two decimals.
fn format_rate(rate: f64) -> String {
    if rate == 0.0 {
        "0".to_string()
    } else if (rate - rate.round()).abs() < f64::EPSILON {
        format!("{}", rate.round())
    } else {
        format!("{rate:.2}")
    }
}

/// Descending order for an optional key, with absent values last.
fn descending<T: PartialOrd>(a: Option<T>, b: Option<T>) -> core::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.partial_cmp(&a).unwrap_or(core::cmp::Ordering::Equal),
        (Some(_), None) => core::cmp::Ordering::Less,
        (None, Some(_)) => core::cmp::Ordering::Greater,
        (None, None) => core::cmp::Ordering::Equal,
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
    fn the_best_models_lead_the_options_not_the_alphabet() {
        let scored = |id: &str, score: f64, created: i64| ModelInfo {
            coding_score: Some(score),
            created: Some(created),
            supports_tools: Some(true),
            ..ModelInfo::new(id)
        };
        let options = model_options(&[
            ModelInfo {
                created: Some(100),
                supports_tools: Some(true),
                ..ModelInfo::new("aion-labs/aion-2.0")
            },
            scored("anthropic/claude-opus-5", 78.0, 200),
            scored("openai/gpt-5.6-sol", 77.4, 150),
            ModelInfo {
                created: Some(300),
                supports_tools: Some(true),
                ..ModelInfo::new("zz/newer-unscored")
            },
        ]);
        assert_eq!(
            options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            vec![
                "anthropic/claude-opus-5",
                "openai/gpt-5.6-sol",
                // unscored fall below, newest first
                "zz/newer-unscored",
                "aion-labs/aion-2.0",
            ]
        );
    }

    #[test]
    fn a_listing_without_scores_keeps_the_providers_own_order() {
        // Anthropic and local servers publish no ranking signal, and their
        // listings are already meaningful (newest first / pull order).
        let options = model_options(&[
            ModelInfo::new("claude-sonnet-5"),
            ModelInfo::new("claude-haiku-4-5"),
        ]);
        assert_eq!(
            options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            vec!["claude-sonnet-5", "claude-haiku-4-5"]
        );
    }

    #[test]
    fn models_the_agent_cannot_drive_are_not_offered() {
        let models = vec![
            ModelInfo {
                supports_tools: Some(false),
                ..ModelInfo::new("chat-only")
            },
            ModelInfo {
                supports_tools: Some(true),
                ..ModelInfo::new("tool-capable")
            },
            // Silence about tool support is not a no.
            ModelInfo::new("unknown-support"),
            ModelInfo::new("text-embedding-3-large"),
            ModelInfo::new("nomic-embed-text:latest"),
        ];
        let options = model_options(&models);
        assert_eq!(
            options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            vec!["tool-capable", "unknown-support"]
        );
        assert_eq!(hidden_model_count(&models), 3);
    }

    #[test]
    fn the_option_detail_carries_the_score_and_the_rates() {
        let options = model_options(&[ModelInfo {
            coding_score: Some(78.0),
            price: Some(lpa_agent::ModelPrice {
                input_per_mtok: 5.0,
                output_per_mtok: 25.0,
            }),
            ..ModelInfo::new("anthropic/claude-opus-5")
        }]);
        assert_eq!(options[0].detail.as_deref(), Some("code 78 · $5/$25"));

        // Sub-dollar rates keep their cents; a scoreless model shows price
        // alone, and a listing with neither shows nothing.
        let options = model_options(&[
            ModelInfo {
                price: Some(lpa_agent::ModelPrice {
                    input_per_mtok: 0.15,
                    output_per_mtok: 0.6,
                }),
                ..ModelInfo::new("cheap/model")
            },
            ModelInfo::new("bare/model"),
        ]);
        assert_eq!(options[0].detail.as_deref(), Some("$0.15/$0.60"));
        assert_eq!(options[1].detail, None);
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
