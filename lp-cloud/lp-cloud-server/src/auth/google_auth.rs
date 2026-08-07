//! Sign in with Google: the authorization-code flow, hand-rolled (Q18).
//!
//! Three routes and one round trip:
//!
//! 1. `GET /auth/google` — 302 to Google's consent screen, carrying a random
//!    `state` that is *also* dropped into a short-lived `HttpOnly` cookie.
//! 2. `GET /auth/google/callback?code&state` — the state has to match the
//!    cookie, the code is exchanged for an access token, the access token is
//!    spent at the userinfo endpoint, and the verified email becomes a
//!    session.
//! 3. `POST /auth/logout` — the session row goes, the cookie goes.
//!
//! # Why no `oauth2` crate, and no JWT
//!
//! The code flow is four query parameters and two HTTP calls, and the
//! `id_token` Google hands back would have to be validated against a rotating
//! JWKS to be worth anything. We never do that: the access token is spent
//! immediately at `userinfo` over TLS, which *is* the validation — the answer
//! comes from Google directly rather than from a signature we would have to
//! verify ourselves.
//!
//! # What never gets logged
//!
//! The client secret, the authorization code, the access token, and the
//! session token. Failures log a status code and a reason, never a body: a
//! provider's error body is not ours to hold, and an authorization code in a
//! log file is a login.
//!
//! # Where identity comes from
//!
//! [`upsert_user`](lp_cloud_domain::CloudService::upsert_user) is keyed on the
//! Google `sub`, not on the email — a person who changes address is the same
//! account — and that call is also what resolves pending membership rows for
//! the verified email (Q4/D10). The moment a user invited by email first logs
//! in is the moment their invitation becomes access, and it happens here
//! because it happens in `upsert_user`.

use std::fmt::Write as _;
use std::sync::OnceLock;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;

use crate::app_state::AppState;
use crate::auth::session_cookie::{
    captured_user_agent, clear_session_cookie, session_token, set_session_cookie,
};

/// The cookie holding the `state` we expect back from Google.
pub const OAUTH_STATE_COOKIE: &str = "lp_oauth_state";

/// How long a half-finished sign-in stays valid. Long enough to pick an
/// account and type a password; short enough that an abandoned tab does not
/// leave a usable state lying around.
const STATE_TTL_SECONDS: u64 = 10 * 60;

/// What we ask Google for: identity and an address, nothing else. No Drive,
/// no Photos, no scope that would put this app in front of a verification
/// review it does not need.
const SCOPE: &str = "openid email profile";

/// The path Google is told to come back to. Also what has to be registered in
/// the console, verbatim — see the crate README.
pub const CALLBACK_PATH: &str = "/auth/google/callback";

/// Longest `?next=` we will carry. A path is a path; anything this long is
/// somebody's payload.
const MAX_NEXT_LEN: usize = 512;

/// `GET /auth/google?next=/somewhere` — start the dance.
#[derive(Debug, Deserialize)]
pub struct GoogleAuthQuery {
    /// Where to land after signing in. Same-origin *relative* paths only; see
    /// [`safe_next_path`]. Anything else is silently dropped rather than
    /// refused, because a bad `next` is a broken link, not a failed login.
    pub next: Option<String>,
}

/// Redirect the browser to Google's consent screen.
pub async fn get_google_auth(
    State(state): State<AppState>,
    Query(query): Query<GoogleAuthQuery>,
) -> Response {
    let config = state.config();
    let Some((client_id, _)) = config.google.credentials() else {
        return not_configured();
    };

    let next = query
        .next
        .as_deref()
        .and_then(safe_next_path)
        .unwrap_or_default();
    let oauth_state = mint_state(&next);

    let redirect_uri = config.absolute(CALLBACK_PATH);
    let mut url = String::from(&config.google.endpoints.authorize);
    url.push('?');
    append_query(
        &mut url,
        &[
            ("client_id", client_id),
            ("redirect_uri", &redirect_uri),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("state", &oauth_state),
            // Without this, a browser already signed into one Google account
            // is silently signed into the app as that account, with no way to
            // pick another. "Which account?" is a question the user gets to
            // answer.
            ("prompt", "select_account"),
        ],
    );

    redirect(
        StatusCode::FOUND,
        &url,
        &[&state_cookie(&oauth_state, config.cookies_are_secure())],
    )
}

/// `GET /auth/google/callback?code&state` — what Google sends back.
#[derive(Debug, Deserialize)]
pub struct GoogleCallbackQuery {
    /// The authorization code, on success. Never logged.
    pub code: Option<String>,
    /// The `state` we sent, echoed back.
    pub state: Option<String>,
    /// Google's own refusal (`access_denied` when the user says no).
    pub error: Option<String>,
}

/// Finish the dance: verify, exchange, identify, mint a session.
pub async fn get_google_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GoogleCallbackQuery>,
) -> Response {
    let config = state.config();
    let secure = config.cookies_are_secure();
    let Some((client_id, client_secret)) = config.google.credentials() else {
        return not_configured();
    };

    if let Some(error) = &query.error {
        // The user pressing "cancel" is not a server fault, and their own
        // browser already knows what happened.
        log::info!("google sign-in was refused by the provider: {error}");
        return refuse(
            StatusCode::BAD_REQUEST,
            "google sign-in was cancelled or refused\n",
            secure,
        );
    }

    // The cookie is the only copy of what we expect; a request that does not
    // carry it is either a stale tab or somebody else's forged callback.
    let expected = cookie_value(&headers, OAUTH_STATE_COOKIE);
    let matched = match (&expected, &query.state) {
        (Some(expected), Some(returned)) => constant_time_eq(expected, returned),
        _ => false,
    };
    if !matched {
        log::warn!("google callback with a state that does not match the cookie");
        return refuse(
            StatusCode::BAD_REQUEST,
            "this sign-in did not start here — try again\n",
            secure,
        );
    }

    let Some(code) = query.code.as_deref().filter(|code| !code.is_empty()) else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "google sent no authorization code\n",
            secure,
        );
    };

    let redirect_uri = config.absolute(CALLBACK_PATH);
    let access_token = match exchange_code(
        &config.google.endpoints.token,
        code,
        client_id,
        client_secret,
        &redirect_uri,
    )
    .await
    {
        Ok(token) => token,
        Err(reason) => {
            log::warn!("google token exchange failed: {reason}");
            return refuse(
                StatusCode::BAD_GATEWAY,
                "could not complete sign-in with google\n",
                secure,
            );
        }
    };

    let profile = match fetch_profile(&config.google.endpoints.userinfo, &access_token).await {
        Ok(profile) => profile,
        Err(reason) => {
            log::warn!("google userinfo failed: {reason}");
            return refuse(
                StatusCode::BAD_GATEWAY,
                "could not read your google profile\n",
                secure,
            );
        }
    };

    let Some(email) = profile.email.as_deref().filter(|email| !email.is_empty()) else {
        return refuse(
            StatusCode::FORBIDDEN,
            "google did not provide an email address for this account\n",
            secure,
        );
    };
    // An unverified address would let anyone claim an invitation sent to
    // somebody else's email, which is exactly the door `resolve_pending_members`
    // opens.
    if profile.email_verified != Some(true) {
        return refuse(
            StatusCode::FORBIDDEN,
            "this google account's email address is not verified\n",
            secure,
        );
    }

    let display_name = profile
        .name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| email.split('@').next().unwrap_or(email).to_string());
    // Captured for `upsert_user` — what happens with them (seeded once at
    // creation, never touched by a returning login) is that call's own
    // Q4/Q5 ruling, not this edge's.
    let given_name = profile.given_name.filter(|name| !name.trim().is_empty());
    let family_name = profile.family_name.filter(|name| !name.trim().is_empty());
    let picture = profile.picture.filter(|url| !url.trim().is_empty());
    let google_sub = profile.sub;
    let email = email.to_string();
    let ttl = config.session_ttl_seconds;
    let user_agent = captured_user_agent(&headers);

    let token = state
        .with_service(move |core| {
            // This one call mints-or-finds the account *and* resolves every
            // pending membership for the address (Q4). "google" (P2's
            // provider column) — the only connection this handler ever
            // authenticates through.
            let user = core.service.upsert_user(
                &google_sub,
                &email,
                &display_name,
                "google",
                given_name.as_deref(),
                family_name.as_deref(),
                picture.as_deref(),
            );
            core.service.open_session(user.uid, ttl, user_agent)
        })
        .await;

    // The `next` rode along inside the state we just verified, so it is ours
    // — and it is validated again anyway, because a redirect target is not
    // somewhere to be relaxed.
    let landing = query
        .state
        .as_deref()
        .and_then(next_from_state)
        .and_then(|next| safe_next_path(&next))
        .unwrap_or_else(|| "/".to_string());

    redirect(
        StatusCode::SEE_OTHER,
        &landing,
        &[
            &set_session_cookie(&token, secure, ttl),
            &clear_state_cookie(secure),
        ],
    )
}

/// `POST /auth/logout` — end the session on the server, then in the browser.
///
/// Deleting the row first is the point: clearing the cookie alone would leave
/// a live credential in whatever else has a copy of it.
pub async fn post_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        state
            .with_service(move |core| core.service.close_session(&token))
            .await;
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    append_cookie(
        &mut response,
        &clear_session_cookie(state.config().cookies_are_secure()),
    );
    response
}

// ---- the two calls to Google ------------------------------------------

/// What the token endpoint answers. Everything but the access token is
/// deliberately not modelled: an `id_token` we do not validate and a refresh
/// token we do not want are fields it is better not to hold.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// The subset of OIDC userinfo that identity is built from.
///
/// `given_name`/`family_name`/`picture` are Google's own field names for the
/// `profile` scope (already requested, `:63`) — present whenever the account
/// has them set, absent (never an error) otherwise, which `serde`'s default
/// handling for `Option<T>` covers with no `#[serde(default)]` needed.
#[derive(Debug, Deserialize)]
struct ProfileResponse {
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    given_name: Option<String>,
    family_name: Option<String>,
    picture: Option<String>,
}

async fn exchange_code(
    token_url: &str,
    code: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<String, String> {
    let response = http_client()
        .post(token_url)
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|error| format!("the request did not complete: {}", error.without_url()))?;

    // Status only. The body of a failed token exchange is the provider's to
    // describe, and it is one field away from the credential we just sent.
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the endpoint answered {status}"));
    }
    let token: TokenResponse = response
        .json()
        .await
        .map_err(|_| "the answer was not a token response".to_string())?;
    if token.access_token.is_empty() {
        return Err("the answer carried an empty access token".to_string());
    }
    Ok(token.access_token)
}

async fn fetch_profile(userinfo_url: &str, access_token: &str) -> Result<ProfileResponse, String> {
    let response = http_client()
        .get(userinfo_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| format!("the request did not complete: {}", error.without_url()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("the endpoint answered {status}"));
    }
    response
        .json()
        .await
        .map_err(|_| "the answer was not a userinfo document".to_string())
}

/// The one HTTP client, built once.
///
/// A timeout is not optional: without one, a provider that accepts the
/// connection and then says nothing holds a request handler forever.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("lp-cloud-server/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("lp-cloud-server: could not build an HTTPS client")
    })
}

// ---- state, cookies, redirects ----------------------------------------

/// A fresh `state`: 32 random bytes, plus the landing path it belongs to.
///
/// Carrying `next` *inside* the state — rather than as its own query
/// parameter on the callback — means the round trip cannot be re-pointed by
/// anyone who did not also set our cookie, since the two are compared whole.
fn mint_state(next: &str) -> String {
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).expect("lp-cloud-server: the OS refused to provide random bytes");
    format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(random),
        URL_SAFE_NO_PAD.encode(next.as_bytes())
    )
}

/// The landing path out of a state value, if it carries one.
fn next_from_state(state: &str) -> Option<String> {
    let (_, encoded) = state.split_once('.')?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let next = String::from_utf8(bytes).ok()?;
    (!next.is_empty()).then_some(next)
}

/// Whether a `?next=` may be redirected to.
///
/// Only a same-origin **relative** path qualifies, which rules out the open
/// redirect: `https://evil.example`, the scheme-relative `//evil.example`
/// (which a browser reads as an absolute URL), its backslash variants that
/// some parsers normalise, and anything carrying a control character that
/// could split the `Location` header.
pub fn safe_next_path(raw: &str) -> Option<String> {
    let path = raw.trim();
    if path.len() > MAX_NEXT_LEN {
        return None;
    }
    let mut chars = path.chars();
    if chars.next() != Some('/') {
        return None;
    }
    // `//host` and `/\host` are absolute URLs wearing a path's clothes.
    if matches!(chars.next(), Some('/') | Some('\\')) {
        return None;
    }
    if path.chars().any(|c| c <= ' ' || c == '\u{7f}' || c == '\\') {
        return None;
    }
    Some(path.to_string())
}

/// The `Set-Cookie` that carries the expected state across the round trip.
///
/// `SameSite=Lax` because the callback *is* a cross-site top-level navigation
/// — a `Strict` cookie would not be sent and every sign-in would fail the
/// state check. `Path=/auth` keeps it off every other request.
fn state_cookie(value: &str, secure: bool) -> String {
    format!(
        "{OAUTH_STATE_COOKIE}={value}; Path=/auth; Max-Age={STATE_TTL_SECONDS}; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    )
}

/// The `Set-Cookie` that retires a used or failed state. Every exit from the
/// callback clears it: a state is good for exactly one attempt.
fn clear_state_cookie(secure: bool) -> String {
    format!(
        "{OAUTH_STATE_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    )
}

/// One named cookie out of a request's `Cookie` header.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
}

/// Compare two states without an early exit on the first differing byte.
///
/// The state is not a stored secret, so this is belt-and-braces rather than a
/// load-bearing defence — but a comparison that leaks its progress is never
/// the one to write.
fn constant_time_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right) {
        diff |= a ^ b;
    }
    diff == 0
}

/// A redirect carrying however many cookies it needs.
fn redirect(status: StatusCode, location: &str, cookies: &[&str]) -> Response {
    let Ok(location) = HeaderValue::from_str(location) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not encode the redirect\n",
        )
            .into_response();
    };
    let mut response = status.into_response();
    response.headers_mut().insert(header::LOCATION, location);
    for cookie in cookies {
        append_cookie(&mut response, cookie);
    }
    response
}

/// A refusal that also retires the pending state, so the next attempt starts
/// clean rather than tripping over the last one.
fn refuse(status: StatusCode, message: &'static str, secure: bool) -> Response {
    let mut response = (status, message).into_response();
    append_cookie(&mut response, &clear_state_cookie(secure));
    response
}

fn not_configured() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "google sign-in is not configured on this server\n",
    )
        .into_response()
}

fn append_cookie(response: &mut Response, cookie: &str) {
    match HeaderValue::from_str(cookie) {
        Ok(value) => {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
        Err(_) => log::error!("a cookie value could not be encoded as a header"),
    }
}

/// Append `name=value` pairs to a URL that already ends in `?`.
fn append_query(url: &mut String, params: &[(&str, &str)]) {
    for (index, (name, value)) in params.iter().enumerate() {
        if index > 0 {
            url.push('&');
        }
        url.push_str(name);
        url.push('=');
        percent_encode_into(url, value);
    }
}

/// Percent-encode everything that is not an RFC 3986 unreserved character.
///
/// Deliberately aggressive: a query value here is a URL, a scope list with
/// spaces, and a base64 state, and over-encoding is always safe where
/// under-encoding is a broken redirect.
fn percent_encode_into(out: &mut String, value: &str) {
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_open_redirect_is_not_a_next_path() {
        for hostile in [
            "https://evil.example",
            "//evil.example",
            "/\\evil.example",
            "/\\/evil.example",
            "http://localhost:8080/",
            "evil.example",
            "",
            "/ok\r\nSet-Cookie: x=1",
            "/ok\nx",
            "/with a space",
        ] {
            assert_eq!(safe_next_path(hostile), None, "for {hostile:?}");
        }
        assert_eq!(safe_next_path(&format!("/{}", "a".repeat(600))), None);
    }

    #[test]
    fn an_ordinary_relative_path_survives() {
        assert_eq!(safe_next_path("/"), Some("/".into()));
        assert_eq!(
            safe_next_path("/p/zook-dome-prj_abc?tab=play"),
            Some("/p/zook-dome-prj_abc?tab=play".into())
        );
        assert_eq!(safe_next_path("  /projects  "), Some("/projects".into()));
    }

    /// The state has to survive the round trip with its landing path intact,
    /// and two mints must never collide.
    #[test]
    fn a_state_carries_its_landing_path_and_is_unique() {
        let state = mint_state("/p/thing");
        assert_eq!(next_from_state(&state).as_deref(), Some("/p/thing"));
        assert_eq!(next_from_state(&mint_state("")), None);
        assert_ne!(mint_state(""), mint_state(""));
        assert_eq!(next_from_state("nonsense"), None);
    }

    /// A state is cookie-safe and URL-safe by construction; if it ever were
    /// not, the sign-in would break in a browser rather than in a test.
    #[test]
    fn a_state_needs_no_escaping() {
        let state = mint_state("/p/a b?c=d&e");
        assert!(
            state
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        );
    }

    #[test]
    fn query_values_are_encoded() {
        let mut url = String::from("https://accounts.example/auth?");
        append_query(
            &mut url,
            &[
                (
                    "redirect_uri",
                    "https://lightplayer.app/auth/google/callback",
                ),
                ("scope", "openid email profile"),
            ],
        );
        assert_eq!(
            url,
            "https://accounts.example/auth?\
             redirect_uri=https%3A%2F%2Flightplayer.app%2Fauth%2Fgoogle%2Fcallback\
             &scope=openid%20email%20profile"
        );
    }

    #[test]
    fn the_state_cookie_is_scoped_and_httponly() {
        let cookie = state_cookie("abc.def", true);
        assert!(cookie.starts_with("lp_oauth_state=abc.def;"));
        assert!(cookie.contains("Path=/auth"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.ends_with("; Secure"));
        assert!(!state_cookie("abc.def", false).contains("Secure"));
        assert!(clear_state_cookie(false).contains("Max-Age=0"));
    }

    #[test]
    fn states_are_compared_whole() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("", "a"));
    }

    #[test]
    fn a_cookie_is_found_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "theme=dark; lp_oauth_state=xyz; lp_session=q"
                .parse()
                .unwrap(),
        );
        assert_eq!(
            cookie_value(&headers, OAUTH_STATE_COOKIE).as_deref(),
            Some("xyz")
        );
        assert_eq!(cookie_value(&HeaderMap::new(), OAUTH_STATE_COOKIE), None);
    }
}
