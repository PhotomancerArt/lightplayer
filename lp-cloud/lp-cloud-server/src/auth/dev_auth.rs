//! `GET /auth/dev?email=…` — a login with no password, no provider, and no
//! network (Q23).
//!
//! This is what makes a local run demonstrable before Google OAuth exists
//! (P08), and it is gated twice: the flag has to be set *and* the origin has
//! to be localhost ([`dev_auth_allowed`](crate::config::dev_auth_allowed)).
//! When it is off the route answers **404**, not 403: a disabled back door
//! should not advertise that it is there.
//!
//! It reaches the domain through exactly the calls the Google callback will
//! use — `upsert_user` (which also resolves pending memberships, Q4) and
//! `open_session` — so P08 replaces the identity source and nothing else.

use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::auth::session_cookie::set_session_cookie;

/// Query parameters for the dev login.
#[derive(Debug, Deserialize)]
pub struct DevAuthQuery {
    /// The email to sign in as. Minted on first sight, like a real login.
    /// Optional in the type so a request that forgets it gets a usage
    /// message rather than an extractor's rejection.
    pub email: Option<String>,
}

/// Sign in as `email` and land on the app.
pub async fn get_dev_auth(
    State(state): State<AppState>,
    Query(query): Query<DevAuthQuery>,
) -> Response {
    if !state.config().dev_auth {
        return (StatusCode::NOT_FOUND, "dev auth is not enabled\n").into_response();
    }
    let Some(email) = query.email else {
        return (
            StatusCode::BAD_REQUEST,
            "usage: /auth/dev?email=you@example.com\n",
        )
            .into_response();
    };
    let email = email.trim().to_string();
    if !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
        return (
            StatusCode::BAD_REQUEST,
            "email must look like name@domain\n",
        )
            .into_response();
    }

    let display_name = email.split('@').next().unwrap_or(&email).to_string();
    // The provider identity a real login would carry. Namespaced so a dev
    // account can never collide with a Google `sub` (P08), which is an
    // opaque numeric string.
    let google_sub = format!("dev-auth:{}", email.to_lowercase());
    let ttl = state.config().session_ttl_seconds;

    let token = state
        .with_service(move |core| {
            // "dev" (P2's provider column) — `google_sub`'s own `dev-auth:`
            // namespace already keeps this from ever colliding with a real
            // Google subject, but `provider` is what `get_me` actually reads
            // for the wire's `providerLabel`, so it is set explicitly rather
            // than inferred from the sub text.
            let user = core
                .service
                .upsert_user(&google_sub, &email, &display_name, "dev");
            // User-agent capture is P3's job (auth-edge work); `None` here
            // is an honest "not wired up yet", not a placeholder value.
            core.service.open_session(user.uid, ttl, None)
        })
        .await;

    let cookie = set_session_cookie(&token, state.config().cookies_are_secure(), ttl);
    let mut response = Redirect::to("/").into_response();
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
