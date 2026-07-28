//! Browser glue for the model picker: one GET per provider, then the core
//! catalog parser.
//!
//! The endpoints, the auth headers, the id filter and the copy all live in
//! [`lpa_studio_core::app::settings::model_catalog`]. This file sends the
//! request and turns whatever came back into the picker's state.

#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "the call sites are wasm-only glue; the decisions are host-tested in core"
    )
)]

use lpa_studio_core::AgentProvider;
use lpa_studio_core::app::settings::model_catalog::ModelCatalogState;

/// A list request may sit behind a cold local server or a slow API, and
/// OpenRouter's catalog alone is ~340 models / half a megabyte — long enough
/// for that on a bad connection, short enough that the picker never looks
/// stuck.
const CATALOG_TIMEOUT_MS: u32 = 15_000;

/// What the picker needs to fetch one provider's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogQuery {
    pub provider: AgentProvider,
    /// The Custom provider's base URL (ignored by the others).
    pub base_url: Option<String>,
    /// The effective key for the selected provider.
    pub api_key: Option<String>,
}

/// The state to render while a list is loading, keeping any already-loaded
/// catalog visible underneath (a refresh must not blank the list).
pub fn loading_state(previous: &ModelCatalogState) -> ModelCatalogState {
    ModelCatalogState {
        open: true,
        loading: true,
        error: None,
        catalog: previous.catalog.clone(),
        loaded_for: previous.loaded_for.clone(),
    }
}

#[cfg(target_arch = "wasm32")]
pub use glue::load;

#[cfg(target_arch = "wasm32")]
mod glue {
    use futures_util::future::{Either, select};
    use gloo_net::http::Request;
    use gloo_timers::future::TimeoutFuture;
    use lpa_studio_core::app::settings::model_catalog as catalog;

    use super::*;

    /// Fetch and parse one provider's model list.
    pub async fn load(query: CatalogQuery) -> ModelCatalogState {
        let fingerprint = catalog::catalog_fingerprint(query.provider, query.base_url.as_deref());
        let settled = |error: Option<String>, models: Option<catalog::ModelCatalog>| {
            ModelCatalogState {
                open: true,
                loading: false,
                error,
                // Only a successful load claims the fingerprint, so a failed
                // attempt is retried rather than cached as "loaded".
                loaded_for: models.is_some().then(|| fingerprint.clone()),
                catalog: models,
            }
        };
        let request = match catalog::catalog_request(
            query.provider,
            query.base_url.as_deref(),
            query.api_key.as_deref(),
        ) {
            Ok(request) => request,
            Err(reason) => return settled(Some(reason), None),
        };
        match fetch(&request).await {
            Ok((status, body)) if (200..300).contains(&status) => {
                match catalog::parse_catalog(&body) {
                    Ok(models) => settled(None, Some(models)),
                    Err(detail) => settled(Some(detail), None),
                }
            }
            Ok((status, body)) => settled(Some(status_message(status, &body)), None),
            Err(detail) => settled(Some(detail), None),
        }
    }

    async fn fetch(request: &catalog::ModelCatalogRequest) -> Result<(u16, String), String> {
        let mut builder = Request::get(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let sent = builder.build().map_err(|e| format!("{e}"))?;
        let response = match with_timeout(sent.send()).await {
            Some(Ok(response)) => response,
            Some(Err(error)) => return Err(network_message(&request.url, &error.to_string())),
            None => return Err("the provider did not answer in time.".to_string()),
        };
        let status = response.status();
        // A body that never finished arriving must say so: treating the empty
        // string as the answer reports a half-megabyte download that timed
        // out as "the response was not JSON".
        let body = match with_timeout(response.text()).await {
            Some(Ok(body)) => body,
            Some(Err(error)) => {
                return Err(format!(
                    "The provider's answer could not be read ({}).",
                    trim(&error.to_string(), 120)
                ));
            }
            None => {
                return Err(format!(
                    "The provider answered {status}, but the model list did not \
                     finish downloading in {}s — try again.",
                    CATALOG_TIMEOUT_MS / 1000
                ));
            }
        };
        Ok((status, body))
    }

    /// A failed fetch says nothing useful on its own; name the likely cause
    /// for the address that failed.
    fn network_message(url: &str, raw: &str) -> String {
        if url.starts_with("http://") {
            return format!(
                "Could not reach {url} — is the server running, and does it allow \
                 this page? Use Test connection above for a full diagnosis."
            );
        }
        format!("Could not reach the provider ({}).", trim(raw, 120))
    }

    fn status_message(status: u16, body: &str) -> String {
        let reason = serde_json::from_str::<serde_json::Value>(body.trim())
            .ok()
            .and_then(|json| {
                let error = json.get("error")?;
                error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .or_else(|| error.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| trim(body, 120));
        match status {
            401 | 403 => format!("The provider rejected the key ({status}): {reason}"),
            404 => format!("No model list at this address ({status}). Check the base URL."),
            _ => format!("The provider answered {status}: {reason}"),
        }
    }

    async fn with_timeout<T>(future: impl core::future::Future<Output = T>) -> Option<T> {
        let future = core::pin::pin!(future);
        match select(future, TimeoutFuture::new(CATALOG_TIMEOUT_MS)).await {
            Either::Left((value, _)) => Some(value),
            Either::Right(_) => None,
        }
    }

    fn trim(text: &str, max: usize) -> String {
        text.trim().chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::app::settings::model_catalog::{CatalogModel, ModelCatalog};

    #[test]
    fn a_refresh_keeps_the_current_list_on_screen() {
        let loaded = ModelCatalogState {
            open: true,
            loading: false,
            error: Some("stale failure".to_string()),
            catalog: Some(ModelCatalog {
                models: vec![CatalogModel {
                    id: "llama3.2".to_string(),
                    label: None,
                    price: None,
                }],
                hidden: 0,
            }),
            loaded_for: Some("Custom|http://localhost:11434/v1".to_string()),
        };
        let loading = loading_state(&loaded);
        assert!(loading.loading && loading.open);
        assert_eq!(loading.catalog, loaded.catalog);
        // The previous attempt's error must not sit next to a spinner.
        assert_eq!(loading.error, None);
    }
}
