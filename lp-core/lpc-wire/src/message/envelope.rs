//! Bidirectional message envelope.

use crate::message::client::ClientMessage;
use crate::server::ServerMsgBody as ServerMessagePayload;
use serde::{Deserialize, Serialize};

/// Top-level JSON envelope (`client` / `server`).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Message {
    Client(ClientMessage),
    Server(ServerMessage),
}

/// Server message correlated to a client request id.
#[derive(Debug, Serialize, Deserialize)]
pub struct ServerMessage {
    pub id: u64,
    pub msg: ServerMessagePayload,
}
