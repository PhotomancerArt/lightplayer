use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::management_event::LinkManagementEventSink;
use crate::provider::management_request::LinkManagementRequest;
use crate::provider::management_result::LinkManagementResult;
use crate::provider::session::LinkSessionId;
use crate::providers::{LinkProviderDescriptor, LinkProviderKind};
use crate::{
    LinkConnection, LinkDiagnostic, LinkEndpoint, LinkError, LinkLogEntry, LinkProvider,
    LinkSession,
};

/// Owned, enum-dispatched provider handle shared across connection flows.
///
/// A connector comes from [`LinkProviderRegistry::create_connector`] — built
/// once per kind and memoized, so every flow sees the same instance and the
/// endpoint state it accumulates — or is handed in preconfigured by tests.
/// The `Rc` is held by whoever drives the connection — the studio's
/// `DeviceController`/`DeviceSession` since M4. All methods
/// take `&self` (each provider keeps its state behind internal `RefCell`s
/// with borrows scoped to synchronous sections), so the owner can hold
/// `Rc<LinkConnector>` and hand clones to client I/O adapters without any
/// shared mutable registry.
///
/// `LinkProvider` is not object-safe because it has async methods, so this
/// enum gives owners a single stored type while preserving concrete provider
/// ownership and forwarding the shared controller interface.
///
/// [`LinkProviderRegistry::create_connector`]: crate::providers::LinkProviderRegistry::create_connector
pub enum LinkConnector {
    Fake(crate::providers::fake::FakeProvider),
    #[cfg(feature = "host-process")]
    HostProcess(crate::providers::host_process::HostProcessProvider),
    #[cfg(feature = "host-serial-esp32")]
    HostSerialEsp32(crate::providers::host_serial_esp32::HostSerialEsp32Provider),
    #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
    BrowserWorker(crate::providers::browser_worker::BrowserWorkerProvider),
    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    BrowserSerialEsp32(crate::providers::browser_serial_esp32::BrowserSerialEsp32Provider),
}

impl LinkConnector {
    /// Descriptor for the concrete provider's kind.
    pub fn descriptor(&self) -> LinkProviderDescriptor {
        self.kind().descriptor()
    }
}

impl LinkProvider for LinkConnector {
    fn kind(&self) -> LinkProviderKind {
        match self {
            Self::Fake(provider) => provider.kind(),
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.kind(),
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.kind(),
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.kind(),
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.kind(),
        }
    }

    async fn discover(&self) -> Result<Vec<LinkEndpoint>, LinkError> {
        match self {
            Self::Fake(provider) => provider.discover().await,
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.discover().await,
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.discover().await,
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.discover().await,
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.discover().await,
        }
    }

    async fn status(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpointStatus, LinkError> {
        match self {
            Self::Fake(provider) => provider.status(endpoint_id).await,
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.status(endpoint_id).await,
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.status(endpoint_id).await,
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.status(endpoint_id).await,
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.status(endpoint_id).await,
        }
    }

    async fn connect(&self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError> {
        match self {
            Self::Fake(provider) => provider.connect(endpoint_id).await,
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.connect(endpoint_id).await,
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.connect(endpoint_id).await,
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.connect(endpoint_id).await,
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.connect(endpoint_id).await,
        }
    }

    async fn connection(&self, session_id: &LinkSessionId) -> Result<LinkConnection, LinkError> {
        match self {
            Self::Fake(provider) => provider.connection(session_id).await,
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.connection(session_id).await,
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.connection(session_id).await,
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.connection(session_id).await,
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.connection(session_id).await,
        }
    }

    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError> {
        match self {
            Self::Fake(provider) => provider.logs(session_id),
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.logs(session_id),
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.logs(session_id),
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.logs(session_id),
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.logs(session_id),
        }
    }

    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError> {
        match self {
            Self::Fake(provider) => provider.diagnostics(session_id),
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.diagnostics(session_id),
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.diagnostics(session_id),
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.diagnostics(session_id),
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.diagnostics(session_id),
        }
    }

    async fn manage(
        &self,
        session_id: &LinkSessionId,
        request: LinkManagementRequest,
    ) -> Result<LinkManagementResult, LinkError> {
        match self {
            Self::Fake(provider) => provider.manage(session_id, request).await,
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.manage(session_id, request).await,
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.manage(session_id, request).await,
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.manage(session_id, request).await,
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.manage(session_id, request).await,
        }
    }

    async fn manage_with_events(
        &self,
        session_id: &LinkSessionId,
        request: LinkManagementRequest,
        events: LinkManagementEventSink,
    ) -> Result<LinkManagementResult, LinkError> {
        match self {
            Self::Fake(provider) => {
                provider
                    .manage_with_events(session_id, request, events)
                    .await
            }
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => {
                provider
                    .manage_with_events(session_id, request, events)
                    .await
            }
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => {
                provider
                    .manage_with_events(session_id, request, events)
                    .await
            }
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => {
                provider
                    .manage_with_events(session_id, request, events)
                    .await
            }
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => {
                provider
                    .manage_with_events(session_id, request, events)
                    .await
            }
        }
    }

    async fn close(&self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        match self {
            Self::Fake(provider) => provider.close(session_id).await,
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.close(session_id).await,
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.close(session_id).await,
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.close(session_id).await,
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.close(session_id).await,
        }
    }
}

impl From<crate::providers::fake::FakeProvider> for LinkConnector {
    fn from(provider: crate::providers::fake::FakeProvider) -> Self {
        Self::Fake(provider)
    }
}

#[cfg(feature = "host-process")]
impl From<crate::providers::host_process::HostProcessProvider> for LinkConnector {
    fn from(provider: crate::providers::host_process::HostProcessProvider) -> Self {
        Self::HostProcess(provider)
    }
}

#[cfg(feature = "host-serial-esp32")]
impl From<crate::providers::host_serial_esp32::HostSerialEsp32Provider> for LinkConnector {
    fn from(provider: crate::providers::host_serial_esp32::HostSerialEsp32Provider) -> Self {
        Self::HostSerialEsp32(provider)
    }
}

#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
impl From<crate::providers::browser_worker::BrowserWorkerProvider> for LinkConnector {
    fn from(provider: crate::providers::browser_worker::BrowserWorkerProvider) -> Self {
        Self::BrowserWorker(provider)
    }
}

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
impl From<crate::providers::browser_serial_esp32::BrowserSerialEsp32Provider> for LinkConnector {
    fn from(provider: crate::providers::browser_serial_esp32::BrowserSerialEsp32Provider) -> Self {
        Self::BrowserSerialEsp32(provider)
    }
}
