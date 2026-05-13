//! Server-side transport trait
//!
//! Defines the interface for server-side transport implementations.
//! Messages are consumed (moved) on send, and receive is non-blocking.
//!
//! The transport handles serialization/deserialization internally.

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use lpc_model::Revision;
use lpc_wire::json::json_write::JsonWrite;
use lpc_wire::json::json_writer::{JsonWriter, JsonWriterError};
use lpc_wire::{
    ProjectProbeRequest, ProjectReadQuery, ProjectReadRequest, ProjectReadResponse, TransportError,
    WireProjectHandle, WireServerMessage, WireSlotMutationRequest, WireSlotMutationResponse,
    messages::ClientMessage,
};

/// Source that can write a project-read response to JSON without requiring the
/// caller to first allocate a full [`ProjectReadResponse`].
pub trait ProjectReadJsonSource {
    fn project_read_revision(&self) -> Revision;

    fn apply_project_mutations(
        &mut self,
        mutations: Vec<WireSlotMutationRequest>,
    ) -> Vec<WireSlotMutationResponse>;

    fn write_project_read_result_json<W>(
        &mut self,
        since: Option<Revision>,
        query: ProjectReadQuery,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite;

    fn write_project_probe_result_json<W>(
        &mut self,
        probe: ProjectProbeRequest,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite;

    fn write_project_read_json<W>(
        &mut self,
        request: ProjectReadRequest,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        let mutation_responses = self.apply_project_mutations(request.mutations);
        let mut writer = JsonWriter::new(out);
        writer.write_raw(b"{\"revision\":")?;
        writer.serde(&self.project_read_revision())?;
        writer.write_raw(b",\"results\":[")?;

        let since = request.since;
        for (index, query) in request.queries.into_iter().enumerate() {
            if index > 0 {
                writer.write_raw(b",")?;
            }
            let out = self.write_project_read_result_json(since, query, writer.into_inner())?;
            writer = JsonWriter::new(out);
        }

        writer.write_raw(b"],\"probes\":[")?;
        for (index, probe) in request.probes.into_iter().enumerate() {
            if index > 0 {
                writer.write_raw(b",")?;
            }
            let out = self.write_project_probe_result_json(probe, writer.into_inner())?;
            writer = JsonWriter::new(out);
        }

        writer.write_raw(b"],\"mutations\":[")?;
        for (index, mutation) in mutation_responses.into_iter().enumerate() {
            if index > 0 {
                writer.write_raw(b",")?;
            }
            writer.serde(&mutation)?;
        }

        writer.write_raw(b"]}")?;
        Ok(writer.into_inner())
    }
}

/// Trait for server-side transport implementations
///
/// This trait provides a simple polling-based interface for sending and receiving
/// messages. Messages are consumed (moved) on send, and receive is non-blocking
/// (returns `None` if no message is available).
///
/// The transport handles serialization/deserialization internally.
///
/// Separate from `ClientTransport` for clarity, even though the interface is
/// similar. This allows for different implementations or future extensions
/// specific to server-side use cases.
///
/// # Examples
///
/// ```rust,no_run
/// use lpc_shared::transport::ServerTransport;
/// use lpc_wire::{ClientMessage, TransportError};
/// use lpc_wire::WireServerMessage;
///
/// struct MyTransport;
///
/// impl ServerTransport for MyTransport {
///     async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
///         // Send message (transport handles serialization)
///         let _ = msg;
///         Ok(())
///     }
///
///     async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
///         // Receive message (transport handles deserialization)
///         Ok(None)
///     }
///
///     async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
///         Ok(Vec::new())
///     }
///
///     async fn close(&mut self) -> Result<(), TransportError> {
///         // Close the transport connection
///         Ok(())
///     }
/// }
/// ```
#[allow(async_fn_in_trait, reason = "trait async fn stable in Rust 1.75+")]
pub trait ServerTransport {
    /// Send a server message (consumes the message)
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError>;

    /// Stream a project-read response.
    ///
    /// Desktop transports may use the default fallback, which collects the
    /// response JSON and deserializes it back into the normal semantic response
    /// before sending. Firmware transports should override this with a bounded
    /// direct writer.
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
        let bytes = source
            .write_project_read_json(request, Vec::new())
            .map_err(|_| TransportError::Serialization("project read JSON write failed".into()))?;
        let response: ProjectReadResponse = lpc_wire::json::from_slice(&bytes)
            .map_err(|error| TransportError::Serialization(format!("{error}")))?;
        self.send(WireServerMessage {
            id,
            msg: lpc_wire::server::ServerMsgBody::ProjectRequest { response },
        })
        .await
    }

    /// Receive a client message (non-blocking). Returns `Ok(None)` if no message is available.
    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError>;

    /// Receive all available client messages (non-blocking)
    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError>;

    /// Close the transport connection
    async fn close(&mut self) -> Result<(), TransportError>;
}
