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

    /// The sticky instance-fatal message a session's worker reported —
    /// only the browser-worker provider has this failure mode (a panic
    /// escaping its panic=abort wasm instance condemns the instance, and
    /// recovery is a worker reboot, not a retry). `None` for every other
    /// provider; the fake provider scripts one for crash-recovery tests.
    pub fn session_fatal(&self, session_id: &LinkSessionId) -> Option<String> {
        match self {
            Self::Fake(provider) => provider.session_fatal(session_id),
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.session_fatal(session_id),
            #[allow(
                unreachable_patterns,
                reason = "the non-fatal provider arms are feature/target-gated; \
                          in configurations where none exist, Fake covers everything"
            )]
            _ => None,
        }
    }

    /// Ask whether a ROM/stub bootloader is listening, and which chip it is.
    ///
    /// **Reboots the device** — the esptool SYNC handshake drives DTR/RTS to
    /// enter download mode, and on USB-Serial-JTAG that reset drops USB
    /// enumeration. Callers must hold the wire exclusively and rebuild
    /// afterwards; `DeviceSession::probe_link_mode` does both.
    ///
    /// `Ok(None)` = a bootloader answered but did not name its chip.
    /// `Err` = nothing answered, which does NOT prove the device is absent —
    /// it may simply be running the app.
    ///
    /// Providers with no bootloader concept (sim runtimes, host processes)
    /// report this as unsupported rather than pretending to answer.
    pub(crate) async fn probe_target(
        &self,
        session_id: &LinkSessionId,
        events: LinkManagementEventSink,
    ) -> Result<Option<String>, LinkError> {
        // Only the host provider surfaces probe progress as management
        // events; the browser provider collects its logs into the probe
        // result, and the rest have no bootloader to probe. Which arms exist
        // is feature-dependent, so bind it unconditionally.
        let _ = &events;
        match self {
            Self::Fake(provider) => provider.probe_target(session_id).await,
            #[cfg(feature = "host-process")]
            Self::HostProcess(_) => Err(LinkError::unsupported("probe_target")),
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.probe_target(session_id, events).await,
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(_) => Err(LinkError::unsupported("probe_target")),
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider
                .probe_target_for_session(session_id)
                .await
                .map(|result| result.chip_name),
        }
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

    // Delegated EXPLICITLY even though the trait defaults it: a wrapper
    // that inherits the default silently answers `Ok(false)` for every
    // provider — the grant would survive a forget while the UI said it
    // was gone. (The same shape once made a defaulted method a no-op
    // through this very enum; M4 session-targeted ops.)
    async fn forget_endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<bool, LinkError> {
        match self {
            Self::Fake(provider) => provider.forget_endpoint(endpoint_id).await,
            #[cfg(feature = "host-process")]
            Self::HostProcess(provider) => provider.forget_endpoint(endpoint_id).await,
            #[cfg(feature = "host-serial-esp32")]
            Self::HostSerialEsp32(provider) => provider.forget_endpoint(endpoint_id).await,
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            Self::BrowserWorker(provider) => provider.forget_endpoint(endpoint_id).await,
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            Self::BrowserSerialEsp32(provider) => provider.forget_endpoint(endpoint_id).await,
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
