//! OpenRouter OAuth PKCE Connect flow (settings ADR: openrouter-connect).
//!
//! Zero-backend, no client secret: redirect to `openrouter.ai/auth` with an
//! S256 challenge, come back with `?code=…`, exchange it for an API key that
//! lives in the USER's OpenRouter account (revocable there, charged to their
//! credits). The pure pieces (verifier/challenge/URLs/parsing) are host-unit
//! tested; the browser glue (storage, redirect, fetch) is wasm-only.
//!
//! Redirect round-trip: [`start_connect`] stashes the verifier and the
//! current route PATH (with its query) in sessionStorage and navigates away;
//! [`take_pending_callback`] runs synchronously at boot BEFORE the router
//! parses the URL — it consumes the stash, scrubs `?code=…` from the address
//! bar, and restores the stashed path so boot routing sees the pre-redirect
//! location.
//!
//! The callback URL we hand OpenRouter is the site ROOT, never the deep
//! route we happen to be on: the return leg then needs nothing of the host
//! but the page it would serve anyway, and the deep path comes back out of
//! the stash. (Before P09 this was the same thing by accident — every route
//! lived in the fragment, and fragments do not survive a redirect.)

#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "the call sites are wasm-only glue; host builds run the unit tests"
    )
)]

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// OpenRouter's authorization page.
const AUTH_ORIGIN: &str = "https://openrouter.ai/auth";
/// The PKCE verifier: 32 random bytes, base64url — 43 chars, inside
/// RFC 7636's 43–128 window.
pub fn verifier_from_bytes(bytes: &[u8; 32]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The S256 challenge for a verifier: base64url(sha256(ascii(verifier))).
pub fn challenge_for(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// The authorization URL for one connect attempt.
pub fn auth_url(callback_url: &str, challenge: &str) -> String {
    format!(
        "{AUTH_ORIGIN}?callback_url={}&code_challenge={challenge}&code_challenge_method=S256",
        percent_encode(callback_url)
    )
}

/// The `code` query param from a `location.search` string, if present.
pub fn code_from_search(search: &str) -> Option<String> {
    search
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("code="))
        .filter(|code| !code.is_empty())
        .map(percent_decode)
}

/// RFC 3986 component encoding (unreserved chars pass through). The
/// callback URL is the only thing we encode; challenges are already
/// base64url-safe.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Minimal percent-decoding for the auth code (codes are URL-safe today;
/// this just keeps us honest if that changes).
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = value.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A `?code=…` return leg, consumed from the URL + sessionStorage at boot.
#[cfg(target_arch = "wasm32")]
pub struct PendingCallback {
    pub code: String,
    pub verifier: String,
}

#[cfg(target_arch = "wasm32")]
mod glue {
    use super::*;

    /// The code→key exchange endpoint (CORS-open).
    const EXCHANGE_URL: &str = "https://openrouter.ai/api/v1/auth/keys";
    /// sessionStorage slots for the redirect round-trip.
    const VERIFIER_SLOT: &str = "lp.openrouter.pkce_verifier";
    const RETURN_ROUTE_SLOT: &str = "lp.openrouter.return_route";

    fn session_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.session_storage().ok().flatten()
    }

    /// 32 crypto-quality bytes (same source as uid minting; PKCE verifiers
    /// need real entropy, so no Math.random fallback — `None` on the
    /// vanishingly rare crypto-less environment).
    fn random_verifier_bytes() -> Option<[u8; 32]> {
        let mut bytes = [0u8; 32];
        web_sys::window()?
            .crypto()
            .ok()?
            .get_random_values_with_u8_array(&mut bytes)
            .ok()?;
        Some(bytes)
    }

    /// Kick off the connect: stash verifier + the route we are leaving from,
    /// then head for OpenRouter's approval page. Returns an error string
    /// only when the browser refused us (no crypto/storage) — there is no
    /// async here, the page navigates away.
    pub fn start_connect() -> Result<(), String> {
        let window = web_sys::window().ok_or("no window")?;
        let location = window.location();
        let bytes = random_verifier_bytes().ok_or("browser crypto unavailable")?;
        let verifier = verifier_from_bytes(&bytes);
        let challenge = challenge_for(&verifier);
        let storage = session_storage().ok_or("sessionStorage unavailable")?;
        let return_route = format!(
            "{}{}",
            location.pathname().unwrap_or_else(|_| "/".to_string()),
            location.search().unwrap_or_default()
        );
        storage
            .set_item(VERIFIER_SLOT, &verifier)
            .map_err(|_| "sessionStorage write failed")?;
        let _ = storage.set_item(RETURN_ROUTE_SLOT, &return_route);
        let origin = location.origin().map_err(|_| "no origin")?;
        let url = auth_url(&format!("{origin}/"), &challenge);
        location.assign(&url).map_err(|_| "navigation failed")?;
        Ok(())
    }

    /// Boot-time interception: if the URL carries `?code=` AND we hold a
    /// stashed verifier, consume both, scrub the query from the address bar,
    /// and restore the stashed path — synchronously, before the router reads
    /// the URL. A code without a stash (foreign/stale redirect) is scrubbed
    /// and dropped, landing on the root.
    pub fn take_pending_callback() -> Option<PendingCallback> {
        let window = web_sys::window()?;
        let location = window.location();
        let code = code_from_search(&location.search().ok()?)?;
        let storage = session_storage();
        let verifier = storage
            .as_ref()
            .and_then(|s| s.get_item(VERIFIER_SLOT).ok().flatten());
        let return_route = storage
            .as_ref()
            .and_then(|s| s.get_item(RETURN_ROUTE_SLOT).ok().flatten())
            .filter(|route| route.starts_with('/'))
            .unwrap_or_else(|| "/".to_string());
        if let Some(storage) = &storage {
            let _ = storage.remove_item(VERIFIER_SLOT);
            let _ = storage.remove_item(RETURN_ROUTE_SLOT);
        }
        // Scrub: the restored route, `?code=` gone. The router's own
        // canonicalization runs right after and keeps this shape.
        if let Ok(history) = window.history() {
            let _ = history.replace_state_with_url(
                &wasm_bindgen::JsValue::NULL,
                "",
                Some(&return_route),
            );
        }
        let verifier = verifier?;
        Some(PendingCallback { code, verifier })
    }

    /// The code→key exchange. Returns the API key. Never logs any secret.
    pub async fn exchange(callback: PendingCallback) -> Result<String, String> {
        let body = serde_json::json!({
            "code": callback.code,
            "code_verifier": callback.verifier,
            "code_challenge_method": "S256",
        });
        let response = gloo_net::http::Request::post(EXCHANGE_URL)
            .header("content-type", "application/json")
            .body(body.to_string())
            .map_err(|e| format!("request build failed: {e}"))?
            .send()
            .await
            .map_err(|e| format!("network error: {e}"))?;
        if !response.ok() {
            return Err(format!(
                "OpenRouter rejected the exchange (HTTP {})",
                response.status()
            ));
        }
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("bad response: {e}"))?;
        json.get("key")
            .and_then(|k| k.as_str())
            .filter(|k| !k.is_empty())
            .map(str::to_string)
            .ok_or_else(|| "response carried no key".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
pub use glue::{exchange, start_connect, take_pending_callback};

/// UI entry point for the Connect buttons: kick off the redirect, routing a
/// synchronous refusal (no crypto/storage) into the transient error signal.
/// Host builds (stories, unit tests) are inert — the page can't navigate.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(unused_variables, reason = "host builds render the button inert")
)]
pub fn begin_connect(error: Option<dioxus::prelude::Signal<Option<String>>>) {
    #[cfg(target_arch = "wasm32")]
    if let Err(message) = start_connect()
        && let Some(mut error) = error
    {
        use dioxus::prelude::*;
        error.set(Some(format!("Connect failed: {message}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_matches_rfc7636_appendix_b() {
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_is_43_url_safe_chars() {
        let verifier = verifier_from_bytes(&[7u8; 32]);
        assert_eq!(verifier.len(), 43);
        assert!(
            verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn auth_url_encodes_the_callback() {
        let url = auth_url("http://localhost:8080/", "abc123");
        assert_eq!(
            url,
            "https://openrouter.ai/auth?callback_url=http%3A%2F%2Flocalhost%3A8080%2F\
             &code_challenge=abc123&code_challenge_method=S256"
        );
    }

    #[test]
    fn code_parses_from_search_variants() {
        assert_eq!(code_from_search("?code=abc"), Some("abc".to_string()));
        assert_eq!(
            code_from_search("?foo=1&code=a%2Bb"),
            Some("a+b".to_string())
        );
        assert_eq!(code_from_search("?code="), None);
        assert_eq!(code_from_search(""), None);
        assert_eq!(code_from_search("?other=x"), None);
    }
}
