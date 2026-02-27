//! Streaming transport: serializes ServerMessage in io_task, minimal buffering.
//!
//! Sends ServerMessage to OUTGOING_SERVER_MSG (capacity 1). io_task receives
//! and serializes with ser-write-json directly to serial. Never buffers full JSON.

extern crate alloc;

use alloc::vec::Vec;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use lp_model::{ClientMessage, ServerMessage, TransportError, json};
use lp_shared::transport::ServerTransport;

use crate::serial::io_task;

/// Streaming transport that sends ServerMessage to io_task for serialization
///
/// Uses Channel<ServerMessage, 1> - at most one message in flight.
/// transport.send(msg).await blocks until io_task receives (backpressure).
pub struct StreamingMessageRouterTransport {
    incoming: &'static Channel<CriticalSectionRawMutex, alloc::string::String, 32>,
    server_msg_channel: &'static Channel<CriticalSectionRawMutex, ServerMessage, 1>,
}

impl StreamingMessageRouterTransport {
    /// Create using channels from io_task
    pub fn from_io_channels() -> Self {
        let (incoming, _) = io_task::get_message_channels();
        let server_msg = io_task::get_server_msg_channel();
        Self {
            incoming,
            server_msg_channel: server_msg,
        }
    }
}

impl ServerTransport for StreamingMessageRouterTransport {
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        let id = msg.id;
        self.server_msg_channel.sender().send(msg).await;
        log::debug!(
            "StreamingMessageRouterTransport: Sent message id={} via server_msg channel",
            id
        );
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        let receiver = self.incoming.receiver();
        loop {
            match receiver.try_receive() {
                Ok(msg_line) => {
                    if !msg_line.starts_with("M!") {
                        log::trace!("StreamingMessageRouterTransport: Skipping non-message line");
                        continue;
                    }
                    let json_str = msg_line.strip_prefix("M!").unwrap_or(&msg_line);
                    let json_str = json_str.trim_end_matches('\n');
                    match json::from_str::<ClientMessage>(json_str) {
                        Ok(msg) => {
                            log::debug!(
                                "StreamingMessageRouterTransport: Received message id={}",
                                msg.id
                            );
                            return Ok(Some(msg));
                        }
                        Err(e) => {
                            log::warn!("StreamingMessageRouterTransport: Failed to parse: {e}");
                            continue;
                        }
                    }
                }
                Err(_) => return Ok(None),
            }
        }
    }

    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        let mut messages = Vec::new();
        loop {
            match self.receive().await? {
                Some(msg) => messages.push(msg),
                None => break,
            }
        }
        Ok(messages)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
