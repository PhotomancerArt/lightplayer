//! http → https, in the app, for every path but the relay's device leg.
//!
//! fly.io terminates TLS and used to redirect plain HTTP itself
//! (`force_https = true` in `infra/fly.toml`). A ESP32-C6 cannot afford TLS
//! (+79 KB flash, ~29 KB heap a connection, measured 2026-10-01), so its one
//! socket to the relay, `ws://lightplayer.app/relay/device`, has to reach us
//! over plain HTTP — and fly's switch is all-or-nothing. So it is off, and
//! this middleware does what it did, for everything else:
//!
//! - A request fly's proxy received over plain HTTP carries
//!   `X-Forwarded-Proto: http`. It is answered `301 Moved Permanently` to
//!   `https://<host><path>?<query>`, exactly what fly answered before
//!   (checked against the live service on 2026-10-06: a 301, the same
//!   `Location`, no HSTS header — so none is added here either).
//! - [`RELAY_DEVICE_PATH`] passes over plain HTTP: the device leg is
//!   sealed end to end by lp-link, and the board proves its accounts
//!   in-band.
//! - A request with no `X-Forwarded-Proto` passes: it did not come through
//!   fly's proxy (fly's own health checks reach the machine directly; local
//!   runs and tests have no proxy at all).
//!
//! Decision record: `docs/adr/2026-10-06-cloud-relay.md` (the plain-HTTP
//! exception).

use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use lpc_relay::RELAY_DEVICE_PATH;

/// The header fly's proxy names the client's scheme in.
pub const X_FORWARDED_PROTO: &str = "x-forwarded-proto";

/// Axum middleware: see the module doc.
pub async fn https_redirect(request: Request, next: Next) -> Response {
    let came_in_plain = request
        .headers()
        .get(X_FORWARDED_PROTO)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|proto| proto.trim().eq_ignore_ascii_case("http"));
    if !came_in_plain || request.uri().path() == RELAY_DEVICE_PATH {
        return next.run(request).await;
    }
    let Some(host) = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return (StatusCode::BAD_REQUEST, "no Host header\n").into_response();
    };
    let path_and_query = request
        .uri()
        .path_and_query()
        .map_or("/", |path_and_query| path_and_query.as_str());
    match HeaderValue::from_str(&format!("https://{host}{path_and_query}")) {
        Ok(location) => (
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, location)],
        )
            .into_response(),
        Err(_) => (StatusCode::BAD_REQUEST, "unusable Host header\n").into_response(),
    }
}
