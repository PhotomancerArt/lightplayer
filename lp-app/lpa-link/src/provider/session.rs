use crate::LinkCapabilities;
use crate::provider::connection::LinkConnectionKind;
use crate::provider::endpoint::LinkEndpointId;
use crate::providers::LinkProviderKind;
use serde::{Deserialize, Serialize};

/// Provider-neutral snapshot of a live link session.
///
/// A session begins when a provider successfully connects to a `LinkEndpoint`.
/// The concrete resources below the session, such as browser serial ports,
/// workers, spawned host runtimes, and protocol streams, remain owned by the
/// provider that created the session.
///
/// A `LinkSession` is not itself the `lp-server` client protocol and does not
/// own resources directly. Call `LinkProvider::connection()` with the session id
/// when the caller needs the protocol handoff.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkSession {
    /// Provider-scoped session id used for connection/log/close operations.
    pub id: LinkSessionId,
    /// Built-in provider kind that owns the concrete resources for this session.
    pub provider_kind: LinkProviderKind,
    /// Endpoint that was connected to create this session.
    pub endpoint_id: LinkEndpointId,
    /// Protocol/transport shape exposed by the session.
    pub connection_kind: LinkConnectionKind,
    /// Operations available through this live session.
    pub capabilities: LinkCapabilities,
    /// Provider-neutral session lifecycle state.
    pub status: LinkSessionStatus,
}

impl LinkSession {
    pub fn new(
        id: impl Into<LinkSessionId>,
        provider_kind: impl Into<LinkProviderKind>,
        endpoint_id: impl Into<LinkEndpointId>,
        connection_kind: LinkConnectionKind,
        capabilities: LinkCapabilities,
    ) -> Self {
        Self {
            id: id.into(),
            provider_kind: provider_kind.into(),
            endpoint_id: endpoint_id.into(),
            connection_kind,
            capabilities,
            status: LinkSessionStatus::Open,
        }
    }

    pub fn id(&self) -> &LinkSessionId {
        &self.id
    }

    pub fn endpoint_id(&self) -> &LinkEndpointId {
        &self.endpoint_id
    }

    pub fn with_status(mut self, status: LinkSessionStatus) -> Self {
        self.status = status;
        self
    }
}

/// Opaque provider-scoped live session identity.
///
/// The provider that created a session owns the underlying resources. Pass this
/// id back to that provider to request a protocol connection, logs,
/// diagnostics, or resource cleanup.
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

/// Provider-neutral live session lifecycle state.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkSessionStatus {
    Open,
    Closing,
    Closed,
    Error { message: String },
}
