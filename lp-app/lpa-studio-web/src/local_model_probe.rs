//! Browser glue for local-model discovery: the fetches behind the settings
//! popover's Test and Scan buttons.
//!
//! Every decision — which ports, how to normalize a URL, what a result
//! means, what copy to show — lives in
//! [`lpa_studio_core::app::settings::local_model_probe`]. This file only
//! performs IO and reports [`ProbeOutcome`]s back.
//!
//! The one browser-specific trick worth knowing: a cross-origin `fetch` that
//! a server answers *without* CORS headers fails identically to a fetch at a
//! port where nothing listens — same `TypeError`, no status, no headers. So
//! every failure is retried once in `no-cors` mode, which resolves (with an
//! unreadable opaque response) whenever the connection was actually
//! accepted. Opaque success after a normal failure means "the server is
//! there, it just did not allow this page" — the single most common local-
//! model setup problem, and otherwise indistinguishable from a wrong port.

#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "the call sites are wasm-only glue; the decisions are host-tested in core"
    )
)]

use lpa_studio_core::app::settings::local_model_probe::{
    self as probe, LocalModelProbeState, ProbeLevel,
};

/// How long one attempt may take. A refused connection fails in
/// milliseconds; this bound is for addresses that swallow packets (a
/// firewalled box, a stale VPN route) so a scan cannot hang the UI.
const PROBE_TIMEOUT_MS: u32 = 3_000;

/// What the UI asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeRequest {
    /// Test one base URL (the field's current value).
    TestBaseUrl(String),
    /// Try every well-known local server port.
    ScanCommonPorts,
}

/// The in-flight state to render while a request runs.
pub fn running_state(request: &ProbeRequest) -> LocalModelProbeState {
    LocalModelProbeState {
        running: true,
        running_label: Some(match request {
            ProbeRequest::TestBaseUrl(url) => match probe::normalize_base_url(url) {
                Some(url) => format!("Testing {url}…"),
                None => "Testing…".to_string(),
            },
            ProbeRequest::ScanCommonPorts => format!(
                "Looking for local servers on {} common ports…",
                probe::COMMON_LOCAL_SERVERS.len()
            ),
        }),
        summary: None,
        findings: Vec::new(),
    }
}

/// The state for a Test with nothing in the base-URL field.
pub fn empty_url_state() -> LocalModelProbeState {
    LocalModelProbeState {
        running: false,
        running_label: None,
        summary: Some(probe::ProbeSummary {
            level: ProbeLevel::Error,
            headline: "Enter a base URL first".to_string(),
            detail: Some("Or press Find my server and let Studio look for one itself.".to_string()),
        }),
        findings: Vec::new(),
    }
}

#[cfg(target_arch = "wasm32")]
pub use glue::run;

#[cfg(target_arch = "wasm32")]
mod glue {
    use futures_util::future::{Either, join_all, select};
    use gloo_net::http::Request;
    use gloo_timers::future::TimeoutFuture;
    use lpa_studio_core::app::settings::local_model_probe::{
        BrowserFacts, ProbeFinding, ProbeOutcome,
    };
    use web_sys::RequestMode;

    use super::*;

    /// Run one probe request to completion.
    pub async fn run(
        request: ProbeRequest,
        api_key: Option<String>,
        configured_model: Option<String>,
    ) -> LocalModelProbeState {
        let facts = browser_facts();
        let key = api_key.as_deref();
        let model = configured_model.as_deref();
        match request {
            ProbeRequest::TestBaseUrl(url) => {
                let Some(url) = probe::normalize_base_url(&url) else {
                    return empty_url_state();
                };
                let findings = probe_with_ipv4_fallback(&url, key, &facts, model).await;
                LocalModelProbeState {
                    running: false,
                    running_label: None,
                    // One target speaks for itself; the finding carries the
                    // headline, so a summary would only repeat it.
                    summary: None,
                    findings,
                }
            }
            ProbeRequest::ScanCommonPorts => {
                let probes = probe::COMMON_LOCAL_SERVERS
                    .iter()
                    .map(|server| probe_with_ipv4_fallback(server.base_url, key, &facts, model));
                let mut findings: Vec<ProbeFinding> =
                    join_all(probes).await.into_iter().flatten().collect();
                let summary = probe::scan_summary(&findings, &facts);
                // Dead ports are the expected case and say nothing once
                // something better exists — the summary already counts them.
                if findings.iter().any(|f| f.level != ProbeLevel::Error) {
                    findings.retain(|f| f.level != ProbeLevel::Error);
                }
                findings.sort_by_key(|f| match f.level {
                    ProbeLevel::Ok => 0,
                    ProbeLevel::Warn => 1,
                    ProbeLevel::Error => 2,
                });
                LocalModelProbeState {
                    running: false,
                    running_label: None,
                    summary: Some(summary),
                    findings,
                }
            }
        }
    }

    /// Probe one base URL, and — when nothing answered at a `localhost`
    /// address — the same port at `127.0.0.1`. A server bound to the IPv4
    /// loopback is invisible to a browser that resolved `localhost` to `::1`,
    /// and the retry turns that dead end into a working address.
    async fn probe_with_ipv4_fallback(
        base_url: &str,
        api_key: Option<&str>,
        facts: &BrowserFacts,
        configured_model: Option<&str>,
    ) -> Vec<ProbeFinding> {
        let outcome = probe_once(base_url, api_key).await;
        let unreachable = matches!(outcome, ProbeOutcome::Unreachable { .. });
        let first = probe::diagnose(base_url, outcome, facts, configured_model);
        if !unreachable {
            return vec![first];
        }
        let Some(ipv4) = probe::ipv4_loopback_variant(base_url) else {
            return vec![first];
        };
        let outcome = probe_once(&ipv4, api_key).await;
        if matches!(outcome, ProbeOutcome::Unreachable { .. }) {
            // Both spellings are dead: report the address the user knows.
            return vec![first];
        }
        vec![probe::diagnose(&ipv4, outcome, facts, configured_model)]
    }

    /// One `GET {base_url}/models`, classified.
    async fn probe_once(base_url: &str, api_key: Option<&str>) -> ProbeOutcome {
        let url = probe::models_url(base_url);
        match fetch_models(&url, api_key).await {
            Some(Ok((status, body))) if (200..300).contains(&status) => {
                if body.is_empty() {
                    return ProbeOutcome::BadBody(
                        "The server answered, but sent no body to read.".to_string(),
                    );
                }
                match probe::parse_models(&body) {
                    Ok(models) => ProbeOutcome::Models(models),
                    Err(detail) => ProbeOutcome::BadBody(detail),
                }
            }
            Some(Ok((status, body))) => ProbeOutcome::Status { status, body },
            // A failed fetch is ambiguous: no server, or a server whose
            // answer the browser refused to hand us. The opaque probe tells
            // them apart.
            Some(Err(detail)) => match opaque_reachable(&url).await {
                true => ProbeOutcome::CorsBlocked,
                false => ProbeOutcome::Unreachable { detail },
            },
            None => ProbeOutcome::Unreachable {
                detail: format!("no answer within {}s", PROBE_TIMEOUT_MS / 1000),
            },
        }
    }

    /// `Some(Ok(status, body))` for a real response, `Some(Err(message))` for
    /// a fetch that never produced one, `None` on timeout.
    async fn fetch_models(
        url: &str,
        api_key: Option<&str>,
    ) -> Option<Result<(u16, String), String>> {
        // No key ⇒ no request headers at all, which keeps this a CORS-simple
        // GET: no preflight to fail on servers that ignore OPTIONS.
        let mut request = Request::get(url).mode(RequestMode::Cors);
        if let Some(key) = api_key {
            request = request.header("authorization", &format!("Bearer {key}"));
        }
        let sent = match request.build() {
            Ok(request) => request,
            Err(e) => return Some(Err(format!("{e}"))),
        };
        let response = match with_timeout(sent.send()).await? {
            Ok(response) => response,
            Err(e) => return Some(Err(fetch_failure_message(&e.to_string()))),
        };
        let status = response.status();
        // An unreadable body is still an answer — the address and status are
        // the useful facts, and `probe_once` reports the empty body honestly
        // rather than as a malformed model list.
        let body = with_timeout(response.text()).await?.unwrap_or_default();
        Some(Ok((status, body)))
    }

    /// Whether the port accepts connections at all, read through an opaque
    /// `no-cors` response (unreadable by design — resolving is the signal).
    async fn opaque_reachable(url: &str) -> bool {
        let Ok(request) = Request::get(url).mode(RequestMode::NoCors).build() else {
            return false;
        };
        matches!(with_timeout(request.send()).await, Some(Ok(_)))
    }

    /// Run a future under [`PROBE_TIMEOUT_MS`]; `None` if it did not finish.
    async fn with_timeout<T>(future: impl core::future::Future<Output = T>) -> Option<T> {
        let future = core::pin::pin!(future);
        match select(future, TimeoutFuture::new(PROBE_TIMEOUT_MS)).await {
            Either::Left((value, _)) => Some(value),
            Either::Right(_) => None,
        }
    }

    /// Browser fetch errors are deliberately uninformative ("TypeError:
    /// Failed to fetch"). Keep them short — the diagnosis carries the
    /// meaning, this is just the raw trace.
    fn fetch_failure_message(raw: &str) -> String {
        raw.split_once("error: ")
            .map_or(raw, |(_, rest)| rest)
            .trim()
            .chars()
            .take(120)
            .collect()
    }

    /// The page facts a diagnosis needs: our origin (what a server has to
    /// allow), whether we are https (mixed-content and local-network rules),
    /// and whether this is Safari (stricter than the rest about
    /// http://localhost).
    fn browser_facts() -> BrowserFacts {
        let Some(window) = web_sys::window() else {
            return BrowserFacts::default();
        };
        let location = window.location();
        let user_agent = window.navigator().user_agent().unwrap_or_default();
        BrowserFacts {
            page_origin: location.origin().unwrap_or_default(),
            page_is_https: location
                .protocol()
                .is_ok_and(|protocol| protocol.starts_with("https")),
            // Chromium UAs also carry "Safari"; the Chrome/Chromium tokens
            // are what tell them apart.
            is_safari: user_agent.contains("Safari")
                && !user_agent.contains("Chrome")
                && !user_agent.contains("Chromium"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_labels_name_the_target() {
        let state = running_state(&ProbeRequest::TestBaseUrl("localhost:11434".to_string()));
        assert!(state.running);
        assert_eq!(
            state.running_label.as_deref(),
            Some("Testing http://localhost:11434/v1…")
        );
        let state = running_state(&ProbeRequest::ScanCommonPorts);
        assert!(state.running_label.expect("label").contains(&format!(
            "{} common ports",
            probe::COMMON_LOCAL_SERVERS.len()
        )));
    }

    #[test]
    fn a_blank_url_asks_for_one_instead_of_probing() {
        let state = empty_url_state();
        assert!(!state.running);
        assert!(state.findings.is_empty());
        assert_eq!(
            state.summary.expect("summary").headline,
            "Enter a base URL first"
        );
    }
}
