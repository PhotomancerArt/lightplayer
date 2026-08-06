//! `POST /api` — the whole [`CloudRequest`](lpc_cloud_api::CloudRequest)
//! vocabulary through one door.
//!
//! Three things happen here and nothing else: the envelope is decoded, its
//! version is checked, and the session cookie becomes an
//! [`Actor`](lpc_cloud_api::Actor). Every rule about *what the answer is* —
//! visibility, membership, push validation — belongs to
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
use lpc_cloud_api::{CLOUD_API_VERSION, CloudCall, CloudReply, check_version};

use crate::app_state::AppState;
use crate::auth::session_cookie::session_token;

/// Answer one [`CloudCall`].
pub async fn post_api(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let call: CloudCall = match serde_json::from_slice(&body) {
        Ok(call) => call,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("not a CloudCall: {error}\n"),
            )
                .into_response();
        }
    };

    let token = session_token(&headers);
    let result = state
        .with_service(move |core| {
            // The version check comes first, and it is the edge's: a client
            // speaking another vocabulary is refused before its request is
            // looked at, never decoded on a best-effort basis
            // (`lpc_cloud_api::version`).
            match check_version(call.version) {
                Ok(()) => {
                    let actor = core.actor_for(token.as_deref());
                    core.service.handle(actor, call.request)
                }
                Err(mismatch) => Err(mismatch),
            }
        })
        .await;

    Json(CloudReply {
        version: CLOUD_API_VERSION,
        result,
    })
    .into_response()
}
