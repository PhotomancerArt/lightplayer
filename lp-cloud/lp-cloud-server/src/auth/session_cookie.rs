//! The `lp_session` cookie: reading one off a request, writing one onto a
//! response.
//!
//! The token in the cookie is the **raw** session token, base64url-encoded
//! because a cookie value is text. Only its hash is stored (P03/P04), so
//! this string is the one copy that exists and it never appears in a log.

use axum::http::HeaderMap;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use lp_cloud_domain::SESSION_TOKEN_LEN;

/// The cookie name. One name, one meaning, on one origin (D26).
pub const SESSION_COOKIE: &str = "lp_session";

/// Longest `User-Agent` a session row keeps (P3). Long enough for any real
/// browser string; short enough that a session capture cannot become a place
/// to stash an arbitrary payload.
const MAX_USER_AGENT_LEN: usize = 256;

/// Capture a request's `User-Agent` for the session `open_session` mints, if
/// it minted one — truncated to [`MAX_USER_AGENT_LEN`] characters.
///
/// Both auth edges (`google_auth`, `dev_auth`) call this at the same point
/// they call `open_session`, so the two logins record it identically. `None`
/// for no header, an unreadable one, or one that is empty after trimming —
/// all three are an honest "nothing sent", not an empty string sitting in
/// the sessions list.
pub fn captured_user_agent(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(axum::http::header::USER_AGENT)?.to_str().ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(MAX_USER_AGENT_LEN).collect())
}

/// Read the session token out of a request's `Cookie` header.
///
/// Returns `None` for no cookie, a cookie that is not ours, and a value that
/// is not a well-formed token — all three are "no session", which the domain
/// answers as [`Actor::Anonymous`](lpc_cloud_api::Actor::Anonymous). A
/// malformed token is deliberately *not* an error: a stale or corrupted
/// cookie must degrade to logged-out, never to a 400 the user cannot clear.
pub fn session_token(headers: &HeaderMap) -> Option<Vec<u8>> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    let value = raw
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value)?;
    let token = URL_SAFE_NO_PAD.decode(value).ok()?;
    (token.len() == SESSION_TOKEN_LEN).then_some(token)
}

/// The `Set-Cookie` value that installs a session (Q17).
///
/// `HttpOnly` so script cannot read a bearer credential; `SameSite=Lax` so
/// the cookie rides an ordinary top-level navigation to a share link but not
/// a cross-site form post; `Path=/` because all three planes are one origin.
///
/// `Secure` is `secure`'s business, not a constant: setting it on the
/// plain-HTTP dev origin would make the browser silently drop the cookie,
/// which presents as a login that does nothing.
pub fn set_session_cookie(
    token: &[u8; SESSION_TOKEN_LEN],
    secure: bool,
    ttl_seconds: f64,
) -> String {
    let value = URL_SAFE_NO_PAD.encode(token);
    let max_age = ttl_seconds.max(0.0) as u64;
    format!(
        "{SESSION_COOKIE}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{}",
        secure_suffix(secure)
    )
}

/// The `Set-Cookie` value that ends a session (logout, P08).
pub fn clear_session_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{}",
        secure_suffix(secure)
    )
}

fn secure_suffix(secure: bool) -> &'static str {
    if secure { "; Secure" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::COOKIE;

    #[test]
    fn reads_our_cookie_out_of_a_crowd() {
        let token = [7u8; SESSION_TOKEN_LEN];
        let cookie = set_session_cookie(&token, false, 60.0);
        let value = cookie.split(';').next().unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            format!("theme=dark; {value}; other=1").parse().unwrap(),
        );

        assert_eq!(session_token(&headers).as_deref(), Some(&token[..]));
    }

    /// A stale or hand-mangled cookie logs you out; it never breaks the
    /// request.
    #[test]
    fn a_malformed_cookie_is_simply_no_session() {
        for raw in ["", "lp_session=", "lp_session=not-base64!!", "other=x"] {
            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, raw.parse().unwrap());
            assert_eq!(session_token(&headers), None, "for cookie {raw:?}");
        }
        assert_eq!(session_token(&HeaderMap::new()), None);
    }

    /// A token of the wrong length cannot be a session token, and hashing it
    /// anyway would be a lookup that can only ever miss.
    #[test]
    fn a_short_token_is_rejected_before_it_is_hashed() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            format!("{SESSION_COOKIE}={}", URL_SAFE_NO_PAD.encode(b"short"))
                .parse()
                .unwrap(),
        );
        assert_eq!(session_token(&headers), None);
    }

    #[test]
    fn secure_follows_the_origin_scheme() {
        let token = [1u8; SESSION_TOKEN_LEN];
        assert!(!set_session_cookie(&token, false, 60.0).contains("Secure"));
        assert!(set_session_cookie(&token, true, 60.0).ends_with("; Secure"));
        assert!(clear_session_cookie(true).contains("Max-Age=0"));
    }

    #[test]
    fn the_header_is_captured_verbatim_when_short() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::USER_AGENT,
            "Mozilla/5.0".parse().unwrap(),
        );
        assert_eq!(
            captured_user_agent(&headers).as_deref(),
            Some("Mozilla/5.0")
        );
    }

    #[test]
    fn no_header_and_a_blank_header_are_both_none() {
        assert_eq!(captured_user_agent(&HeaderMap::new()), None);

        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::USER_AGENT, "   ".parse().unwrap());
        assert_eq!(captured_user_agent(&headers), None);
    }

    #[test]
    fn a_long_header_is_truncated_to_the_cap() {
        let mut headers = HeaderMap::new();
        let long = "x".repeat(1000);
        headers.insert(axum::http::header::USER_AGENT, long.parse().unwrap());
        let captured = captured_user_agent(&headers).expect("a header was sent");
        assert_eq!(captured.chars().count(), MAX_USER_AGENT_LEN);
    }
}
