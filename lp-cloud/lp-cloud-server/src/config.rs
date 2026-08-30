//! Everything the process is told by its environment.
//!
//! One file, one struct, one parser: [`ServerConfig::from_vars`] takes a
//! lookup function rather than reading `std::env` itself, so every rule in
//! here — including the dev-auth localhost gate — is testable without a
//! process-global mutation that races other tests.
//!
//! | Variable | Meaning | Default |
//! |---|---|---|
//! | `LP_CLOUD_BIND` | Address to listen on | `127.0.0.1` |
//! | `LP_CLOUD_PORT` | Port to listen on | `2812` |
//! | `LP_CLOUD_STORE` | `mem` \| `sqlite` | `mem` |
//! | `LP_CLOUD_DATA_DIR` | SQLite file + fs blob root | `target/cloud-data` |
//! | `LP_CLOUD_BLOBS` | `mem` \| `fs` \| `s3` | `fs` |
//! | `LP_CLOUD_S3_BUCKET` / `_ENDPOINT` / `_REGION` | S3 target | — |
//! | `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | S3 creds | from env |
//! | `LP_CLOUD_STATIC_DIR` | `just studio-web-deploy-dir` artifact | none |
//! | `LP_CLOUD_BASE_URL` | Canonical origin (OG urls, cookie `Secure`) | `http://127.0.0.1:{port}` |
//! | `LP_CLOUD_DEV_AUTH` | `1` enables `GET /auth/dev` | off |
//! | `LP_CLOUD_GOOGLE_CLIENT_ID` / `_SECRET` | OAuth; both required for `GET /auth/google` | — |
//! | `LP_CLOUD_GOOGLE_ENDPOINT_BASE` | Point the OAuth dance at a stub (tests) | Google |

use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;

use lp_cloud_domain::{DevPickerConnection, LoginProviders, OidcConnection};

/// How long a minted session lasts. Long enough that a browser tab left open
/// over a weekend still works; short enough to be a value rather than
/// "forever".
pub const SESSION_TTL_SECONDS: f64 = 30.0 * 24.0 * 60.0 * 60.0;

/// How long a GUEST session lasts: one year. Guest ownership is
/// browser-held (examples vision D8) — the cookie IS the identity and
/// there is no login to come back through — so expiry means losing the
/// projects, and the ttl errs long. Guest-owned rows are DB-marked for
/// pruning, which is the real lever against build-up.
pub const GUEST_SESSION_TTL_SECONDS: f64 = 365.0 * 24.0 * 60.0 * 60.0;

/// The whole of the process's configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address to bind.
    pub bind: IpAddr,
    /// Port to bind. Never pinned in a recipe — `scripts/dev-port.sh` picks
    /// it for local runs (AGENTS.md "Dev server ports").
    pub port: u16,
    /// Which [`MetaStore`](lp_cloud_domain::MetaStore) adapter to open.
    pub meta: MetaBackend,
    /// Which [`BlobStore`](lp_cloud_domain::BlobStore) adapter to open.
    pub blobs: BlobBackend,
    /// Where the SQLite file and the filesystem blob root live.
    pub data_dir: PathBuf,
    /// The S3 target, when `blobs` is [`BlobBackend::S3`].
    pub s3: S3Settings,
    /// The built Studio artifact to serve. `None` serves a built-in
    /// placeholder page, which is what `just cloud-serve` does: the edge is
    /// runnable without a 10-minute web build.
    pub static_dir: Option<PathBuf>,
    /// The canonical origin, with no trailing slash. OG urls are absolute
    /// against it, and an `https` one is what makes the session cookie
    /// `Secure`.
    pub base_url: String,
    /// Whether `GET /auth/dev` exists. See [`dev_auth_allowed`].
    pub dev_auth: bool,
    /// The git sha this image was built from (`LP_CLOUD_BUILD_SHA`, set by
    /// the Dockerfile's build arg). `None` in local/dev runs. Reported by
    /// `/healthz` so "what version is deployed" is one curl, and the
    /// cutover smoke can assert deployed == pushed.
    pub build_sha: Option<String>,
    /// What it takes to sign somebody in with Google.
    pub google: GoogleSettings,
    /// How long a minted session lasts, in seconds.
    pub session_ttl_seconds: f64,
    /// How long a minted GUEST session lasts, in seconds (much longer —
    /// see [`GUEST_SESSION_TTL_SECONDS`]).
    pub guest_session_ttl_seconds: f64,
}

impl ServerConfig {
    /// Read the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// Read configuration from an arbitrary lookup — the real parser;
    /// [`from_env`](Self::from_env) is the one-line wrapper that hands it
    /// `std::env`.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let bind: IpAddr = parse_var(&get, "LP_CLOUD_BIND", "127.0.0.1")?;
        let port: u16 = parse_var(&get, "LP_CLOUD_PORT", "2812")?;
        let meta = MetaBackend::parse(&value(&get, "LP_CLOUD_STORE", "mem"))?;
        let blobs = BlobBackend::parse(&value(&get, "LP_CLOUD_BLOBS", "fs"))?;
        let data_dir = PathBuf::from(value(&get, "LP_CLOUD_DATA_DIR", "target/cloud-data"));
        let base_url = normalize_base_url(&value(
            &get,
            "LP_CLOUD_BASE_URL",
            &format!("http://127.0.0.1:{port}"),
        ));

        if blobs == BlobBackend::S3 && get("LP_CLOUD_S3_BUCKET").is_none() {
            return Err(ConfigError::Missing("LP_CLOUD_S3_BUCKET"));
        }

        // Belt and suspenders (Q23): the flag alone is not enough. A
        // deployment that inherits `LP_CLOUD_DEV_AUTH=1` from a copied
        // secrets file must not thereby grow a password-free login, so the
        // base URL has to be a localhost one as well.
        let requested_dev_auth = truthy(&value(&get, "LP_CLOUD_DEV_AUTH", "0"));
        let dev_auth = requested_dev_auth && dev_auth_allowed(&base_url);
        if requested_dev_auth && !dev_auth {
            log::warn!(
                "LP_CLOUD_DEV_AUTH is set but the base URL {base_url} is not localhost — dev auth stays OFF"
            );
        }

        Ok(Self {
            bind,
            port,
            meta,
            blobs,
            data_dir,
            s3: S3Settings {
                bucket: get("LP_CLOUD_S3_BUCKET"),
                endpoint: get("LP_CLOUD_S3_ENDPOINT"),
                region: get("LP_CLOUD_S3_REGION"),
                access_key_id: get("AWS_ACCESS_KEY_ID"),
                secret_access_key: get("AWS_SECRET_ACCESS_KEY"),
                allow_http: truthy(&value(&get, "LP_CLOUD_S3_ALLOW_HTTP", "0")),
            },
            // An empty value means unset, not "the current directory": a
            // shell recipe forwarding `${LP_CLOUD_STATIC_DIR:-}` passes an
            // empty string, and serving the repo root would be worse than
            // serving the placeholder.
            static_dir: get("LP_CLOUD_STATIC_DIR")
                .filter(|raw| !raw.trim().is_empty())
                .map(PathBuf::from),
            base_url,
            dev_auth,
            build_sha: nonempty(get("LP_CLOUD_BUILD_SHA")),
            google: GoogleSettings {
                client_id: nonempty(get("LP_CLOUD_GOOGLE_CLIENT_ID")),
                client_secret: nonempty(get("LP_CLOUD_GOOGLE_CLIENT_SECRET")),
                endpoints: match nonempty(get("LP_CLOUD_GOOGLE_ENDPOINT_BASE")) {
                    Some(base) => GoogleEndpoints::under(&base),
                    None => GoogleEndpoints::google(),
                },
            },
            session_ttl_seconds: SESSION_TTL_SECONDS,
            guest_session_ttl_seconds: GUEST_SESSION_TTL_SECONDS,
        })
    }

    /// An absolute URL for a path on this origin (`/b/<hash>`, `/p/<share>`).
    pub fn absolute(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Whether the session cookie may carry `Secure` — an `https` origin.
    /// Setting it on a plain-HTTP dev origin would make the browser drop the
    /// cookie, which looks exactly like a broken login.
    pub fn cookies_are_secure(&self) -> bool {
        self.base_url.starts_with("https://")
    }

    /// The sign-in connections this deployment reports to `LoginOptions`
    /// (P3): a `"google"` entry when both OAuth halves are configured
    /// ([`GoogleSettings::credentials`]), the dev picker only when
    /// [`dev_auth`](Self) — the flag *and* the localhost gate
    /// ([`dev_auth_allowed`]) — is on. Neither implies the other: a deployed
    /// server with real Google credentials never grows a dev picker just
    /// because someone set the flag, and a bare local run with no OAuth
    /// client id still gets one. The dev picker's live choices are not
    /// configuration — `CloudService::login_options` reads them from the
    /// store at answer time.
    pub fn login_providers(&self) -> LoginProviders {
        let oidc = if self.google.credentials().is_some() {
            vec![OidcConnection {
                id: "google".to_string(),
                label: "Google".to_string(),
                start_path: "/auth/google".to_string(),
            }]
        } else {
            Vec::new()
        };
        let dev_picker = self.dev_auth.then(|| DevPickerConnection {
            start_path: "/auth/dev".to_string(),
        });
        LoginProviders { oidc, dev_picker }
    }
}

/// What the Google login needs: who we are to Google, and where Google is.
#[derive(Debug, Clone, Default)]
pub struct GoogleSettings {
    /// OAuth client id. Public by nature — it travels in the authorize URL.
    pub client_id: Option<String>,
    /// OAuth client secret. **Never logged**, and never leaves the token
    /// exchange's request body.
    pub client_secret: Option<String>,
    /// Where the three OAuth endpoints live.
    pub endpoints: GoogleEndpoints,
}

impl GoogleSettings {
    /// The credential pair, present only when *both* halves are configured.
    ///
    /// Half-configured is the same as unconfigured on purpose: a client id
    /// with no secret produces a redirect to Google that can only fail at the
    /// callback, minutes later and on somebody else's screen.
    pub fn credentials(&self) -> Option<(&str, &str)> {
        Some((self.client_id.as_deref()?, self.client_secret.as_deref()?))
    }
}

/// The three URLs of the authorization-code flow.
///
/// These are configuration rather than constants for exactly one reason: the
/// edge tests run the whole flow against an in-process stub server, and a
/// test that cannot reach the network is worth more than one that must
/// (`tests/google_auth.rs`). Production never sets the override, so the
/// defaults below are what ships.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleEndpoints {
    /// Where the browser is sent to consent.
    pub authorize: String,
    /// Where the authorization code is exchanged for an access token.
    pub token: String,
    /// Where the access token is spent, for `{ sub, email, … }`.
    pub userinfo: String,
}

impl GoogleEndpoints {
    /// The real Google. Three different hosts, which is why this is a struct
    /// of URLs and not one base with paths hung off it.
    pub fn google() -> Self {
        Self {
            authorize: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            userinfo: "https://openidconnect.googleapis.com/v1/userinfo".into(),
        }
    }

    /// A stub server carrying all three paths — `LP_CLOUD_GOOGLE_ENDPOINT_BASE`.
    /// The paths mirror Google's so a stub reads like the thing it stands in
    /// for.
    pub fn under(base: &str) -> Self {
        let base = base.trim().trim_end_matches('/');
        Self {
            authorize: format!("{base}/o/oauth2/v2/auth"),
            token: format!("{base}/token"),
            userinfo: format!("{base}/v1/userinfo"),
        }
    }
}

impl Default for GoogleEndpoints {
    fn default() -> Self {
        Self::google()
    }
}

/// Which state backend to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaBackend {
    /// `lp-cloud-store-mem` — local dev and tests; nothing survives a
    /// restart.
    Mem,
    /// `lp-cloud-store-sqlite` — a file under `LP_CLOUD_DATA_DIR`.
    Sqlite,
}

impl MetaBackend {
    fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw {
            "mem" => Ok(Self::Mem),
            "sqlite" => Ok(Self::Sqlite),
            _ => Err(ConfigError::Invalid {
                name: "LP_CLOUD_STORE",
                expected: "mem | sqlite",
            }),
        }
    }
}

/// Which blob backend to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobBackend {
    /// Bytes in a map. Tests and throwaway dev runs only.
    Mem,
    /// Content-addressed files under `LP_CLOUD_DATA_DIR/blobs`.
    Fs,
    /// An S3-compatible bucket (Tigris).
    S3,
}

impl BlobBackend {
    fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw {
            "mem" => Ok(Self::Mem),
            "fs" => Ok(Self::Fs),
            "s3" => Ok(Self::S3),
            _ => Err(ConfigError::Invalid {
                name: "LP_CLOUD_BLOBS",
                expected: "mem | fs | s3",
            }),
        }
    }
}

/// What it takes to reach a bucket. Mirrors
/// [`S3Config`](lp_cloud_store_sqlite::S3Config); anything left `None` falls
/// back to the standard AWS environment variables.
#[derive(Debug, Clone, Default)]
pub struct S3Settings {
    /// Bucket name.
    pub bucket: Option<String>,
    /// Endpoint URL, for S3-compatible providers such as Tigris.
    pub endpoint: Option<String>,
    /// Region (Tigris accepts `auto`).
    pub region: Option<String>,
    /// Access key id.
    pub access_key_id: Option<String>,
    /// Secret access key. Never logged.
    pub secret_access_key: Option<String>,
    /// Allow a plain-HTTP endpoint (local MinIO only).
    pub allow_http: bool,
}

/// Why the environment could not be read as a configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// A variable this configuration needs is absent.
    Missing(&'static str),
    /// A variable is present but unreadable.
    Invalid {
        /// The variable's name.
        name: &'static str,
        /// What would have been accepted.
        expected: &'static str,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(name) => write!(f, "{name} is required but not set"),
            ConfigError::Invalid { name, expected } => {
                write!(f, "{name} must be one of: {expected}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Whether dev auth may be enabled for this origin.
///
/// Only a loopback origin qualifies. This is a second lock on the same door
/// as `LP_CLOUD_DEV_AUTH`: the flag says "I want a password-free login", and
/// this says "…and you are on a machine where that cannot hurt anyone".
pub fn dev_auth_allowed(base_url: &str) -> bool {
    let host = base_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url)
        .split('/')
        .next()
        .unwrap_or_default();
    let host = host.rsplit_once(':').map(|(head, _)| head).unwrap_or(host);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

fn value(get: &impl Fn(&str) -> Option<String>, name: &str, default: &str) -> String {
    get(name)
        .filter(|raw| !raw.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn parse_var<T: std::str::FromStr>(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    default: &str,
) -> Result<T, ConfigError> {
    value(get, name, default)
        .parse()
        .map_err(|_| ConfigError::Invalid {
            name,
            expected: std::any::type_name::<T>(),
        })
}

/// An empty or whitespace-only variable means "not set". A shell recipe
/// forwarding `${LP_CLOUD_GOOGLE_CLIENT_ID:-}` passes an empty string, and an
/// empty client id would otherwise look configured.
fn nonempty(raw: Option<String>) -> Option<String> {
    raw.filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
}

fn truthy(raw: &str) -> bool {
    matches!(raw.trim(), "1" | "true" | "yes" | "on")
}

fn normalize_base_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_a_runnable_local_service() {
        let config = from(&[]);
        assert_eq!(config.meta, MetaBackend::Mem);
        assert_eq!(config.blobs, BlobBackend::Fs);
        assert_eq!(config.base_url, "http://127.0.0.1:2812");
        assert!(!config.dev_auth);
        assert!(!config.cookies_are_secure());
    }

    #[test]
    fn a_trailing_slash_never_reaches_a_url_we_build() {
        let config = from(&[("LP_CLOUD_BASE_URL", "https://lightplayer.app/")]);
        assert_eq!(config.absolute("/p/x"), "https://lightplayer.app/p/x");
        assert!(config.cookies_are_secure());
    }

    /// Q23: the flag is necessary and not sufficient.
    #[test]
    fn dev_auth_needs_the_flag_and_a_localhost_origin() {
        assert!(!from(&[("LP_CLOUD_BASE_URL", "http://localhost:9000")]).dev_auth);
        assert!(
            from(&[
                ("LP_CLOUD_DEV_AUTH", "1"),
                ("LP_CLOUD_BASE_URL", "http://localhost:9000"),
            ])
            .dev_auth
        );
        assert!(
            !from(&[
                ("LP_CLOUD_DEV_AUTH", "1"),
                ("LP_CLOUD_BASE_URL", "https://lightplayer.app"),
            ])
            .dev_auth
        );
    }

    #[test]
    fn loopback_hosts_are_recognized_with_and_without_a_port() {
        assert!(dev_auth_allowed("http://127.0.0.1:23411"));
        assert!(dev_auth_allowed("http://localhost"));
        assert!(dev_auth_allowed("http://[::1]:8080"));
        assert!(!dev_auth_allowed("https://lightplayer.app"));
        assert!(!dev_auth_allowed("https://localhost.evil.example"));
    }

    /// Half a credential is not a credential: a client id with no secret can
    /// only fail at the callback, which is the worst place to learn it.
    #[test]
    fn google_needs_both_halves_of_the_credential() {
        assert_eq!(from(&[]).google.credentials(), None);
        assert_eq!(
            from(&[("LP_CLOUD_GOOGLE_CLIENT_ID", "id.apps.googleusercontent.com")])
                .google
                .credentials(),
            None
        );
        assert_eq!(
            from(&[
                ("LP_CLOUD_GOOGLE_CLIENT_ID", "id.apps.googleusercontent.com"),
                ("LP_CLOUD_GOOGLE_CLIENT_SECRET", "shh"),
            ])
            .google
            .credentials(),
            Some(("id.apps.googleusercontent.com", "shh"))
        );
    }

    /// The four permutations `LoginOptions` has to answer truthfully: neither
    /// connection configured, each alone, and both together. Google and the
    /// dev picker gate independently — one being on never implies the other.
    #[test]
    fn login_providers_reflects_every_config_permutation() {
        let neither = from(&[]).login_providers();
        assert!(neither.oidc.is_empty());
        assert!(neither.dev_picker.is_none());

        let google_only = from(&[
            ("LP_CLOUD_GOOGLE_CLIENT_ID", "id.apps.googleusercontent.com"),
            ("LP_CLOUD_GOOGLE_CLIENT_SECRET", "shh"),
        ])
        .login_providers();
        assert_eq!(google_only.oidc.len(), 1);
        assert_eq!(google_only.oidc[0].id, "google");
        assert_eq!(google_only.oidc[0].label, "Google");
        assert_eq!(google_only.oidc[0].start_path, "/auth/google");
        assert!(google_only.dev_picker.is_none());

        let dev_only = from(&[
            ("LP_CLOUD_DEV_AUTH", "1"),
            ("LP_CLOUD_BASE_URL", "http://localhost:9000"),
        ])
        .login_providers();
        assert!(dev_only.oidc.is_empty());
        assert_eq!(
            dev_only.dev_picker.expect("dev picker on").start_path,
            "/auth/dev"
        );

        let both = from(&[
            ("LP_CLOUD_GOOGLE_CLIENT_ID", "id.apps.googleusercontent.com"),
            ("LP_CLOUD_GOOGLE_CLIENT_SECRET", "shh"),
            ("LP_CLOUD_DEV_AUTH", "1"),
            ("LP_CLOUD_BASE_URL", "http://localhost:9000"),
        ])
        .login_providers();
        assert_eq!(both.oidc.len(), 1);
        assert!(both.dev_picker.is_some());

        // Half a Google credential is the same as none — the dev picker
        // still answers on its own gate.
        let half_google_plus_dev = from(&[
            ("LP_CLOUD_GOOGLE_CLIENT_ID", "id.apps.googleusercontent.com"),
            ("LP_CLOUD_DEV_AUTH", "1"),
            ("LP_CLOUD_BASE_URL", "http://localhost:9000"),
        ])
        .login_providers();
        assert!(half_google_plus_dev.oidc.is_empty());
        assert!(half_google_plus_dev.dev_picker.is_some());

        // The flag with no localhost origin is off (Q23's second lock),
        // which `login_providers` must inherit rather than re-deciding.
        let dev_flag_but_not_localhost = from(&[
            ("LP_CLOUD_DEV_AUTH", "1"),
            ("LP_CLOUD_BASE_URL", "https://lightplayer.app"),
        ])
        .login_providers();
        assert!(dev_flag_but_not_localhost.dev_picker.is_none());
    }

    /// A recipe forwarding `${VAR:-}` passes an empty string, which must not
    /// read as "configured with the empty client id".
    #[test]
    fn an_empty_credential_variable_is_unset() {
        let config = from(&[
            ("LP_CLOUD_GOOGLE_CLIENT_ID", "  "),
            ("LP_CLOUD_GOOGLE_CLIENT_SECRET", "shh"),
        ]);
        assert_eq!(config.google.credentials(), None);
    }

    #[test]
    fn the_endpoints_are_googles_unless_a_stub_base_says_otherwise() {
        assert_eq!(from(&[]).google.endpoints, GoogleEndpoints::google());
        assert!(
            from(&[])
                .google
                .endpoints
                .authorize
                .starts_with("https://accounts.google.com/")
        );

        let stubbed = from(&[("LP_CLOUD_GOOGLE_ENDPOINT_BASE", "http://127.0.0.1:9/")]);
        assert_eq!(stubbed.google.endpoints.token, "http://127.0.0.1:9/token");
        assert_eq!(
            stubbed.google.endpoints.userinfo,
            "http://127.0.0.1:9/v1/userinfo"
        );
    }

    #[test]
    fn an_unknown_backend_is_refused_by_name() {
        let error = ServerConfig::from_vars(|name| {
            (name == "LP_CLOUD_STORE").then(|| "postgres".to_string())
        })
        .unwrap_err();
        assert_eq!(
            error,
            ConfigError::Invalid {
                name: "LP_CLOUD_STORE",
                expected: "mem | sqlite",
            }
        );
    }

    /// An S3 deployment with no bucket would only fail on the first upload,
    /// which is hours after the deploy that broke it.
    #[test]
    fn s3_without_a_bucket_fails_at_startup() {
        let error =
            ServerConfig::from_vars(|name| (name == "LP_CLOUD_BLOBS").then(|| "s3".to_string()))
                .unwrap_err();
        assert_eq!(error, ConfigError::Missing("LP_CLOUD_S3_BUCKET"));
    }

    fn from(vars: &[(&str, &str)]) -> ServerConfig {
        let owned: Vec<(String, String)> = vars
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect();
        ServerConfig::from_vars(move |name| {
            owned
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        })
        .expect("test configuration parses")
    }
}
