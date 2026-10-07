//! `GET /relay/device` — a board's one socket to the relay.
//!
//! Plain `ws://` in production, on purpose: the C6 cannot afford TLS
//! (+79 KB flash, ~29 KB heap a connection, measured 2026-10-01), and
//! everything that matters inside is sealed end to end by lp-link anyway.
//! This is the one path the http→https redirect leaves alone
//! (`crate::https_redirect`).
//!
//! The registration, in order, each step with a deadline:
//!
//! 1. The board's [`RelayHello`](lpc_relay::RelayHello). Its version is
//!    read first and refused by name if the hub does not list it.
//! 2. A fresh 32-byte challenge from the OS CSPRNG.
//! 3. The board's proofs, checked against the accounts its salts name —
//!    the one store call on this path, through `with_service`, once.
//! 4. `Registered` (or `Refused`), and the board is online until its leg
//!    closes.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use lp_cloud_domain::verify_board_accounts;
use lpc_relay::{
    MAX_RELAY_FRAME, PING_INTERVAL_S, RELAY_NONCE_BYTES, RefuseReason, RelayBoardId,
    RelayCloseCode, RelayFrame, RelayHello, check_relay_version,
};

use super::client_address::ClientAddress;
use super::leg_pump::{close, next_binary, pump_leg};
use super::relay_hub::BoardRegistration;
use super::relay_registry::{LegHandle, RelayRegistry};
use crate::app_state::AppState;

/// How long each registration step may take.
pub const REGISTRATION_STEP_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a board refused `TooManyBoards` or `Busy` is asked to wait.
const RETRY_AFTER_BUSY_S: u16 = 60;

/// Upgrade to a board's device leg.
pub async fn get_device_leg(
    State(state): State<AppState>,
    ClientAddress(ip): ClientAddress,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade
        .max_message_size(MAX_RELAY_FRAME)
        .max_frame_size(MAX_RELAY_FRAME)
        .on_upgrade(move |socket| run_device_leg(state, ip, socket))
}

async fn run_device_leg(state: AppState, ip: Option<IpAddr>, mut socket: WebSocket) {
    let relay = Arc::clone(state.relay());
    let mut handle = relay.open_leg();
    let mut shutdown = relay.shutdown_signal();
    let registered = tokio::select! {
        registered = register(&state, &relay, &mut handle, ip, &mut socket) => registered,
        _ = shutdown.changed() => {
            close(&mut socket, RelayCloseCode::GoingAway).await;
            None
        }
    };
    let Some(board) = registered else {
        return;
    };
    log::info!("relay: board {board} online");
    let started = std::time::Instant::now();
    let leg = handle.leg;
    let (end, traffic) =
        pump_leg(
            &mut socket,
            &mut handle.inbox,
            &mut shutdown,
            |bytes| match RelayFrame::decode(bytes) {
                Ok(frame) => {
                    relay.board_message(leg, frame);
                    Ok(())
                }
                Err(_) => Err(RelayCloseCode::PolicyViolation),
            },
        )
        .await;
    relay.board_gone(leg);
    log::info!(
        "relay: board {board} offline ({end}) after {}s, {} B in, {} B out",
        started.elapsed().as_secs(),
        traffic.bytes_in,
        traffic.bytes_out
    );
}

/// Steps 1–4. The board's id once it is online; `None` when it was refused
/// or went away (the refusal already sent).
async fn register(
    state: &AppState,
    relay: &RelayRegistry,
    handle: &mut LegHandle,
    ip: Option<IpAddr>,
    socket: &mut WebSocket,
) -> Option<RelayBoardId> {
    let first = step(next_binary(socket)).await?;
    let version = RelayFrame::hello_version(&first);
    if let Some(Err(reason)) = version.map(check_relay_version) {
        refuse(socket, reason).await;
        return None;
    }
    let Ok(RelayFrame::Hello(hello)) = RelayFrame::decode(&first) else {
        refuse(socket, RefuseReason::Malformed).await;
        return None;
    };

    let mut nonce = [0u8; RELAY_NONCE_BYTES];
    if getrandom::fill(&mut nonce).is_err() {
        refuse(socket, RefuseReason::Busy).await;
        return None;
    }
    send(socket, &RelayFrame::Challenge { nonce }).await?;
    let answer = step(next_binary(socket)).await?;
    let Ok(RelayFrame::Proof { proofs }) = RelayFrame::decode(&answer) else {
        refuse(socket, RefuseReason::Malformed).await;
        return None;
    };

    let RelayHello {
        board_mac,
        label,
        wire_proto,
        lan,
        accounts: salts,
        ..
    } = hello;
    let accounts = state
        .with_service(move |core| {
            verify_board_accounts(core.service.store(), &board_mac, &nonce, &salts, &proofs)
        })
        .await;
    if accounts.is_empty() {
        refuse(socket, RefuseReason::UnknownAccount).await;
        return None;
    }

    let id = RelayBoardId(board_mac);
    let accounts_ok = accounts.accounts_ok;
    let registration = BoardRegistration {
        id,
        leg: handle.leg,
        accounts,
        label,
        wire_proto,
        lan,
        public_ip: ip,
        since: epoch_seconds(),
    };
    if let Err(reason) = relay.register_board(registration, handle.take_outbox()) {
        refuse(socket, reason).await;
        return None;
    }
    send(
        socket,
        &RelayFrame::Registered {
            accounts_ok,
            ping_s: PING_INTERVAL_S,
        },
    )
    .await?;
    Some(id)
}

/// One registration step under [`REGISTRATION_STEP_TIMEOUT`].
async fn step<T>(work: impl Future<Output = Option<T>>) -> Option<T> {
    tokio::time::timeout(REGISTRATION_STEP_TIMEOUT, work)
        .await
        .ok()
        .flatten()
}

async fn send(socket: &mut WebSocket, frame: &RelayFrame) -> Option<()> {
    socket
        .send(Message::Binary(Bytes::from(frame.encode())))
        .await
        .ok()
}

/// Tell the board why, then close.
async fn refuse(socket: &mut WebSocket, reason: RefuseReason) {
    let retry_after_s = if reason.retries() {
        RETRY_AFTER_BUSY_S
    } else {
        0
    };
    log::info!("relay: board refused: {reason}");
    let _ = send(
        socket,
        &RelayFrame::Refused {
            reason,
            retry_after_s,
        },
    )
    .await;
    close(socket, RelayCloseCode::Normal).await;
}

fn epoch_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}
