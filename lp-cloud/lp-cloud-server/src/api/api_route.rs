//! `POST /api` — the whole [`CloudRequest`](lpc_cloud_api::CloudRequest)
//! vocabulary through one door.
//!
//! Three things happen here and nothing else: the envelope's version is
//! checked, the request is decoded, and the session cookie becomes an
//! [`Actor`](lpc_cloud_api::Actor). Every rule about *what the answer is* —
//! access rules, membership, push validation — belongs to
//! [`CloudService::handle`](lp_cloud_domain::CloudService::handle) and is not
//! duplicated, second-guessed, or re-tested at this layer. This route never
//! looks inside the request or the response: dispatch to a typed handler is
//! `handle`'s exhaustive match, and the reply goes back out as whatever that
//! match produced.
//!
//! # Why a refusal is still `200 OK`
//!
//! A [`CloudError`](lpc_cloud_api::CloudError) is a *typed answer*, not a
//! transport failure: the client's [`TransportError`] family means "the
//! conversation did not happen", and folding a version mismatch into a 400
//! would put a considered refusal in the same bucket as a dead socket. The
//! only non-200 answers here are a body that is not a `CloudCall` at all.
//!
//! [`TransportError`]: https://docs.rs/lpa-cloud-client

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use lp_cloud_domain::{Caller, session_token_hash};
use lpc_cloud_api::{CLOUD_API_VERSION, CloudCall, CloudReply, check_version};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::auth::session_cookie::session_token;

/// Answer one [`CloudCall`].
pub async fn post_api(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    // The version is read out of the envelope *before* the request is
    // decoded, and it has to be: an older client's request payload is
    // exactly the thing this build cannot parse, so decoding first would
    // answer a stale tab with "not a CloudCall" — a malformed-input 400 —
    // where the truth is a named `VersionMismatch` it can act on. Getting
    // this backwards makes the version handshake vanish the moment a
    // message shape changes, which is the only time it matters.
    let version = match serde_json::from_slice::<CallVersion>(&body) {
        Ok(envelope) => envelope.version,
        Err(error) => return bad_request(&error),
    };
    if let Err(mismatch) = check_version(version) {
        return Json(CloudReply {
            version: CLOUD_API_VERSION,
            result: Err(mismatch),
        })
        .into_response();
    }

    let call: CloudCall = match serde_json::from_slice(&body) {
        Ok(call) => call,
        Err(error) => return bad_request(&error),
    };

    let token = session_token(&headers);
    let result = state
        .with_service(move |core| {
            let actor = core.actor_for(token.as_deref());
            // The caller cannot report its own session id itself (the token
            // lives in an HttpOnly cookie it never reads), so
            // `ListSessions`/`RevokeSession` need it threaded through here —
            // see `lp_cloud_domain::Caller`.
            let session = token.as_deref().map(session_token_hash);
            core.service.handle(Caller { actor, session }, call.request)
        })
        .await;

    Json(CloudReply {
        version: CLOUD_API_VERSION,
        result,
    })
    .into_response()
}

/// Just the envelope's version, for the pre-decode above. Every other field
/// is ignored, so a body this build cannot fully parse still yields the one
/// number that says why.
#[derive(Deserialize)]
struct CallVersion {
    version: u32,
}

fn bad_request(error: &serde_json::Error) -> Response {
    (
        StatusCode::BAD_REQUEST,
        format!("not a CloudCall: {error}\n"),
    )
        .into_response()
}
