//! Model discovery: plain-GET `/models` listings for Studio's settings
//! dropdowns.
//!
//! Every provider family exposes a models index — Anthropic
//! `GET {base}/v1/models`, OpenAI-compatible `GET {base_url}/models`
//! (OpenAI itself plus Ollama/LM Studio/llama.cpp/vLLM/OpenRouter) — all
//! shaped `{"data": [{"id", …}]}`. Rides the [`HttpGetTransport`] seam so
//! the same code runs over browser fetch and host reqwest. Errors are
//! typed (auth vs network vs parse) so the settings UI can map them onto
//! its per-provider guidance copy.

use futures_util::StreamExt;
use serde::{Deserialize, Deserializer};

use crate::provider::anthropic::{ANTHROPIC_VERSION, AnthropicConfig};
use crate::provider::http_transport::{ByteStream, HttpGetTransport, HttpResponse, TransportError};
use crate::provider::openai_compat::OpenAiCompatConfig;

/// Cap on how much of a `/models` response body is read. OpenRouter's
/// metadata-rich listing runs to a few megabytes; anything past the cap is
/// a parse error rather than an unbounded buffer.
const MAX_MODELS_BODY_BYTES: usize = 8 * 1024 * 1024;

/// One discoverable model.
///
/// Everything past `id`/`display_name` is metadata some providers publish
/// and others do not — kept here because it is what makes a 340-model
/// listing browsable (see `ModelInfo::rank_key` and the settings store's
/// option building). `None` always means "the provider did not say".
#[derive(Clone, Debug, PartialEq)]
pub struct ModelInfo {
    /// The id the provider expects in requests.
    pub id: String,
    /// Human-readable name when the provider supplies one (Anthropic's
    /// `display_name`, OpenRouter's `name`; absent for OpenAI/Ollama).
    pub display_name: Option<String>,
    /// Published rates, $ per million tokens (OpenRouter's per-token
    /// `pricing`, converted).
    pub price: Option<ModelPrice>,
    /// The provider's published score for coding work (OpenRouter carries
    /// Artificial Analysis' indices). The ordering signal: without it a
    /// listing can only be alphabetical, which buries every model worth
    /// picking under `ai21/…`.
    pub coding_score: Option<f64>,
    /// Whether the provider says the model can call tools. The agent is a
    /// single-tool loop, so an explicit `false` cannot run it at all.
    pub supports_tools: Option<bool>,
    /// Publication time (epoch seconds) when the provider gives one.
    pub created: Option<i64>,
}

/// Published rates for a model, $ per million tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

impl ModelInfo {
    /// A model known only by its id — what a bare OpenAI-style listing
    /// gives, and the starting point for fixtures.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: None,
            price: None,
            coding_score: None,
            supports_tools: None,
            created: None,
        }
    }
}

/// Why a model listing failed, typed for guidance mapping (bad key vs
/// unreachable-server/CORS vs unexpected response).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListModelsError {
    /// The credentials were rejected (HTTP 401/403).
    Auth { message: String },
    /// The request never completed (DNS, TLS, refused connection — and in
    /// the browser, a CORS rejection surfaces here as a fetch failure).
    Network { message: String },
    /// Any other non-2xx response.
    Http { status: u16, message: String },
    /// The response body was not the expected models listing.
    Parse { message: String },
}

/// List the models an Anthropic account can use: `GET {base}/v1/models`
/// with the same auth headers the streaming provider sends (including the
/// browser CORS opt-in). One page, `limit=1000` — far above the catalog
/// size, so pagination is not chased.
pub async fn list_anthropic_models<T: HttpGetTransport>(
    config: &AnthropicConfig,
    transport: &T,
) -> Result<Vec<ModelInfo>, ListModelsError> {
    let url = format!(
        "{}/v1/models?limit=1000",
        config.base_url.trim_end_matches('/')
    );
    let headers = vec![
        ("x-api-key".to_string(), config.api_key.clone()),
        (
            "anthropic-version".to_string(),
            ANTHROPIC_VERSION.to_string(),
        ),
        // Required for CORS from the browser; harmless on host.
        (
            "anthropic-dangerous-direct-browser-access".to_string(),
            "true".to_string(),
        ),
    ];
    fetch_models(transport, url, headers).await
}

/// List an OpenAI-compatible server's models: `GET {base_url}/models` with
/// an optional Bearer token. Covers OpenAI, OpenRouter, and the local
/// servers (Ollama, LM Studio, llama.cpp, vLLM) — the same shape
/// everywhere.
pub async fn list_openai_compat_models<T: HttpGetTransport>(
    config: &OpenAiCompatConfig,
    transport: &T,
) -> Result<Vec<ModelInfo>, ListModelsError> {
    let url = format!("{}/models", config.base_url.trim_end_matches('/'));
    let mut headers = Vec::new();
    if let Some(key) = &config.api_key {
        headers.push(("authorization".to_string(), format!("Bearer {key}")));
    }
    headers.extend(config.extra_headers.iter().cloned());
    fetch_models(transport, url, headers).await
}

/// The shared GET → status triage → `data[]` parse.
async fn fetch_models<T: HttpGetTransport>(
    transport: &T,
    url: String,
    headers: Vec<(String, String)>,
) -> Result<Vec<ModelInfo>, ListModelsError> {
    let HttpResponse { status, body } = transport
        .get(url, headers)
        .await
        .map_err(|TransportError { message }| ListModelsError::Network { message })?;
    let text = read_body_capped(body).await;
    if status == 401 || status == 403 {
        return Err(ListModelsError::Auth {
            message: truncated(&text, 300),
        });
    }
    if !(200..300).contains(&status) {
        return Err(ListModelsError::Http {
            status,
            message: truncated(&text, 300),
        });
    }
    parse_models_page(&text)
}

/// Read a `/models` body into [`ModelInfo`]s, in the order the server gave
/// them. Public because the local-server connection probe reads the same
/// bodies and must agree with discovery about what they mean.
pub fn parse_models_page(text: &str) -> Result<Vec<ModelInfo>, ListModelsError> {
    let page: ModelsPage = serde_json::from_str(text).map_err(|e| ListModelsError::Parse {
        message: e.to_string(),
    })?;
    Ok(page
        .data
        .into_iter()
        .map(|model| ModelInfo {
            id: model.id,
            display_name: model.display_name.or(model.name),
            price: model.pricing.and_then(parse_price),
            coding_score: model.benchmarks.and_then(parse_coding_score),
            supports_tools: model
                .supported_parameters
                .map(|parameters| parameters.iter().any(|parameter| parameter == "tools")),
            created: model.created,
        })
        .collect())
}

/// OpenRouter prices are per-token decimal strings; everything downstream
/// speaks $/MTok. A pair is only a price when both halves parse.
fn parse_price(pricing: WirePricing) -> Option<ModelPrice> {
    let per_mtok = |raw: Option<String>| -> Option<f64> {
        let value = raw?.parse::<f64>().ok()?;
        (value.is_finite() && value >= 0.0).then_some(value * 1_000_000.0)
    };
    Some(ModelPrice {
        input_per_mtok: per_mtok(pricing.prompt)?,
        output_per_mtok: per_mtok(pricing.completion)?,
    })
}

/// The published score for the work this agent does, preferring the coding
/// index and falling back to the agentic and general ones. `benchmarks`
/// mixes shapes (`artificial_analysis` is an object of scalars,
/// `design_arena` an array of per-arena entries), so only the object of
/// comparable indices is read.
fn parse_coding_score(benchmarks: WireBenchmarks) -> Option<f64> {
    let analysis = benchmarks.artificial_analysis?;
    [
        analysis.coding_index,
        analysis.agentic_index,
        analysis.intelligence_index,
    ]
    .into_iter()
    .flatten()
    .find(|score| score.is_finite())
}

/// The listing shape shared by every provider (unknown fields ignored).
#[derive(Deserialize)]
struct ModelsPage {
    /// A nullable-but-REQUIRED field, which is the whole distinction:
    ///
    /// - `"data": null` — Ollama with nothing pulled answers exactly this.
    ///   An empty listing, not a failure (and serde's `default` would not
    ///   have covered it: that applies to a MISSING field, not a null one).
    /// - no `data` at all — not a models listing. A body like Ollama's
    ///   native `{"models":[…]}` means the base URL is missing its `/v1`,
    ///   and saying so beats reporting zero models.
    ///
    /// `deserialize_with` rather than a plain `Option` field: serde gives
    /// `Option` an implicit default, which would silently accept the
    /// second case as an empty listing.
    #[serde(deserialize_with = "nullable_model_list")]
    data: Vec<WireModel>,
}

/// A `data` that must be present but may be null (see [`ModelsPage`]).
fn nullable_model_list<'de, D>(deserializer: D) -> Result<Vec<WireModel>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Vec<WireModel>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize)]
struct WireModel {
    id: String,
    /// Anthropic's human-readable name.
    #[serde(default)]
    display_name: Option<String>,
    /// OpenRouter's human-readable name.
    #[serde(default)]
    name: Option<String>,
    /// OpenRouter's per-token rates.
    #[serde(default)]
    pricing: Option<WirePricing>,
    /// OpenRouter's published quality indices.
    #[serde(default)]
    benchmarks: Option<WireBenchmarks>,
    /// OpenRouter's capability list; `tools` is the one that matters here.
    #[serde(default)]
    supported_parameters: Option<Vec<String>>,
    /// Epoch seconds (OpenAI and OpenRouter).
    #[serde(default)]
    created: Option<i64>,
}

#[derive(Deserialize)]
struct WirePricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
}

#[derive(Deserialize)]
struct WireBenchmarks {
    #[serde(default)]
    artificial_analysis: Option<WireIndices>,
}

#[derive(Deserialize)]
struct WireIndices {
    #[serde(default)]
    coding_index: Option<f64>,
    #[serde(default)]
    agentic_index: Option<f64>,
    #[serde(default)]
    intelligence_index: Option<f64>,
}

/// Read a body stream to completion, capped at [`MAX_MODELS_BODY_BYTES`].
async fn read_body_capped(mut body: ByteStream) -> String {
    let mut bytes = Vec::new();
    while let Some(Ok(chunk)) = body.next().await {
        bytes.extend_from_slice(&chunk);
        if bytes.len() >= MAX_MODELS_BODY_BYTES {
            break;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn truncated(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use futures_util::stream;

    use super::*;
    use crate::provider::http_transport::LocalBoxFuture;

    const ANTHROPIC_PAGE: &str = r#"{
        "data": [
            {"type": "model", "id": "claude-sonnet-5", "display_name": "Claude Sonnet 5", "created_at": "2026-05-01T00:00:00Z"},
            {"type": "model", "id": "claude-haiku-4-5", "display_name": "Claude Haiku 4.5", "created_at": "2025-11-01T00:00:00Z"}
        ],
        "has_more": false,
        "first_id": "claude-sonnet-5",
        "last_id": "claude-haiku-4-5"
    }"#;

    const OPENAI_PAGE: &str = r#"{
        "object": "list",
        "data": [
            {"id": "gpt-5.2", "object": "model", "created": 1755000000, "owned_by": "openai"},
            {"id": "gpt-5.2-mini", "object": "model", "created": 1755000000, "owned_by": "openai"}
        ]
    }"#;

    const OLLAMA_PAGE: &str = r#"{
        "object": "list",
        "data": [
            {"id": "llama3.2:latest", "object": "model", "created": 1721000000, "owned_by": "library"}
        ]
    }"#;

    const OPENROUTER_PAGE: &str = r#"{
        "data": [
            {"id": "anthropic/claude-sonnet-5", "name": "Anthropic: Claude Sonnet 5", "context_length": 200000, "pricing": {"prompt": "0.000003"}}
        ]
    }"#;

    #[test]
    fn anthropic_listing_parses_ids_and_display_names() {
        let transport = FakeGet::ok(ANTHROPIC_PAGE);
        let models = block_on_anthropic(&transport).expect("models");
        assert_eq!(
            models,
            vec![
                ModelInfo {
                    display_name: Some("Claude Sonnet 5".into()),
                    ..ModelInfo::new("claude-sonnet-5")
                },
                ModelInfo {
                    display_name: Some("Claude Haiku 4.5".into()),
                    ..ModelInfo::new("claude-haiku-4-5")
                },
            ]
        );
    }

    #[test]
    fn a_null_data_list_is_an_empty_listing_not_a_parse_error() {
        // Ollama with nothing pulled answers exactly this, and serde's
        // `default` covers a missing field, not an explicit null.
        let models = parse_models_page(r#"{"object":"list","data":null}"#).expect("empty listing");
        assert!(models.is_empty());
    }

    #[test]
    fn openrouter_metadata_rides_along_for_ranking_and_pricing() {
        // Field shapes taken from a live openrouter.ai/api/v1/models body:
        // per-token decimal strings, indices under `artificial_analysis`,
        // and a sibling `design_arena` ARRAY that must not break the read.
        let models = parse_models_page(
            r#"{"data":[{
                "id":"anthropic/claude-opus-5",
                "name":"Anthropic: Claude Opus 5",
                "created":1785190561,
                "pricing":{"prompt":"0.000005","completion":"0.000025"},
                "supported_parameters":["tools","max_tokens"],
                "benchmarks":{
                    "design_arena":[{"arena":"models","category":"3d","elo":1387}],
                    "artificial_analysis":{"intelligence_index":60.7,"coding_index":78,"agentic_index":55.3}}
            },{
                "id":"some/embedding-only",
                "supported_parameters":["max_tokens"]
            }]}"#,
        )
        .expect("models");
        let opus = &models[0];
        let price = opus.price.expect("pricing");
        assert!((price.input_per_mtok - 5.0).abs() < 1e-9, "{price:?}");
        assert!((price.output_per_mtok - 25.0).abs() < 1e-9, "{price:?}");
        assert_eq!(opus.coding_score, Some(78.0));
        assert_eq!(opus.supports_tools, Some(true));
        assert_eq!(opus.created, Some(1785190561));
        assert_eq!(models[1].supports_tools, Some(false));
        assert_eq!(models[1].price, None);
    }

    #[test]
    fn the_score_falls_back_through_the_published_indices() {
        let models = parse_models_page(
            r#"{"data":[
                {"id":"a","benchmarks":{"artificial_analysis":{"agentic_index":50,"intelligence_index":40}}},
                {"id":"b","benchmarks":{"artificial_analysis":{"intelligence_index":45}}},
                {"id":"c","benchmarks":{"design_arena":[{"arena":"models","elo":1300}]}}
            ]}"#,
        )
        .expect("models");
        assert_eq!(models[0].coding_score, Some(50.0));
        assert_eq!(models[1].coding_score, Some(45.0));
        // Per-arena elo is not a comparable index; no score rather than a
        // number that would sort against a different scale.
        assert_eq!(models[2].coding_score, None);
    }

    #[test]
    fn anthropic_request_carries_auth_version_and_browser_headers() {
        let transport = FakeGet::ok(ANTHROPIC_PAGE);
        let _ = block_on_anthropic(&transport);
        let requests = transport.requests.borrow();
        let (url, headers) = &requests[0];
        assert_eq!(url, "https://api.anthropic.com/v1/models?limit=1000");
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "x-api-key" && v == "sk-test")
        );
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "anthropic-version" && v == ANTHROPIC_VERSION)
        );
        assert!(
            headers
                .iter()
                .any(|(k, _)| k == "anthropic-dangerous-direct-browser-access")
        );
    }

    #[test]
    fn openai_listing_parses_plain_ids() {
        let transport = FakeGet::ok(OPENAI_PAGE);
        let models = block_on_compat(&transport, Some("sk-oai"), vec![]).expect("models");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-5.2");
        assert_eq!(models[0].display_name, None);
        let requests = transport.requests.borrow();
        let (url, headers) = &requests[0];
        assert_eq!(url, "http://compat.test/v1/models");
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer sk-oai")
        );
    }

    #[test]
    fn ollama_listing_parses_and_sends_no_auth_header() {
        let transport = FakeGet::ok(OLLAMA_PAGE);
        let models = block_on_compat(&transport, None, vec![]).expect("models");
        assert_eq!(models[0].id, "llama3.2:latest");
        let requests = transport.requests.borrow();
        assert!(
            !requests[0].1.iter().any(|(k, _)| k == "authorization"),
            "keyless local servers must get no Authorization header"
        );
    }

    #[test]
    fn openrouter_name_maps_to_display_name_and_extra_headers_ride() {
        let transport = FakeGet::ok(OPENROUTER_PAGE);
        let extra = vec![("HTTP-Referer".to_string(), "https://app.test".to_string())];
        let models = block_on_compat(&transport, Some("sk-or"), extra).expect("models");
        assert_eq!(
            models,
            vec![ModelInfo {
                display_name: Some("Anthropic: Claude Sonnet 5".into()),
                ..ModelInfo::new("anthropic/claude-sonnet-5")
            }]
        );
        let requests = transport.requests.borrow();
        assert!(requests[0].1.iter().any(|(k, _)| k == "HTTP-Referer"));
    }

    #[test]
    fn unauthorized_status_types_as_auth() {
        let transport = FakeGet::respond(
            401,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
        );
        let error = block_on_anthropic(&transport).expect_err("auth error");
        assert!(
            matches!(&error, ListModelsError::Auth { message } if message.contains("invalid x-api-key")),
            "{error:?}"
        );
    }

    #[test]
    fn server_error_status_types_as_http() {
        let transport = FakeGet::respond(500, "boom");
        let error = block_on_compat(&transport, None, vec![]).expect_err("http error");
        assert!(
            matches!(&error, ListModelsError::Http { status: 500, message } if message == "boom"),
            "{error:?}"
        );
    }

    #[test]
    fn transport_failure_types_as_network() {
        let transport = FakeGet::fail("Failed to fetch");
        let error = block_on_compat(&transport, None, vec![]).expect_err("network error");
        assert!(
            matches!(&error, ListModelsError::Network { message } if message == "Failed to fetch"),
            "{error:?}"
        );
    }

    #[test]
    fn malformed_body_types_as_parse() {
        let transport = FakeGet::ok("<html>not json</html>");
        let error = block_on_anthropic(&transport).expect_err("parse error");
        assert!(matches!(error, ListModelsError::Parse { .. }), "{error:?}");
    }

    #[test]
    fn base_url_trailing_slash_is_tolerated() {
        let transport = FakeGet::ok(OLLAMA_PAGE);
        let config = OpenAiCompatConfig {
            base_url: "http://localhost:11434/v1/".to_string(),
            api_key: None,
            model: String::new(),
            extra_headers: vec![],
        };
        let _ = futures_executor::block_on(list_openai_compat_models(&config, &&transport));
        assert_eq!(
            transport.requests.borrow()[0].0,
            "http://localhost:11434/v1/models"
        );
    }

    // -- helpers ----------------------------------------------------------

    fn block_on_anthropic(transport: &FakeGet) -> Result<Vec<ModelInfo>, ListModelsError> {
        let config = AnthropicConfig::new("sk-test");
        futures_executor::block_on(list_anthropic_models(&config, &transport))
    }

    fn block_on_compat(
        transport: &FakeGet,
        api_key: Option<&str>,
        extra_headers: Vec<(String, String)>,
    ) -> Result<Vec<ModelInfo>, ListModelsError> {
        let config = OpenAiCompatConfig {
            base_url: "http://compat.test/v1".to_string(),
            api_key: api_key.map(str::to_string),
            model: String::new(),
            extra_headers,
        };
        futures_executor::block_on(list_openai_compat_models(&config, &transport))
    }

    enum Scripted {
        Fail(String),
        Respond { status: u16, body: String },
    }

    struct FakeGet {
        script: Scripted,
        requests: RefCell<Vec<(String, Vec<(String, String)>)>>,
    }

    impl FakeGet {
        fn ok(body: &str) -> Self {
            Self::respond(200, body)
        }

        fn respond(status: u16, body: &str) -> Self {
            Self {
                script: Scripted::Respond {
                    status,
                    body: body.to_string(),
                },
                requests: RefCell::new(Vec::new()),
            }
        }

        fn fail(message: &str) -> Self {
            Self {
                script: Scripted::Fail(message.to_string()),
                requests: RefCell::new(Vec::new()),
            }
        }
    }

    impl HttpGetTransport for &FakeGet {
        fn get(
            &self,
            url: String,
            headers: Vec<(String, String)>,
        ) -> LocalBoxFuture<'static, Result<HttpResponse, TransportError>> {
            self.requests.borrow_mut().push((url, headers));
            let result = match &self.script {
                Scripted::Fail(message) => Err(TransportError::new(message.clone())),
                Scripted::Respond { status, body } => Ok((*status, body.clone())),
            };
            Box::pin(async move {
                let (status, body) = result?;
                Ok(HttpResponse {
                    status,
                    body: Box::pin(stream::iter(vec![Ok(body.into_bytes())])),
                })
            })
        }
    }
}
