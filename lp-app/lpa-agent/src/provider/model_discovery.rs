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
use serde::Deserialize;

use crate::provider::anthropic::{ANTHROPIC_VERSION, AnthropicConfig};
use crate::provider::http_transport::{ByteStream, HttpGetTransport, HttpResponse, TransportError};
use crate::provider::openai_compat::OpenAiCompatConfig;

/// Cap on how much of a `/models` response body is read. OpenRouter's
/// metadata-rich listing runs to a few megabytes; anything past the cap is
/// a parse error rather than an unbounded buffer.
const MAX_MODELS_BODY_BYTES: usize = 8 * 1024 * 1024;

/// One discoverable model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    /// The id the provider expects in requests.
    pub id: String,
    /// Human-readable name when the provider supplies one (Anthropic's
    /// `display_name`, OpenRouter's `name`; absent for OpenAI/Ollama).
    pub display_name: Option<String>,
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
    let page: ModelsPage = serde_json::from_str(&text).map_err(|e| ListModelsError::Parse {
        message: e.to_string(),
    })?;
    Ok(page
        .data
        .into_iter()
        .map(|model| ModelInfo {
            display_name: model.display_name.or(model.name),
            id: model.id,
        })
        .collect())
}

/// The listing shape shared by every provider (unknown fields ignored).
#[derive(Deserialize)]
struct ModelsPage {
    #[serde(default)]
    data: Vec<WireModel>,
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
                    id: "claude-sonnet-5".into(),
                    display_name: Some("Claude Sonnet 5".into()),
                },
                ModelInfo {
                    id: "claude-haiku-4-5".into(),
                    display_name: Some("Claude Haiku 4.5".into()),
                },
            ]
        );
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
                id: "anthropic/claude-sonnet-5".into(),
                display_name: Some("Anthropic: Claude Sonnet 5".into()),
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
