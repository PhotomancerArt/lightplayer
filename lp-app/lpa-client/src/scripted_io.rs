//! A scripted [`ClientIo`] for the conversation tests: every request is
//! recorded, responses come back in the scripted order, and a board that is
//! not answering yet is a count of dropped attempts.

use std::collections::VecDeque;

use async_trait::async_trait;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};

use crate::client_io::ClientIo;

pub(crate) struct ScriptedIo {
    pub(crate) sent: Vec<ClientMessage>,
    responses: VecDeque<WireServerMessage>,
    /// Attempts to fail before serving anything, standing in for a board
    /// whose littlefs format is eating requests.
    drops: u32,
}

impl ScriptedIo {
    pub(crate) fn new(responses: impl IntoIterator<Item = WireServerMessage>) -> Self {
        Self {
            sent: Vec::new(),
            responses: responses.into_iter().collect(),
            drops: 0,
        }
    }

    pub(crate) fn with_drops(mut self, drops: u32) -> Self {
        self.drops = drops;
        self
    }
}

#[async_trait(?Send)]
impl ClientIo for ScriptedIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        if self.drops > 0 {
            self.drops -= 1;
            return Err(TransportError::Other("no answer yet".to_string()));
        }
        self.responses
            .pop_front()
            .ok_or(TransportError::ConnectionLost)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
