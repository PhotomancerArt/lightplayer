//! Local OpenAI-compatible server discovery and connection diagnosis.
//!
//! Pointing the Custom provider at a local model server fails in a handful
//! of specific ways, and a browser reports almost all of them as the same
//! opaque "fetch failed". This module owns the decision layer that turns a
//! probe result into an actionable sentence: which ports are worth trying,
//! how a base URL should be normalized, and — crucially — the difference
//! between *nothing is listening* and *the server answered but the browser
//! dropped the response because CORS headers were missing*.
//!
//! Pure and host-tested. The platform edge (`lpa-studio-web`) performs the
//! actual fetches and reports each attempt back as a [`ProbeOutcome`]; the
//! CORS-vs-unreachable discrimination it feeds us comes from a second,
//! opaque `no-cors` request — a request that succeeds whenever the port
//! accepted the connection, whatever headers came back.

use serde_json::Value;

/// A well-known local server: where it listens by default, and how to let a
/// browser page talk to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalServer {
    /// Product name, as the user knows it.
    pub label: &'static str,
    /// Default OpenAI-compatible base URL.
    pub base_url: &'static str,
    /// How to allow this page's origin, with `{origin}` standing in for it.
    pub cors_fix: &'static str,
}

/// The ports a scan tries, in order. Every entry speaks the OpenAI
/// `/chat/completions` dialect at the listed base URL.
pub const COMMON_LOCAL_SERVERS: &[LocalServer] = &[
    LocalServer {
        label: "Ollama",
        base_url: "http://localhost:11434/v1",
        cors_fix: "Restart it with this page allowed: \
                   OLLAMA_ORIGINS={origin} ollama serve",
    },
    LocalServer {
        label: "LM Studio",
        base_url: "http://localhost:1234/v1",
        cors_fix: "In LM Studio's Developer → Local Server settings, turn on \
                   CORS, then restart the server.",
    },
    LocalServer {
        label: "llama.cpp (llama-server)",
        base_url: "http://localhost:8080/v1",
        cors_fix: "llama-server allows every origin by default — if it is \
                   behind a proxy, allow {origin} there.",
    },
    LocalServer {
        label: "vLLM",
        base_url: "http://localhost:8000/v1",
        cors_fix: "Start it with --allowed-origins '[\"{origin}\"]'.",
    },
    LocalServer {
        label: "Jan",
        base_url: "http://localhost:1337/v1",
        cors_fix: "Allow {origin} in Jan's local API server settings.",
    },
];

/// What one attempt against a base URL produced. Built by the platform
/// edge; the copy is this module's job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// A 2xx model list.
    Models(Vec<String>),
    /// A 2xx body that is not an OpenAI model list.
    BadBody(String),
    /// A real HTTP response carrying a non-2xx status.
    Status { status: u16, body: String },
    /// The response never became readable, but the port accepted the
    /// connection — the opaque probe went through. That is CORS.
    CorsBlocked,
    /// Nothing answered, or the browser refused to make the request.
    Unreachable { detail: String },
}

/// How a finding reads: reachable and usable, reachable but blocked or
/// misconfigured, or nothing there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeLevel {
    Ok,
    Warn,
    Error,
}

/// Which of the local-server failure modes a finding is. The summary and
/// the UI branch on this rather than on the copy, so rewording a sentence
/// can never change behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingKind {
    /// A model list came back and the configured model (if any) is served.
    Usable,
    /// A model list came back, but the configured model is not in it.
    WrongModel,
    /// A readable answer with no models in it (nothing pulled/loaded).
    NoModels,
    /// Reachable, but the browser would not let this page read the answer.
    Cors,
    /// A real HTTP response with a non-2xx status.
    HttpStatus,
    /// A readable answer that is not an OpenAI model list.
    BadBody,
    /// Nothing accepted the connection.
    Unreachable,
}

/// One diagnosed attempt, ready to render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeFinding {
    /// The URL that was probed (already normalized).
    pub base_url: String,
    /// The product name, when the URL is a known default.
    pub server_label: Option<String>,
    pub kind: FindingKind,
    pub level: ProbeLevel,
    /// One short sentence: what happened.
    pub headline: String,
    /// Optional second line: why, or what it means.
    pub detail: Option<String>,
    /// Optional copy-pasteable remedy.
    pub fix: Option<String>,
    /// Model ids the server serves (empty unless it answered with a list).
    pub models: Vec<String>,
}

impl ProbeFinding {
    /// Whether this server can run the agent as configured.
    pub fn is_usable(&self) -> bool {
        self.kind == FindingKind::Usable
    }
}

/// The browser facts a diagnosis needs, sampled by the platform edge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowserFacts {
    /// This page's origin, e.g. `https://lightplayer.app` — the exact
    /// string a server's allow-list needs.
    pub page_origin: String,
    /// The page itself is served over https (so `http://…` targets are
    /// subject to the browser's mixed-content and local-network rules).
    pub page_is_https: bool,
    /// WebKit/Safari, which refuses plain-http localhost requests from an
    /// https page where Chrome and Firefox allow them.
    pub is_safari: bool,
}

/// The headline over a set of findings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeSummary {
    pub level: ProbeLevel,
    pub headline: String,
    pub detail: Option<String>,
}

/// The probe slice of the settings view: what the popover renders under the
/// base-URL field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalModelProbeState {
    /// A probe is in flight (buttons disabled, spinner copy shown).
    pub running: bool,
    /// What is being probed right now, e.g. `Testing http://localhost:…`.
    pub running_label: Option<String>,
    pub summary: Option<ProbeSummary>,
    /// Findings worth showing, most useful first.
    pub findings: Vec<ProbeFinding>,
}

/// Bring a hand-typed base URL to the shape the provider needs: a scheme, no
/// trailing slash, and the `/v1` prefix these servers serve their
/// OpenAI-compatible routes under. Returns `None` for blank input.
///
/// The `/v1` completion matters: `http://localhost:11434` is the address
/// Ollama prints, and it 404s every OpenAI-compatible path.
pub fn normalize_base_url(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    // Append `/v1` only when no path was given at all — a user who typed
    // `/v1beta` or `/openai/v1` means it.
    let after_scheme = with_scheme.split_once("://").map_or("", |(_, rest)| rest);
    if after_scheme.contains('/') {
        Some(with_scheme)
    } else {
        Some(format!("{with_scheme}/v1"))
    }
}

/// The model-list URL for a base URL.
pub fn models_url(base_url: &str) -> String {
    format!("{}/models", base_url.trim_end_matches('/'))
}

/// The same URL against the IPv4 loopback literal. A server bound to
/// `127.0.0.1` is unreachable at `localhost` whenever the browser resolves
/// that name to `::1` first — one of the more baffling local-server
/// failures, and free to rule out.
pub fn ipv4_loopback_variant(base_url: &str) -> Option<String> {
    let (scheme, rest) = base_url.split_once("://")?;
    let host_end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(host_end);
    let port = authority.split_once(':').map(|(_, port)| port);
    if !authority.starts_with("localhost") {
        return None;
    }
    Some(match port {
        Some(port) => format!("{scheme}://127.0.0.1:{port}{path}"),
        None => format!("{scheme}://127.0.0.1{path}"),
    })
}

/// Whether a URL points at this machine.
pub fn is_loopback_url(base_url: &str) -> bool {
    let host = base_url
        .split_once("://")
        .map_or(base_url, |(_, rest)| rest)
        .split('/')
        .next()
        .unwrap_or_default();
    let host = host.rsplit_once(':').map_or(host, |(host, _)| host);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
        || host.ends_with(".localhost")
        || host.starts_with("127.")
}

/// The known server whose default base URL this is, if any.
pub fn known_server(base_url: &str) -> Option<&'static LocalServer> {
    let normalized = normalize_base_url(base_url)?;
    COMMON_LOCAL_SERVERS.iter().find(|server| {
        server.base_url == normalized
            || ipv4_loopback_variant(server.base_url).as_deref() == Some(normalized.as_str())
    })
}

/// The CORS remedy for a URL: the known server's own instructions when we
/// recognize it, the protocol-level requirement otherwise.
pub fn cors_fix_for(base_url: &str, page_origin: &str) -> String {
    let origin = if page_origin.is_empty() {
        "this page's origin"
    } else {
        page_origin
    };
    match known_server(base_url) {
        Some(server) => server.cors_fix.replace("{origin}", origin),
        None => format!(
            "The server must answer with Access-Control-Allow-Origin: {origin} \
             (or *) — check its CORS / allowed-origins setting."
        ),
    }
}

/// Turn one attempt into rendered copy.
///
/// `configured_model` is the model id the settings currently name, so a
/// server that answers but does not serve that id is reported as the
/// misconfiguration it is rather than a success.
pub fn diagnose(
    base_url: &str,
    outcome: ProbeOutcome,
    facts: &BrowserFacts,
    configured_model: Option<&str>,
) -> ProbeFinding {
    let server_label = known_server(base_url).map(|server| server.label.to_string());
    let finding = |kind: FindingKind,
                   level: ProbeLevel,
                   headline: String,
                   detail: Option<String>,
                   fix: Option<String>,
                   models: Vec<String>| ProbeFinding {
        base_url: base_url.to_string(),
        server_label: server_label.clone(),
        kind,
        level,
        headline,
        detail,
        fix,
        models,
    };
    match outcome {
        ProbeOutcome::Models(models) if models.is_empty() => finding(
            FindingKind::NoModels,
            ProbeLevel::Warn,
            "Answered, but serves no models".to_string(),
            Some("The server is running and reachable — it just has nothing loaded.".to_string()),
            Some("Pull or load a model first (Ollama: ollama pull qwen3-coder:30b).".to_string()),
            models,
        ),
        ProbeOutcome::Models(models) => {
            let count = models.len();
            let plural = if count == 1 { "model" } else { "models" };
            let missing = configured_model
                .filter(|model| !models.iter().any(|served| served == model))
                .map(str::to_string);
            match missing {
                Some(model) => finding(
                    FindingKind::WrongModel,
                    ProbeLevel::Warn,
                    format!("Connected — but this server does not serve “{model}”"),
                    Some(format!("It offers {count} other {plural}; pick one below.")),
                    None,
                    models,
                ),
                None => finding(
                    FindingKind::Usable,
                    ProbeLevel::Ok,
                    format!("Connected — {count} {plural} available"),
                    None,
                    None,
                    models,
                ),
            }
        }
        ProbeOutcome::CorsBlocked => finding(
            FindingKind::Cors,
            ProbeLevel::Warn,
            "Something is listening, but the browser blocked the response (CORS)".to_string(),
            Some(
                "The connection succeeded, so the address is reachable — whatever \
                 answered did not allow this page to read it."
                    .to_string(),
            ),
            Some(cors_fix_for(base_url, &facts.page_origin)),
            Vec::new(),
        ),
        ProbeOutcome::Status { status, body } => diagnose_status(finding, status, &body, base_url),
        ProbeOutcome::Unreachable { detail } => {
            let mut hints = Vec::new();
            if is_loopback_url(base_url) && facts.page_is_https {
                if facts.is_safari {
                    hints.push(
                        "Safari refuses http://localhost requests from an https page — \
                         try Chrome, or run Studio from a local http:// address."
                            .to_string(),
                    );
                } else {
                    hints.push(
                        "A browser can also block a public site from reaching your local \
                         network — check this site's permissions in the address bar."
                            .to_string(),
                    );
                }
            }
            finding(
                FindingKind::Unreachable,
                ProbeLevel::Error,
                "Nothing answered here".to_string(),
                Some(match detail.is_empty() {
                    true => "No server accepted the connection.".to_string(),
                    false => format!("No server accepted the connection ({detail})."),
                }),
                hints.pop(),
                Vec::new(),
            )
        }
        ProbeOutcome::BadBody(detail) => finding(
            FindingKind::BadBody,
            ProbeLevel::Warn,
            "Something answered, but not an OpenAI-compatible model list".to_string(),
            Some(detail),
            Some(
                "Check the base URL — these servers serve the OpenAI routes under \
                 a prefix such as /v1."
                    .to_string(),
            ),
            Vec::new(),
        ),
    }
}

/// Non-2xx statuses, which are the informative failures: the server exists,
/// it just said no.
fn diagnose_status(
    finding: impl Fn(
        FindingKind,
        ProbeLevel,
        String,
        Option<String>,
        Option<String>,
        Vec<String>,
    ) -> ProbeFinding,
    status: u16,
    body: &str,
    base_url: &str,
) -> ProbeFinding {
    let excerpt = body_excerpt(body);
    match status {
        401 | 403 => finding(
            FindingKind::HttpStatus,
            ProbeLevel::Warn,
            format!("The server answered {status} — it wants an API key"),
            excerpt,
            Some("Paste the key the server was started with in the API key field.".to_string()),
            Vec::new(),
        ),
        404 => finding(
            FindingKind::HttpStatus,
            ProbeLevel::Warn,
            "Reachable, but there is no model list at this path (404)".to_string(),
            Some(format!("Nothing is served at {}.", models_url(base_url))),
            Some(
                "Most servers expect the base URL to end in /v1 \
                 (e.g. http://localhost:11434/v1)."
                    .to_string(),
            ),
            Vec::new(),
        ),
        status if status >= 500 => finding(
            FindingKind::HttpStatus,
            ProbeLevel::Warn,
            format!("The server answered {status} — it is up but erroring"),
            excerpt,
            Some("Check the server's own log for the failure.".to_string()),
            Vec::new(),
        ),
        status => finding(
            FindingKind::HttpStatus,
            ProbeLevel::Warn,
            format!("The server answered {status}"),
            excerpt,
            None,
            Vec::new(),
        ),
    }
}

/// The summary over a scan's findings.
pub fn scan_summary(findings: &[ProbeFinding], facts: &BrowserFacts) -> ProbeSummary {
    let usable: Vec<&ProbeFinding> = findings.iter().filter(|f| f.is_usable()).collect();
    if let Some(first) = usable.first() {
        let name = first
            .server_label
            .clone()
            .unwrap_or_else(|| first.base_url.clone());
        return ProbeSummary {
            level: ProbeLevel::Ok,
            headline: match usable.len() {
                1 => format!("Found {name}"),
                more => format!("Found {name} and {} more", more - 1),
            },
            detail: Some("Pick a server and model below to use it.".to_string()),
        };
    }
    // Nothing runnable. Lead with the closest miss, in the order that gets a
    // user to a working setup fastest: a server that only needs a model, then
    // one that only needs to allow this page, then anything else that spoke.
    if let Some(empty) = findings
        .iter()
        .find(|f| matches!(f.kind, FindingKind::NoModels | FindingKind::WrongModel))
    {
        return ProbeSummary {
            level: ProbeLevel::Warn,
            headline: empty.headline.clone(),
            detail: empty.detail.clone(),
        };
    }
    if let Some(blocked) = findings.iter().find(|f| f.kind == FindingKind::Cors) {
        return ProbeSummary {
            level: ProbeLevel::Warn,
            headline: format!(
                "Something answered at {}, but this page cannot read it yet",
                blocked.base_url
            ),
            detail: Some("Usually one setting on the server — see below.".to_string()),
        };
    }
    if let Some(warned) = findings.iter().find(|f| f.level == ProbeLevel::Warn) {
        return ProbeSummary {
            level: ProbeLevel::Warn,
            headline: warned.headline.clone(),
            detail: warned.detail.clone(),
        };
    }
    ProbeSummary {
        level: ProbeLevel::Error,
        headline: format!(
            "No local server answered on {} common {}",
            findings.len(),
            if findings.len() == 1 { "port" } else { "ports" }
        ),
        detail: Some(match facts.page_is_https && facts.is_safari {
            true => "Start your server, or try Chrome — Safari blocks \
                     http://localhost from an https page."
                .to_string(),
            false => "Start the server first, then scan again — or type its \
                      address above and press Test."
                .to_string(),
        }),
    }
}

/// The model ids a `/models` body lists, in the order the server gave them.
///
/// Reads through the agent crate's discovery parser, so a diagnosis and the
/// settings dropdown can never disagree about what a body means — including
/// the `{"data":null}` empty listing Ollama answers with nothing pulled.
pub fn parse_models(body: &str) -> Result<Vec<String>, String> {
    lpa_agent::provider::model_discovery::parse_models_page(body)
        .map(|models| models.into_iter().map(|model| model.id).collect())
        .map_err(|error| match error {
            lpa_agent::ListModelsError::Parse { message } => {
                format!("the response was not a model list: {message}")
            }
            other => format!("{other:?}"),
        })
}

/// A short, quote-worthy slice of an error body.
fn body_excerpt(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Servers usually wrap the reason in {"error": …}; surface that when
    // it is there, and the raw text otherwise.
    let message = serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|json| {
            let error = json.get("error")?;
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| trimmed.to_string());
    Some(short(&message, 160))
}

fn short(text: &str, max: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(max) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_urls_normalize_scheme_slash_and_v1() {
        assert_eq!(
            normalize_base_url("localhost:11434").as_deref(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(
            normalize_base_url("  http://localhost:11434/  ").as_deref(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(
            normalize_base_url("http://localhost:11434/v1").as_deref(),
            Some("http://localhost:11434/v1")
        );
        // An explicit path is the user's business.
        assert_eq!(
            normalize_base_url("http://box:8080/openai/v1").as_deref(),
            Some("http://box:8080/openai/v1")
        );
        assert_eq!(normalize_base_url("   "), None);
    }

    #[test]
    fn ipv4_variant_only_rewrites_localhost() {
        assert_eq!(
            ipv4_loopback_variant("http://localhost:1234/v1").as_deref(),
            Some("http://127.0.0.1:1234/v1")
        );
        assert_eq!(ipv4_loopback_variant("http://127.0.0.1:1234/v1"), None);
        assert_eq!(ipv4_loopback_variant("http://box:1234/v1"), None);
    }

    #[test]
    fn loopback_detection_covers_the_usual_spellings() {
        assert!(is_loopback_url("http://localhost:11434/v1"));
        assert!(is_loopback_url("http://127.0.0.1:11434/v1"));
        assert!(is_loopback_url("http://[::1]:11434/v1"));
        assert!(!is_loopback_url("http://192.168.1.9:11434/v1"));
        assert!(!is_loopback_url("https://api.openai.com/v1"));
    }

    #[test]
    fn known_servers_are_recognized_through_both_loopback_spellings() {
        assert_eq!(
            known_server("http://localhost:11434/v1").map(|s| s.label),
            Some("Ollama")
        );
        // Un-normalized input still resolves.
        assert_eq!(
            known_server("localhost:1234").map(|s| s.label),
            Some("LM Studio")
        );
        assert_eq!(
            known_server("http://127.0.0.1:11434/v1").map(|s| s.label),
            Some("Ollama")
        );
        assert_eq!(known_server("http://localhost:9999/v1"), None);
    }

    #[test]
    fn cors_fix_names_the_real_origin_and_the_real_server() {
        let fix = cors_fix_for("http://localhost:11434/v1", "https://lightplayer.app");
        assert!(
            fix.contains("OLLAMA_ORIGINS=https://lightplayer.app"),
            "{fix}"
        );
        // Unknown servers get the protocol-level requirement instead.
        let fix = cors_fix_for("http://box:9999/v1", "https://lightplayer.app");
        assert!(
            fix.contains("Access-Control-Allow-Origin: https://lightplayer.app"),
            "{fix}"
        );
    }

    #[test]
    fn cors_blocked_is_reported_as_reachable_with_a_fix() {
        let finding = diagnose(
            "http://localhost:11434/v1",
            ProbeOutcome::CorsBlocked,
            &facts(),
            None,
        );
        assert_eq!(finding.level, ProbeLevel::Warn);
        assert_eq!(finding.server_label.as_deref(), Some("Ollama"));
        assert!(finding.headline.contains("CORS"), "{}", finding.headline);
        // The opaque probe proves a listener, not which product it is: the
        // headline must not claim the port's usual owner is running.
        assert!(!finding.headline.contains("Ollama"), "{}", finding.headline);
        assert!(finding.fix.expect("fix").contains("OLLAMA_ORIGINS"));
    }

    #[test]
    fn unreachable_loopback_over_https_explains_the_browser_policy() {
        let mut safari = facts();
        safari.is_safari = true;
        let finding = diagnose(
            "http://localhost:11434/v1",
            ProbeOutcome::Unreachable {
                detail: "TypeError: Load failed".to_string(),
            },
            &safari,
            None,
        );
        assert_eq!(finding.level, ProbeLevel::Error);
        assert!(finding.fix.expect("fix").contains("Safari"));

        // Chrome/Firefox get the local-network permission hint instead.
        let finding = diagnose(
            "http://localhost:11434/v1",
            ProbeOutcome::Unreachable {
                detail: String::new(),
            },
            &facts(),
            None,
        );
        assert!(finding.fix.expect("fix").contains("local network"));

        // A remote server carries no browser-policy hint at all.
        let finding = diagnose(
            "http://box.lan:11434/v1",
            ProbeOutcome::Unreachable {
                detail: String::new(),
            },
            &facts(),
            None,
        );
        assert_eq!(finding.fix, None);
    }

    #[test]
    fn a_served_model_list_is_a_success_unless_the_configured_id_is_absent() {
        let models = vec!["qwen3-coder:30b".to_string(), "llama3.2".to_string()];
        let finding = diagnose(
            "http://localhost:11434/v1",
            ProbeOutcome::Models(models.clone()),
            &facts(),
            Some("llama3.2"),
        );
        assert_eq!(finding.level, ProbeLevel::Ok);
        assert!(
            finding.headline.contains("2 models"),
            "{}",
            finding.headline
        );

        let finding = diagnose(
            "http://localhost:11434/v1",
            ProbeOutcome::Models(models),
            &facts(),
            Some("gpt-4o"),
        );
        assert_eq!(finding.level, ProbeLevel::Warn);
        assert!(finding.headline.contains("gpt-4o"), "{}", finding.headline);
        assert_eq!(finding.models.len(), 2);
    }

    #[test]
    fn an_empty_model_list_says_load_a_model() {
        let finding = diagnose(
            "http://localhost:11434/v1",
            ProbeOutcome::Models(Vec::new()),
            &facts(),
            None,
        );
        assert_eq!(finding.level, ProbeLevel::Warn);
        assert!(finding.fix.expect("fix").contains("ollama pull"));
    }

    #[test]
    fn statuses_map_to_their_own_remedies() {
        let key = diagnose(
            "http://localhost:1234/v1",
            ProbeOutcome::Status {
                status: 401,
                body: r#"{"error":{"message":"missing api key"}}"#.to_string(),
            },
            &facts(),
            None,
        );
        assert!(key.headline.contains("API key"), "{}", key.headline);
        assert_eq!(key.detail.as_deref(), Some("missing api key"));

        let missing_path = diagnose(
            "http://localhost:11434",
            ProbeOutcome::Status {
                status: 404,
                body: "404 page not found".to_string(),
            },
            &facts(),
            None,
        );
        assert!(missing_path.fix.expect("fix").contains("/v1"));
        assert_eq!(
            missing_path.detail.as_deref(),
            Some("Nothing is served at http://localhost:11434/models.")
        );

        let down = diagnose(
            "http://localhost:8000/v1",
            ProbeOutcome::Status {
                status: 503,
                body: String::new(),
            },
            &facts(),
            None,
        );
        assert!(down.headline.contains("503"), "{}", down.headline);
        assert_eq!(down.detail, None);
    }

    #[test]
    fn summary_leads_with_a_usable_server_then_a_blocked_one() {
        let found = scan_summary(
            &[
                diagnose(
                    "http://localhost:1234/v1",
                    ProbeOutcome::Unreachable {
                        detail: String::new(),
                    },
                    &facts(),
                    None,
                ),
                diagnose(
                    "http://localhost:11434/v1",
                    ProbeOutcome::Models(vec!["llama3.2".to_string()]),
                    &facts(),
                    None,
                ),
            ],
            &facts(),
        );
        assert_eq!(found.level, ProbeLevel::Ok);
        assert_eq!(found.headline, "Found Ollama");

        let blocked = scan_summary(
            &[diagnose(
                "http://localhost:11434/v1",
                ProbeOutcome::CorsBlocked,
                &facts(),
                None,
            )],
            &facts(),
        );
        assert_eq!(blocked.level, ProbeLevel::Warn);
        assert!(
            blocked
                .headline
                .contains("Something answered at http://localhost:11434/v1"),
            "{blocked:?}"
        );
    }

    #[test]
    fn a_server_with_no_models_outranks_an_unreadable_one_in_the_summary() {
        // The real shape of "Ollama is running but nothing is pulled": a
        // readable answer, so it must not be summarized as a CORS problem.
        let summary = scan_summary(
            &[
                diagnose(
                    "http://localhost:1234/v1",
                    ProbeOutcome::CorsBlocked,
                    &facts(),
                    None,
                ),
                diagnose(
                    "http://localhost:11434/v1",
                    ProbeOutcome::Models(Vec::new()),
                    &facts(),
                    None,
                ),
            ],
            &facts(),
        );
        assert_eq!(summary.level, ProbeLevel::Warn);
        assert!(summary.headline.contains("serves no models"), "{summary:?}");
    }

    #[test]
    fn an_all_quiet_scan_says_so_and_names_the_safari_case() {
        let quiet: Vec<ProbeFinding> = COMMON_LOCAL_SERVERS
            .iter()
            .map(|server| {
                diagnose(
                    server.base_url,
                    ProbeOutcome::Unreachable {
                        detail: String::new(),
                    },
                    &facts(),
                    None,
                )
            })
            .collect();
        let summary = scan_summary(&quiet, &facts());
        assert_eq!(summary.level, ProbeLevel::Error);
        assert!(
            summary
                .headline
                .contains(&format!("{} common ports", COMMON_LOCAL_SERVERS.len())),
            "{summary:?}"
        );
        assert!(summary.detail.expect("detail").contains("Start the server"));

        let mut safari = facts();
        safari.is_safari = true;
        let summary = scan_summary(&quiet, &safari);
        assert!(summary.detail.expect("detail").contains("Safari"));
    }

    #[test]
    fn model_lists_parse_and_report_their_own_shape_failures() {
        let models = parse_models(r#"{"object":"list","data":[{"id":"a"},{"id":"b"}]}"#).unwrap();
        assert_eq!(models, vec!["a".to_string(), "b".to_string()]);
        // Ollama's native /api/tags shape is not the OpenAI one.
        // Ollama serving nothing: a list object with a null `data`.
        assert_eq!(
            parse_models(r#"{"object":"list","data":null}"#).unwrap(),
            Vec::<String>::new()
        );
        // Ollama's native /api/tags shape is not the OpenAI one — a body
        // with no `data` at all means the base URL is missing its /v1.
        let error = parse_models(r#"{"models":[{"name":"llama3.2"}]}"#).unwrap_err();
        assert!(error.contains("data"), "{error}");
        let error = parse_models("<html>404</html>").unwrap_err();
        assert!(error.contains("not a model list"), "{error}");
    }

    #[test]
    fn every_candidate_is_a_known_v1_url_with_an_origin_slot() {
        for server in COMMON_LOCAL_SERVERS {
            assert_eq!(
                normalize_base_url(server.base_url).as_deref(),
                Some(server.base_url),
                "{} base URL is not already normalized",
                server.label
            );
            assert!(is_loopback_url(server.base_url), "{}", server.label);
            let fix = cors_fix_for(server.base_url, "https://lightplayer.app");
            assert!(!fix.contains("{origin}"), "{}: {fix}", server.label);
        }
    }

    fn facts() -> BrowserFacts {
        BrowserFacts {
            page_origin: "https://lightplayer.app".to_string(),
            page_is_https: true,
            is_safari: false,
        }
    }
}
