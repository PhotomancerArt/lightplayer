//! Bidirectional message envelope.

use super::client::ClientMessage;
use crate::server::ServerMsgBody as ServerMessagePayload;
use serde::{Deserialize, Serialize};

/// Placeholder for a future domain-split protocol surface.
#[allow(
    unreachable_code,
    clippy::empty_enums,
    reason = "NoDomain is intentionally uninhabited"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoDomain {}

/// Top-level JSON envelope (`client` / `server`).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Message<R> {
    Client(ClientMessage),
    Server(ServerMessage<R>),
}

/// Server message correlated to a client request id.
#[derive(Debug, Serialize, Deserialize)]
pub struct ServerMessage<R> {
    pub id: u64,
    pub msg: ServerMessagePayload<R>,
}
