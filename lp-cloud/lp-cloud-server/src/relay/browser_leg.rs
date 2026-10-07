//! `GET /relay/board/{id}` — one browser session to one board.
//!
//! `wss://` in production (the redirect sends a plain request to https
//! first). Every binary message is one lp-link frame, carried to the board
//! inside a route and back unchanged: the session is Noise-sealed end to
//! end, so the relay reads none of it, and a browser's existing lp-link
//! code runs above this socket as it does above a LAN one.
//!
//! The upgrade always succeeds for a well-formed id; who may actually open
//! a session is decided after it ([`super::route_admission`]), and a
//! refusal is the close code, because a browser cannot read the status of a
//! refused upgrade.

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use lpc_cloud_api::Actor;
use lpc_history::PrefixedUid;
use lpc_relay::{MAX_RELAY_FRAME, ROUTE_FRAME_OVERHEAD, RelayBoardId};

use super::client_address::ClientAddress;
use super::leg_pump::{close, pump_leg};
use crate::app_state::AppState;
use crate::auth::session_cookie::session_token;

/// The largest message a browser may send: what fits one route frame.
pub const MAX_BROWSER_MESSAGE: usize = MAX_RELAY_FRAME - ROUTE_FRAME_OVERHEAD;

/// Upgrade to a browser session on board `id`.
pub async fn get_browser_leg(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ClientAddress(ip): ClientAddress,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Ok(board) = id.parse::<RelayBoardId>() else {
        return (StatusCode::NOT_FOUND, "not a board id\n").into_response();
    };
    let token = session_token(&headers);
    let user = state
        .with_service(move |core| match core.actor_for(token.as_deref()) {
            Actor::User(uid) => Some(uid),
            Actor::Anonymous => None,
        })
        .await;
    upgrade
        .max_message_size(MAX_BROWSER_MESSAGE)
        .max_frame_size(MAX_BROWSER_MESSAGE)
        .on_upgrade(move |socket| run_browser_leg(state, socket, board, user, ip))
}

async fn run_browser_leg(
    state: AppState,
    mut socket: WebSocket,
    board: RelayBoardId,
    user: Option<PrefixedUid>,
    ip: Option<IpAddr>,
) {
    let relay = Arc::clone(state.relay());
    let mut handle = relay.open_leg();
    let leg = handle.leg;
    let admitted = match relay.open_route(leg, handle.take_outbox(), user, ip, board) {
        Ok(admitted) => admitted,
        Err(code) => {
            log::info!("relay: session to {board} refused: {}", code.reason());
            close(&mut socket, code).await;
            return;
        }
    };
    let route = admitted.route;
    log::info!(
        "relay: session {board}/{route} open ({})",
        match admitted.admission {
            super::route_admission::Admission::Member => "member",
            super::route_admission::Admission::Visitor => "visitor",
        }
    );
    let started = std::time::Instant::now();
    let mut shutdown = relay.shutdown_signal();
    let (end, traffic) = pump_leg(&mut socket, &mut handle.inbox, &mut shutdown, |bytes| {
        relay.browser_message(leg, bytes);
        Ok(())
    })
    .await;
    relay.browser_gone(leg);
    log::info!(
        "relay: session {board}/{route} ended ({end}) after {}s, {} B in, {} B out",
        started.elapsed().as_secs(),
        traffic.bytes_in,
        traffic.bytes_out
    );
}
