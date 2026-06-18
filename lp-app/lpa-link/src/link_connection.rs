use serde::{Deserialize, Serialize};

use crate::{LinkEndpointId, LinkSessionId};

#[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
pub type LinkClientTransport =
    std::sync::Arc<tokio::sync::Mutex<Box<dyn lpa_client::ClientTransport>>>;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkConnectionKind {
    Fake,
    HostProcess,
    BrowserWorker { protocol: String },
    HostSerialEsp32,
    PendingImplementation { kind: String },
}

#[derive(Clone, Deserialize, Serialize)]
pub struct LinkConnection {
    pub endpoint_id: LinkEndpointId,
    pub session_id: LinkSessionId,
    pub kind: LinkConnectionKind,
    #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
    #[serde(skip)]
    pub client_transport: Option<LinkClientTransport>,
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
            client_transport: None,
        }
    }

    pub fn pending(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::PendingImplementation { kind: kind.into() },
            #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
            client_transport: None,
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
            client_transport: None,
        }
    }

    #[cfg(feature = "host-process")]
    pub fn host_process(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
        transport: LinkClientTransport,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::HostProcess,
            client_transport: Some(transport),
        }
    }

    #[cfg(feature = "host-serial-esp32")]
    pub fn host_serial_esp32(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
        transport: LinkClientTransport,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::HostSerialEsp32,
            client_transport: Some(transport),
        }
    }

    #[cfg(any(feature = "host-process", feature = "host-serial-esp32"))]
    pub fn client_transport(&self) -> Option<LinkClientTransport> {
        self.client_transport.clone()
    }
}
