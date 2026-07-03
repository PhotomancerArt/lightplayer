//! Accountable transport: serializes WireServerMessage in io_task, minimal buffering.
//!
//! Sends a server write request to io_task and waits for io_task to serialize
//! and write the message before returning success.

extern crate alloc;

use alloc::vec::Vec;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use lpc_shared::transport::ServerTransport;
use lpc_wire::WireServerMessage;
use lpc_wire::{ClientMessage, TransportError, json};

use crate::serial::io_task;

/// Server transport that sends WireServerMessage to io_task for serialization.
///
/// Uses a single in-flight write request/result pair. `send(msg).await` blocks
/// until io_task reports that the message was fully written or failed.
pub struct StreamingMessageRouterTransport {
    incoming: &'static Channel<CriticalSectionRawMutex, alloc::string::String, 32>,
    server_write_request: &'static Channel<CriticalSectionRawMutex, (u32, WireServerMessage), 1>,
    server_write_result:
        &'static Channel<CriticalSectionRawMutex, (u32, Result<(), TransportError>), 1>,
    /// Wrapping generation stamped on each write request. `send()` discards any
    /// result whose generation does not match, so a result orphaned by a
    /// cancelled send can never be misattributed to the next write.
    generation: u32,
}

impl StreamingMessageRouterTransport {
    /// Create using channels from io_task
    pub fn from_io_channels() -> Self {
        let (incoming, _) = io_task::get_message_channels();
        let (server_write_request, server_write_result) = io_task::get_server_write_channels();
        Self {
            incoming,
            server_write_request,
            server_write_result,
            generation: 0,
        }
    }
}

impl ServerTransport for StreamingMessageRouterTransport {
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        let id = msg.id;
        let generation = self.generation;
        self.generation = self.generation.wrapping_add(1);
        self.server_write_request
            .sender()
            .send((generation, msg))
            .await;
        // Await the result matching this generation. A mismatched result is a
        // stale response orphaned by a previously cancelled send; discard it and
        // keep waiting for the one io_task produced for this request.
        let result = loop {
            let (result_generation, result) = self.server_write_result.receiver().receive().await;
            if result_generation == generation {
                break result;
            }
            log::warn!(
                "StreamingMessageRouterTransport: discarding stale write result \
                 generation={result_generation} (awaiting {generation}) for id={id}"
            );
        };
        match &result {
            Ok(()) => log::debug!(
                "StreamingMessageRouterTransport: wrote message id={id} through io_task"
            ),
            Err(error) => log::warn!(
                "StreamingMessageRouterTransport: failed to write message id={id}: {error}"
            ),
        }
        result
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
                            log::debug!("StreamingMessageRouterTransport: Failed to parse: {e}");
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
