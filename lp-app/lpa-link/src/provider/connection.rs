use serde::{Deserialize, Serialize};

use crate::provider::endpoint::LinkEndpointId;
use crate::provider::session::LinkSessionId;

/// Handoff from a live link session to the `lp-server` protocol layer.
///
/// `LinkConnection` is created by `LinkProvider::connection()`. It identifies
/// which endpoint/session produced the server protocol connection and describes
/// the provider/runtime flavor used to reach that server.
///
/// It is not an endpoint and it does not own the whole session lifecycle. Keep
/// the provider-owned session open while using the connection.
#[derive(Clone, Deserialize, Serialize)]
pub struct LinkConnection {
    /// Endpoint that the owning session was opened from.
    pub endpoint_id: LinkEndpointId,
    /// Live session that produced this connection.
    pub session_id: LinkSessionId,
    /// Provider/runtime flavor for this protocol connection.
    pub kind: LinkConnectionKind,
    #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
    /// Host-side protocol channel for links that can expose one directly.
    #[serde(skip)]
    pub server_connection: Option<LinkServerConnection>,
}

/// Host-side server protocol connection opened by a link session.
///
/// Browser links currently expose protocol identity in `LinkConnectionKind`;
/// their actual streams are owned by the web runtime and should be adapted into
/// `lpa_client::ClientIo`.
#[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
pub type LinkServerConnection = lpa_client::SharedClientTransport;
/// Compatibility alias for the previous host transport name.
#[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
pub type LinkClientTransport = LinkServerConnection;

/// Transport/runtime flavor for a server protocol connection.
///
/// Browser variants include protocol identity, but browser-owned streams still
/// live in the web runtime. Host variants may carry a `LinkServerConnection`.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkConnectionKind {
    Fake,
    HostProcess,
    BrowserWorker { protocol: String },
    HostSerialEsp32,
    BrowserSerialEsp32 { protocol: String },
}

impl LinkConnection {
    pub fn fake(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::Fake,
            #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
            server_connection: None,
        }
    }

    pub fn browser_worker(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::BrowserWorker {
                protocol: "fw-browser-post-message-v1".to_string(),
            },
            #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
            server_connection: None,
        }
    }

    pub fn browser_serial_esp32(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::BrowserSerialEsp32 {
                protocol: "lp-serial-json-lines-v1".to_string(),
            },
            #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
            server_connection: None,
        }
    }

    #[cfg(feature = "host-process")]
    pub fn host_process(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
        server_connection: LinkServerConnection,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::HostProcess,
            server_connection: Some(server_connection),
        }
    }

    #[cfg(feature = "host-serial-esp32")]
    pub fn host_serial_esp32(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
        server_connection: LinkServerConnection,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::HostSerialEsp32,
            server_connection: Some(server_connection),
        }
    }

    #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
    /// Return the host protocol channel opened by this connection.
    pub fn server_connection(&self) -> Option<LinkServerConnection> {
        self.server_connection.clone()
    }

    #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
    /// Wrap the host protocol channel with the Tokio client adapter.
    pub fn server_client(&self) -> Option<lpa_client::TokioLpClient> {
        self.server_connection()
            .map(lpa_client::TokioLpClient::new_shared)
    }

    #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
    /// Deprecated compatibility shim for callers still using transport wording.
    pub fn client_transport(&self) -> Option<LinkClientTransport> {
        self.server_connection()
    }
}
