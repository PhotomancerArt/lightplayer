use serde::{Deserialize, Serialize};

use crate::{LinkEndpointId, LinkSessionId};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkConnectionKind {
    Fake,
    LocalBrowserWorker { protocol: String },
    PendingImplementation { kind: String },
}

#[derive(Clone, Deserialize, Serialize)]
pub struct LinkConnection {
    pub endpoint_id: LinkEndpointId,
    pub session_id: LinkSessionId,
    pub kind: LinkConnectionKind,
    #[cfg(feature = "local-host")]
    #[serde(skip)]
    pub local_host_transport:
        Option<std::sync::Arc<tokio::sync::Mutex<Box<dyn lpa_client::ClientTransport>>>>,
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
            #[cfg(feature = "local-host")]
            local_host_transport: None,
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
            #[cfg(feature = "local-host")]
            local_host_transport: None,
        }
    }

    pub fn local_browser_worker(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::LocalBrowserWorker {
                protocol: "fw-browser-post-message-v1".to_string(),
            },
            #[cfg(feature = "local-host")]
            local_host_transport: None,
        }
    }

    #[cfg(feature = "local-host")]
    pub fn local_host(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: impl Into<LinkSessionId>,
        transport: std::sync::Arc<tokio::sync::Mutex<Box<dyn lpa_client::ClientTransport>>>,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            kind: LinkConnectionKind::PendingImplementation {
                kind: "local-host".to_string(),
            },
            local_host_transport: Some(transport),
        }
    }

    #[cfg(feature = "local-host")]
    pub fn local_host_transport(
        &self,
    ) -> Option<std::sync::Arc<tokio::sync::Mutex<Box<dyn lpa_client::ClientTransport>>>> {
        self.local_host_transport.clone()
    }
}
