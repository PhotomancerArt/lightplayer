use crate::link_endpoint::LinkEndpointId;
use crate::{LinkConnection, LinkDiagnostic, LinkError, LinkLogEntry};
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct LinkSessionId(String);

impl LinkSessionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for LinkSessionId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for LinkSessionId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
