//! Streaming transport: serializes WireServerMessage in io_task, minimal buffering.
//!
//! Sends WireServerMessage to OUTGOING_SERVER_MSG (capacity 1). io_task receives
//! and serializes with ser-write-json directly to serial. Never buffers full JSON.

extern crate alloc;

use alloc::{format, vec::Vec};
use core::fmt;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use lpc_shared::transport::{ProjectReadJsonSource, ServerTransport};
use lpc_wire::WireServerMessage;
use lpc_wire::json::json_write::JsonWrite;
use lpc_wire::json::json_writer::JsonWriter;
use lpc_wire::{ClientMessage, TransportError, json};
use lpc_wire::{ProjectReadRequest, WireProjectHandle};

use crate::serial::io_task;
use crate::serial::io_task::{SERVER_JSON_CHUNK_CAPACITY, SERVER_JSON_CHUNK_SIZE, ServerJsonChunk};

/// Streaming transport that sends WireServerMessage to io_task for serialization
///
/// Uses Channel<WireServerMessage, 1> - at most one message in flight.
/// transport.send(msg).await blocks until io_task receives (backpressure).
pub struct StreamingMessageRouterTransport {
    incoming: &'static Channel<CriticalSectionRawMutex, alloc::string::String, 32>,
    server_msg_channel: &'static Channel<CriticalSectionRawMutex, WireServerMessage, 1>,
    server_json_chunk_channel:
        &'static Channel<CriticalSectionRawMutex, ServerJsonChunk, SERVER_JSON_CHUNK_CAPACITY>,
}

impl StreamingMessageRouterTransport {
    /// Create using channels from io_task
    pub fn from_io_channels() -> Self {
        let (incoming, _) = io_task::get_message_channels();
        let server_msg = io_task::get_server_msg_channel();
        let server_json_chunk = io_task::get_server_json_chunk_channel();
        Self {
            incoming,
            server_msg_channel: server_msg,
            server_json_chunk_channel: server_json_chunk,
        }
    }
}

#[derive(Debug)]
struct ChunkedJsonError;

impl fmt::Display for ChunkedJsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "serial JSON chunk queue full ({} KiB frame budget exhausted)",
            (SERVER_JSON_CHUNK_SIZE * SERVER_JSON_CHUNK_CAPACITY) / 1024
        )
    }
}

struct ChunkedJsonWriter {
    channel: &'static Channel<CriticalSectionRawMutex, ServerJsonChunk, SERVER_JSON_CHUNK_CAPACITY>,
    buf: [u8; SERVER_JSON_CHUNK_SIZE],
    len: usize,
}

impl ChunkedJsonWriter {
    fn new(
        channel: &'static Channel<
            CriticalSectionRawMutex,
            ServerJsonChunk,
            SERVER_JSON_CHUNK_CAPACITY,
        >,
    ) -> Self {
        Self {
            channel,
            buf: [0; SERVER_JSON_CHUNK_SIZE],
            len: 0,
        }
    }

    fn flush(&mut self) -> Result<(), ChunkedJsonError> {
        if self.len == 0 {
            return Ok(());
        }
        let chunk = ServerJsonChunk::from_slice(&self.buf[..self.len]).ok_or(ChunkedJsonError)?;
        self.channel
            .sender()
            .try_send(chunk)
            .map_err(|_| ChunkedJsonError)?;
        self.len = 0;
        Ok(())
    }
}

impl JsonWrite for ChunkedJsonWriter {
    type Error = ChunkedJsonError;

    fn write_all(&mut self, mut bytes: &[u8]) -> Result<(), Self::Error> {
        while !bytes.is_empty() {
            let free = self.buf.len().saturating_sub(self.len);
            if free == 0 {
                self.flush()?;
                continue;
            }

            let take = free.min(bytes.len());
            self.buf[self.len..self.len + take].copy_from_slice(&bytes[..take]);
            self.len += take;
            bytes = &bytes[take..];
        }
        Ok(())
    }
}

impl ServerTransport for StreamingMessageRouterTransport {
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        let id = msg.id;
        self.server_msg_channel.sender().send(msg).await;
        log::debug!("StreamingMessageRouterTransport: Sent message id={id} via server_msg channel");
        Ok(())
    }

    async fn send_project_read<S>(
        &mut self,
        id: u64,
        _handle: WireProjectHandle,
        source: &mut S,
        request: ProjectReadRequest,
    ) -> Result<(), TransportError>
    where
        S: ProjectReadJsonSource,
    {
        let result = write_project_read_chunks(self.server_json_chunk_channel, id, source, request);

        if result.is_err() {
            if let Some(chunk) = ServerJsonChunk::from_slice(b"\n") {
                self.server_json_chunk_channel.sender().send(chunk).await;
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

fn write_project_read_chunks<S>(
    channel: &'static Channel<CriticalSectionRawMutex, ServerJsonChunk, SERVER_JSON_CHUNK_CAPACITY>,
    id: u64,
    source: &mut S,
    request: ProjectReadRequest,
) -> Result<(), TransportError>
where
    S: ProjectReadJsonSource,
{
    let mut writer = ChunkedJsonWriter::new(channel);
    writer
        .write_all(b"M!")
        .map_err(|_| TransportError::ConnectionLost)?;

    let mut json = JsonWriter::new(writer);
    json.write_raw(b"{\"id\":")
        .and_then(|_| json.u64(id))
        .and_then(|_| json.write_raw(b",\"msg\":{\"projectRequest\":{\"response\":"))
        .map_err(|_| TransportError::Serialization("project-read header write failed".into()))?;

    let out = source
        .write_project_read_json(request, json.into_inner())
        .map_err(|error| {
            TransportError::Serialization(format!("project-read JSON write failed: {error}"))
        })?;

    let mut json = JsonWriter::new(out);
    json.write_raw(b"}}}\n")
        .map_err(|_| TransportError::Serialization("project-read footer write failed".into()))?;
    let mut writer = json.into_inner();
    writer.flush().map_err(|_| TransportError::ConnectionLost)?;
    Ok(())
}
