use crate::{
    LinkConnection, LinkDiagnostic, LinkEndpointId, LinkError, LinkLogEntry, LinkSessionId,
};

/// Live ownership of a connected endpoint.
///
/// A session begins when a provider successfully connects to a `LinkEndpoint`.
/// It owns the lifecycle below the server protocol connection: an open serial
/// port, a spawned `fw-host` runtime, a browser worker identity, or another
/// provider-specific live resource.
///
/// A session is not itself the `lp-server` client protocol. Call
/// `connection()` to get the protocol handoff for runtimes that expose one.
#[allow(async_fn_in_trait, reason = "Link sessions are not object-safe yet")]
pub trait LinkSession {
    /// Stable id for this live session.
    fn id(&self) -> &LinkSessionId;

    /// Endpoint this session was opened from.
    fn endpoint_id(&self) -> &LinkEndpointId;

    /// Link-level logs available through the session.
    fn logs(&self) -> Vec<LinkLogEntry>;

    /// Link-level diagnostics available through the session.
    fn diagnostics(&self) -> Vec<LinkDiagnostic>;

    /// Open or return the client connection associated with this session.
    ///
    /// The session owns lifecycle below the connection. For `host-process`, that
    /// means keeping the in-process `fw-host` runtime alive while the returned
    /// transport is in use.
    async fn connection(&mut self) -> Result<LinkConnection, LinkError>;

    /// Close provider-owned live resources for this session.
    async fn close(&mut self) -> Result<(), LinkError>;
}
