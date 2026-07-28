//! Model discovery: asking the selected provider which models it serves.
//!
//! Every provider Studio talks to exposes a model list, and all four answer
//! in the same `{"data":[{"id":…}]}` envelope — Anthropic adds
//! `display_name`, OpenRouter adds `name` and per-token `pricing`. So the
//! provider-specific part is only the *request* (URL + auth headers); one
//! parser reads every answer.
//!
//! Pure and host-tested: the platform edge performs the fetch and hands the
//! body back. What belongs here is everything a reviewer would want pinned —
//! which endpoint each provider uses, which ids are worth offering (a model
//! list is full of embedding and speech models the shader agent cannot use),
//! and how a candidate reads in the picker.

use serde_json::Value;

use crate::app::settings::agent_provider::AgentProvider;
use crate::app::settings::settings_command::SettingsCommand;
use crate::app::settings::studio_settings::OPENROUTER_BASE_URL;

/// Anthropic's dated API version, matching the agent provider's requests.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Anthropic's API origin (the agent provider's default).
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
/// OpenAI's API origin.
const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// One model-list request, ready for the platform edge to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelCatalogRequest {
    pub url: String,
    /// Auth/version headers, set verbatim.
    pub headers: Vec<(String, String)>,
}

/// One offerable model.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogModel {
    /// The id to put in the model field — the only value that matters.
    pub id: String,
    /// Human name, when the provider supplies one distinct from the id.
    pub label: Option<String>,
    /// Published rates, when the provider lists them (OpenRouter).
    pub price: Option<CatalogPrice>,
}

/// Published rates for a model, $ per million tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CatalogPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

/// A provider's answer, ready to browse.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelCatalog {
    /// Offerable models, sorted by id.
    pub models: Vec<CatalogModel>,
    /// How many ids were dropped as unusable for this agent (embeddings,
    /// speech, images). Surfaced so a missing id reads as a filter, not a
    /// bug — the model field still accepts anything typed.
    pub hidden: usize,
}

/// The picker's state, carried on the settings view.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelCatalogState {
    /// The picker is expanded.
    pub open: bool,
    /// A fetch is in flight.
    pub loading: bool,
    /// Why the last attempt produced nothing to browse.
    pub error: Option<String>,
    pub catalog: Option<ModelCatalog>,
    /// Which provider+endpoint the loaded catalog belongs to, so switching
    /// provider or base URL cannot leave a stale list on screen.
    pub loaded_for: Option<String>,
}

impl ModelCatalogState {
    /// Whether a fresh fetch is needed to show `fingerprint`'s models.
    pub fn needs_load(&self, fingerprint: &str) -> bool {
        self.loaded_for.as_deref() != Some(fingerprint)
    }
}

/// The model-list request for a provider, or the one-line reason there is
/// none to make yet.
///
/// `base_url` is the Custom provider's address; `api_key` the effective key
/// for the selected provider. OpenRouter's catalog is public — it lists
/// before the user connects, which is exactly when a model id is needed.
pub fn catalog_request(
    provider: AgentProvider,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<ModelCatalogRequest, String> {
    match provider {
        AgentProvider::Anthropic => {
            let key = api_key.ok_or("Add your Anthropic API key first, then browse models.")?;
            Ok(ModelCatalogRequest {
                url: format!("{ANTHROPIC_BASE_URL}/v1/models?limit=100"),
                headers: vec![
                    ("x-api-key".to_string(), key.to_string()),
                    (
                        "anthropic-version".to_string(),
                        ANTHROPIC_VERSION.to_string(),
                    ),
                    // Same header the agent's own requests carry: without it
                    // the browser's preflight is refused.
                    (
                        "anthropic-dangerous-direct-browser-access".to_string(),
                        "true".to_string(),
                    ),
                ],
            })
        }
        AgentProvider::OpenAi => {
            let key = api_key.ok_or("Add your OpenAI API key first, then browse models.")?;
            Ok(ModelCatalogRequest {
                url: format!("{OPENAI_BASE_URL}/models"),
                headers: vec![("authorization".to_string(), format!("Bearer {key}"))],
            })
        }
        AgentProvider::OpenRouter => Ok(ModelCatalogRequest {
            url: format!("{OPENROUTER_BASE_URL}/models"),
            // Public list: no key, so no header — which also keeps this a
            // CORS-simple GET.
            headers: Vec::new(),
        }),
        AgentProvider::Custom => {
            let base_url = base_url
                .ok_or("Set the base URL first — then Studio can ask the server what it serves.")?;
            let mut headers = Vec::new();
            if let Some(key) = api_key {
                headers.push(("authorization".to_string(), format!("Bearer {key}")));
            }
            Ok(ModelCatalogRequest {
                url: crate::app::settings::local_model_probe::models_url(base_url),
                headers,
            })
        }
    }
}

/// Everything choosing a catalog model changes: the model id, and the
/// cost-estimate rates that describe *that* model.
///
/// Rates ride along because nobody fills them in by hand — and because the
/// alternative is worse than empty: rates left over from the previously
/// chosen model would quietly mis-price the new one. A provider that
/// publishes no rates therefore CLEARS them, which falls back to the
/// built-in table for known ids.
pub fn adopt_model_commands(model: &CatalogModel) -> Vec<SettingsCommand> {
    let (input, output) = match model.price {
        Some(price) => (
            Some(rate_field(price.input_per_mtok)),
            Some(rate_field(price.output_per_mtok)),
        ),
        None => (None, None),
    };
    vec![
        SettingsCommand::SetAgentModel(Some(model.id.clone())),
        SettingsCommand::SetAgentPriceInputPerMtok(input),
        SettingsCommand::SetAgentPriceOutputPerMtok(output),
    ]
}

/// A published rate as the rate field's text (the store parses it back).
fn rate_field(per_mtok: f64) -> String {
    format!("{per_mtok}")
}

/// An identity for the loaded catalog, so a provider or address change
/// invalidates it.
pub fn catalog_fingerprint(provider: AgentProvider, base_url: Option<&str>) -> String {
    format!("{}|{}", provider.label(), base_url.unwrap_or_default())
}

/// Read a model-list body into offerable candidates: parsed, filtered to
/// what this agent can drive, and sorted by id.
pub fn parse_catalog(body: &str) -> Result<ModelCatalog, String> {
    let all = parse_catalog_entries(body)?;
    let total = all.len();
    let mut models: Vec<CatalogModel> = all
        .into_iter()
        .filter(|model| is_usable_model_id(&model.id))
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(ModelCatalog {
        hidden: total - models.len(),
        models,
    })
}

/// Read the `{"data":[…]}` envelope every provider answers with, keeping
/// whatever optional fields are present. An explicitly null `data` is an
/// empty list (Ollama with nothing pulled answers that way), not a
/// malformed body.
pub fn parse_catalog_entries(body: &str) -> Result<Vec<CatalogModel>, String> {
    let json: Value = serde_json::from_str(body)
        .map_err(|_| format!("the response was not JSON: {}", excerpt(body)))?;
    let data = match json.get("data") {
        Some(Value::Array(data)) => data,
        Some(Value::Null) | None if json.get("object").and_then(Value::as_str) == Some("list") => {
            return Ok(Vec::new());
        }
        _ => {
            return Err(format!(
                "no \"data\" list in the response: {}",
                excerpt(body)
            ));
        }
    };
    Ok(data
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id").and_then(Value::as_str)?;
            Some(CatalogModel {
                id: id.to_string(),
                label: entry
                    .get("display_name")
                    .or_else(|| entry.get("name"))
                    .and_then(Value::as_str)
                    .filter(|label| *label != id)
                    .map(str::to_string),
                price: parse_price(entry.get("pricing")),
            })
        })
        .collect())
}

/// Whether an id is worth offering for shader work. A provider's list mixes
/// in embedding, speech, image and moderation models the agent cannot use;
/// they are noise in a picker, and the model field still accepts anything.
pub fn is_usable_model_id(id: &str) -> bool {
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

/// The models matching a picker query: a case-insensitive substring of the
/// id or the label. A blank query matches everything.
pub fn filter_models<'a>(models: &'a [CatalogModel], query: &str) -> Vec<&'a CatalogModel> {
    let query = query.trim().to_ascii_lowercase();
    models
        .iter()
        .filter(|model| {
            query.is_empty()
                || model.id.to_ascii_lowercase().contains(&query)
                || model
                    .label
                    .as_deref()
                    .is_some_and(|label| label.to_ascii_lowercase().contains(&query))
        })
        .collect()
}

impl CatalogPrice {
    /// Compact rate pair for a picker row (`$3/$15` per MTok).
    pub fn summary(&self) -> String {
        format!(
            "${}/${}",
            format_rate(self.input_per_mtok),
            format_rate(self.output_per_mtok)
        )
    }
}

/// OpenRouter prices are per-token decimal strings; the UI speaks $/MTok.
fn parse_price(pricing: Option<&Value>) -> Option<CatalogPrice> {
    let pricing = pricing?;
    let per_mtok = |field: &str| -> Option<f64> {
        let raw = pricing.get(field)?;
        let value = match raw {
            Value::String(text) => text.parse::<f64>().ok()?,
            other => other.as_f64()?,
        };
        value.is_finite().then_some(value * 1_000_000.0)
    };
    let input = per_mtok("prompt")?;
    let output = per_mtok("completion")?;
    // Free models list "0"; that is a real answer, not a missing one.
    (input >= 0.0 && output >= 0.0).then_some(CatalogPrice {
        input_per_mtok: input,
        output_per_mtok: output,
    })
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

fn excerpt(body: &str) -> String {
    let body = body.trim();
    match body.char_indices().nth(80) {
        Some((index, _)) => format!("{}…", &body[..index]),
        None => body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_names_its_endpoint_and_auth() {
        let anthropic = catalog_request(AgentProvider::Anthropic, None, Some("sk-ant-x")).unwrap();
        assert!(
            anthropic
                .url
                .starts_with("https://api.anthropic.com/v1/models")
        );
        assert!(
            anthropic
                .headers
                .iter()
                .any(|(k, v)| k == "x-api-key" && v == "sk-ant-x")
        );
        // The browser-access header the agent's own requests carry: without
        // it the preflight is refused and discovery would look "broken".
        assert!(
            anthropic
                .headers
                .iter()
                .any(|(k, _)| k == "anthropic-dangerous-direct-browser-access")
        );

        let openai = catalog_request(AgentProvider::OpenAi, None, Some("sk-oai-x")).unwrap();
        assert_eq!(openai.url, "https://api.openai.com/v1/models");
        assert_eq!(
            openai.headers,
            vec![("authorization".to_string(), "Bearer sk-oai-x".to_string())]
        );

        // OpenRouter lists publicly — before Connect, which is when a model
        // id is actually needed.
        let openrouter = catalog_request(AgentProvider::OpenRouter, None, None).unwrap();
        assert_eq!(openrouter.url, "https://openrouter.ai/api/v1/models");
        assert!(openrouter.headers.is_empty());

        let custom = catalog_request(
            AgentProvider::Custom,
            Some("http://localhost:11434/v1"),
            None,
        )
        .unwrap();
        assert_eq!(custom.url, "http://localhost:11434/v1/models");
        assert!(custom.headers.is_empty(), "{:?}", custom.headers);
    }

    #[test]
    fn missing_credentials_explain_themselves_instead_of_fetching() {
        for (provider, expected) in [
            (AgentProvider::Anthropic, "Anthropic API key"),
            (AgentProvider::OpenAi, "OpenAI API key"),
            (AgentProvider::Custom, "base URL"),
        ] {
            let error = catalog_request(provider, None, None).expect_err("blocked");
            assert!(error.contains(expected), "{provider:?}: {error}");
        }
    }

    #[test]
    fn a_custom_key_rides_along_when_the_server_wants_one() {
        let custom = catalog_request(
            AgentProvider::Custom,
            Some("http://box:8000/v1"),
            Some("local-key"),
        )
        .unwrap();
        assert_eq!(
            custom.headers,
            vec![("authorization".to_string(), "Bearer local-key".to_string())]
        );
    }

    #[test]
    fn anthropic_shape_parses_with_display_names() {
        let catalog = parse_catalog(
            r#"{"data":[
                {"type":"model","id":"claude-sonnet-5","display_name":"Claude Sonnet 5"},
                {"type":"model","id":"claude-haiku-4-5","display_name":"Claude Haiku 4.5"}
            ],"has_more":false}"#,
        )
        .unwrap();
        assert_eq!(catalog.models.len(), 2);
        // Sorted by id: haiku before sonnet.
        assert_eq!(catalog.models[0].id, "claude-haiku-4-5");
        assert_eq!(catalog.models[0].label.as_deref(), Some("Claude Haiku 4.5"));
        assert_eq!(catalog.hidden, 0);
    }

    #[test]
    fn openrouter_shape_parses_names_and_converts_pricing_to_per_mtok() {
        let catalog = parse_catalog(
            r#"{"data":[{
                "id":"anthropic/claude-sonnet-5",
                "name":"Anthropic: Claude Sonnet 5",
                "pricing":{"prompt":"0.000003","completion":"0.000015"}
            },{
                "id":"meta-llama/llama-3.3-8b-instruct:free",
                "name":"Llama 3.3 8B (free)",
                "pricing":{"prompt":"0","completion":"0"}
            }]}"#,
        )
        .unwrap();
        let sonnet = &catalog.models[0];
        assert_eq!(sonnet.id, "anthropic/claude-sonnet-5");
        assert_eq!(sonnet.label.as_deref(), Some("Anthropic: Claude Sonnet 5"));
        let price = sonnet.price.expect("pricing");
        assert!((price.input_per_mtok - 3.0).abs() < 1e-9, "{price:?}");
        assert!((price.output_per_mtok - 15.0).abs() < 1e-9, "{price:?}");
        assert_eq!(price.summary(), "$3/$15");
        // Free models carry a real zero, not a missing price.
        assert_eq!(
            catalog.models[1].price.expect("free pricing").summary(),
            "$0/$0"
        );
    }

    #[test]
    fn sub_dollar_rates_keep_their_cents() {
        let price = CatalogPrice {
            input_per_mtok: 0.15,
            output_per_mtok: 0.6,
        };
        assert_eq!(price.summary(), "$0.15/$0.60");
    }

    #[test]
    fn non_chat_models_are_filtered_and_counted() {
        let catalog = parse_catalog(
            r#"{"object":"list","data":[
                {"id":"gpt-5"},
                {"id":"text-embedding-3-large"},
                {"id":"whisper-1"},
                {"id":"gpt-4o-audio-preview"},
                {"id":"dall-e-3"},
                {"id":"omni-moderation-latest"},
                {"id":"nomic-embed-text:latest"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            catalog.models.iter().map(|m| &m.id).collect::<Vec<_>>(),
            vec!["gpt-5"]
        );
        assert_eq!(catalog.hidden, 6);
    }

    #[test]
    fn usable_ids_keep_the_models_the_agent_can_actually_drive() {
        for id in [
            "gpt-5",
            "claude-sonnet-5",
            "qwen3-coder:30b",
            "anthropic/claude-opus-4-8",
            "llama3.2",
        ] {
            assert!(is_usable_model_id(id), "{id} should be offered");
        }
        for id in [
            "text-embedding-3-small",
            "nomic-embed-text",
            "whisper-large-v3",
            "gpt-4o-mini-tts",
            "gpt-4o-transcribe",
            "dall-e-2",
            "stable-diffusion-xl",
            "text-davinci-003",
        ] {
            assert!(!is_usable_model_id(id), "{id} should be hidden");
        }
    }

    #[test]
    fn a_null_data_list_is_empty_not_broken() {
        let catalog = parse_catalog(r#"{"object":"list","data":null}"#).unwrap();
        assert!(catalog.models.is_empty());
        assert_eq!(catalog.hidden, 0);
    }

    #[test]
    fn wrong_shapes_report_themselves() {
        let error = parse_catalog(r#"{"models":[{"name":"llama3.2"}]}"#).unwrap_err();
        assert!(error.contains("\"data\""), "{error}");
        let error = parse_catalog("<html>nope</html>").unwrap_err();
        assert!(error.contains("not JSON"), "{error}");
    }

    #[test]
    fn the_filter_matches_ids_and_labels_case_insensitively() {
        let models = vec![
            CatalogModel {
                id: "anthropic/claude-sonnet-5".to_string(),
                label: Some("Anthropic: Claude Sonnet 5".to_string()),
                price: None,
            },
            CatalogModel {
                id: "qwen/qwen3-coder-30b".to_string(),
                label: None,
                price: None,
            },
        ];
        assert_eq!(filter_models(&models, "").len(), 2);
        assert_eq!(filter_models(&models, "SONNET").len(), 1);
        // Label-only match (the id says "qwen3", the query says "coder").
        assert_eq!(
            filter_models(&models, "coder")[0].id,
            "qwen/qwen3-coder-30b"
        );
        assert!(filter_models(&models, "gpt").is_empty());
    }

    #[test]
    fn adopting_a_priced_model_carries_its_rates() {
        let commands = adopt_model_commands(&CatalogModel {
            id: "anthropic/claude-sonnet-5".to_string(),
            label: None,
            price: Some(CatalogPrice {
                input_per_mtok: 3.0,
                output_per_mtok: 15.0,
            }),
        });
        assert!(matches!(
            &commands[0],
            SettingsCommand::SetAgentModel(Some(id)) if id == "anthropic/claude-sonnet-5"
        ));
        assert!(matches!(
            &commands[1],
            SettingsCommand::SetAgentPriceInputPerMtok(Some(rate)) if rate == "3"
        ));
        assert!(matches!(
            &commands[2],
            SettingsCommand::SetAgentPriceOutputPerMtok(Some(rate)) if rate == "15"
        ));
    }

    #[test]
    fn adopting_an_unpriced_model_clears_the_previous_models_rates() {
        // The failure this prevents: pick a $15/MTok model, then pick a
        // local one, and keep estimating the local run at $15/MTok.
        let commands = adopt_model_commands(&CatalogModel {
            id: "qwen3-coder:30b".to_string(),
            label: None,
            price: None,
        });
        assert!(matches!(
            commands[1],
            SettingsCommand::SetAgentPriceInputPerMtok(None)
        ));
        assert!(matches!(
            commands[2],
            SettingsCommand::SetAgentPriceOutputPerMtok(None)
        ));
    }

    #[test]
    fn sub_dollar_rates_survive_the_round_trip_through_the_field() {
        let commands = adopt_model_commands(&CatalogModel {
            id: "qwen/qwen3-coder-30b".to_string(),
            label: None,
            price: Some(CatalogPrice {
                input_per_mtok: 0.07,
                output_per_mtok: 0.28,
            }),
        });
        let SettingsCommand::SetAgentPriceInputPerMtok(Some(input)) = &commands[1] else {
            panic!("expected an input rate");
        };
        assert_eq!(input, "0.07");
        assert_eq!(input.parse::<f64>().unwrap(), 0.07);
    }

    #[test]
    fn a_fingerprint_change_forces_a_reload() {
        let mut state = ModelCatalogState::default();
        let ollama = catalog_fingerprint(AgentProvider::Custom, Some("http://localhost:11434/v1"));
        assert!(state.needs_load(&ollama));
        state.loaded_for = Some(ollama.clone());
        assert!(!state.needs_load(&ollama));
        // A different address — or a different provider — invalidates it.
        assert!(state.needs_load(&catalog_fingerprint(
            AgentProvider::Custom,
            Some("http://localhost:1234/v1")
        )));
        assert!(state.needs_load(&catalog_fingerprint(AgentProvider::OpenRouter, None)));
    }
}
