//! `POST /auth/guest` — a session with no account behind it (examples
//! vision D3/D8).
//!
//! An anonymous fork's publish needs an owner, and sign-in must not gate
//! saving/sharing (D3). This endpoint mints a guest [`CloudUser`] (marked
//! `anonymous` — the D8 pruning lever) and installs a LONG-ttl session
//! cookie: the cookie IS the ownership, browser-held, with
//! claim-on-sign-in parked as future work.
//!
//! Idempotent by cookie: a caller whose request already carries a live
//! session — guest or real — gets `204 No Content` and no second mint, so
//! a client may call this unconditionally before its first publish.
//!
//! POST, not GET: it mints state, and a prefetched or link-previewed GET
//! must never create accounts.

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::app_state::AppState;
use crate::auth::session_cookie::{captured_user_agent, session_token, set_session_cookie};

/// Ensure the caller has a session, minting a guest account if needed.
pub async fn post_guest_auth(state: axum::extract::State<AppState>, headers: HeaderMap) -> Response {
    let axum::extract::State(state) = state;
    let token = session_token(&headers);
    let ttl = state.config().guest_session_ttl_seconds;
    let user_agent = captured_user_agent(&headers);

    let minted = state
        .with_service(move |core| {
            if let Some(token) = token.as_deref()
                && matches!(core.actor_for(Some(token)), lpc_cloud_api::Actor::User(_))
            {
                // A live session already owns things; never shadow it.
                return None;
            }
            let user = core.service.begin_guest_user();
            Some(core.service.open_session(user.uid, ttl, user_agent))
        })
        .await;

    let Some(token) = minted else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let cookie = set_session_cookie(&token, state.config().cookies_are_secure(), ttl);
    let mut response = StatusCode::NO_CONTENT.into_response();
    match HeaderValue::from_str(&cookie) {
        Ok(value) => {
            response.headers_mut().insert(header::SET_COOKIE, value);
            response
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not encode the session cookie\n",
        )
            .into_response(),
    }
}
