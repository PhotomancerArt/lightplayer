//! Accountable transport: serializes in thread context, io_task writes bytes.
//!
//! `send` serializes the WireServerMessage HERE (thread context) into the
//! shared static frame buffer, submits its length to io_task, and waits for
//! io_task to report the write's outcome before returning — which is also
//! what makes the single buffer sound (one frame in flight, ever). io_task never serializes: on the classic its
//! polls run in interrupt context on a borrowed stack, and serialization
//! recursion there corrupted the system on the bench (see
//! `serial::server_msg`'s module docs and the 2026-08-25 ADR).

use alloc::vec::Vec;

use crate::serial::server_msg::serialize_server_msg;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use lpc_shared::transport::ServerTransport;
use lpc_wire::WireServerMessage;
use lpc_wire::{ClientMessage, TransportError, json};

/// Server transport that sends WireServerMessage to io_task for serialization.
///
/// Uses a single in-flight write request/result pair. `send(msg).await` blocks
/// until io_task reports that the message was fully written or failed.
pub struct StreamingMessageRouterTransport {
    incoming: &'static Channel<CriticalSectionRawMutex, alloc::string::String, 32>,
    server_write_request: &'static Channel<CriticalSectionRawMutex, (u32, usize), 1>,
    server_write_result:
        &'static Channel<CriticalSectionRawMutex, (u32, Result<(), TransportError>), 1>,
    /// Wrapping generation stamped on each write request. `send()` discards any
    /// result whose generation does not match, so a result orphaned by a
    /// cancelled send can never be misattributed to the next write.
    generation: u32,
}

impl StreamingMessageRouterTransport {
    /// Create from the chip crate's io-task channels: the incoming message-line
    /// channel plus the single-slot write request/result pair.
    pub fn new(
        incoming: &'static Channel<CriticalSectionRawMutex, alloc::string::String, 32>,
        server_write_request: &'static Channel<CriticalSectionRawMutex, (u32, usize), 1>,
        server_write_result: &'static Channel<
            CriticalSectionRawMutex,
            (u32, Result<(), TransportError>),
            1,
        >,
    ) -> Self {
        Self {
            incoming,
            server_write_request,
            server_write_result,
            generation: 0,
        }
    }
}

impl StreamingMessageRouterTransport {
    /// One accountable write through io_task: serialize HERE (thread
    /// context — serialization recursion and the frame buffer must never ride
    /// the io task's interrupt-context stack; see `serial::server_msg`),
    /// submit the framed bytes, then await the generation-matched result. A
    /// mismatched result is a stale response orphaned by a previously
    /// cancelled send; discard it and keep waiting for the one io_task
    /// produced for this request.
    async fn write_once(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        let id = msg.id;
        // Fills the shared static frame buffer; sending the LENGTH hands the
        // buffer to the io task, and awaiting the matching result below is
        // what makes reusing it for the next message sound (see FRAME_BUF).
        let len = serialize_server_msg(&msg)?;
        let generation = self.generation;
        self.generation = self.generation.wrapping_add(1);
        self.server_write_request
            .sender()
            .send((generation, len))
            .await;
        loop {
            let (result_generation, result) = self.server_write_result.receiver().receive().await;
            if result_generation == generation {
                break result;
            }
            log::warn!(
                "StreamingMessageRouterTransport: discarding stale write result \
                 generation={result_generation} (awaiting {generation}) for id={id}"
            );
        }
    }
}

impl ServerTransport for StreamingMessageRouterTransport {
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        let id = msg.id;
        // Captured before the message moves: a failed Error notice must not
        // recurse into another notice.
        let is_error_frame = matches!(msg.msg, lpc_wire::server::ServerMsgBody::Error { .. });
        let result = self.write_once(msg).await;
        match &result {
            Ok(()) => log::debug!(
                "StreamingMessageRouterTransport: wrote message id={id} through io_task"
            ),
            Err(error) => {
                // io_task has already retried per its link's write policy, so
                // this drop is final — say so at error level, never as a
                // debuggable-away warn (the debt entry's "no silent drop with
                // responses=0" criterion).
                log::error!("StreamingMessageRouterTransport: dropping message id={id}: {error}");
                // Best-effort: tell the peer its response was dropped, with
                // the original request id so the client can fail that call
                // instead of timing out. Skipped when the link is gone
                // (nobody to tell) and for Error frames themselves (no
                // recursion); a failed notice is only logged.
                if !is_error_frame && !matches!(error, TransportError::ConnectionLost) {
                    let notice = WireServerMessage::new(
                        id,
                        lpc_wire::server::ServerMsgBody::Error {
                            error: alloc::format!("response id={id} dropped: {error}"),
                        },
                    );
                    if self.write_once(notice).await.is_err() {
                        log::error!(
                            "StreamingMessageRouterTransport: error notice for id={id} \
                             also failed; peer left waiting"
                        );
                    }
                }
            }
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
