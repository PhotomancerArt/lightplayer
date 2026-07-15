use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::session::LinkSessionId;
use crate::providers::{LinkProviderDescriptor, LinkProviderKind};
use crate::{
    LinkConnection, LinkConnectionKind, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpoint,
    LinkError, LinkLogEntry, LinkLogLevel, LinkProvider, LinkSession, LinkSessionStatus,
};

pub fn descriptor() -> LinkProviderDescriptor {
    LinkProviderKind::Fake.descriptor()
}

/// The test provider.
///
/// Two tiers coexist:
///
/// - **Record-level endpoints** ([`Self::with_endpoint`] plus the
///   `with_*_error` knobs): cheap fakes for unit tests of link/session
///   bookkeeping. Their connections carry no server protocol.
/// - **Device-backed endpoints** ([`Self::with_device_endpoint`], feature
///   `fake-device`): a scripted [`FakeEsp32Device`] behind the REAL serial
///   framing. `connect()` runs the same transport machinery as the hardware
///   path over the fake's byte stream, and `connection()` hands out a
///   [`LinkConnection`] carrying a real `LinkServerConnection`. `manage()`
///   implements Flash/Erase/Reset as scripted device transitions.
///
/// Session state lives behind an internal `RefCell` (borrows scoped to
/// synchronous sections), so the provider serves `&self` callers through a
/// shared `LinkConnector`.
///
/// [`FakeEsp32Device`]: crate::providers::fake_device::FakeEsp32Device
pub struct FakeProvider {
    endpoints: Vec<LinkEndpoint>,
    sessions: RefCell<BTreeMap<LinkSessionId, FakeSessionState>>,
    next_session_index: Cell<u64>,
    discover_error: Option<String>,
    connect_error: Option<String>,
    connection_error: Option<String>,
    #[cfg(feature = "fake-device")]
    devices: BTreeMap<LinkEndpointId, crate::providers::fake_device::FakeEsp32Device>,
}

impl FakeProvider {
    pub fn new() -> Self {
        Self {
            endpoints: Vec::new(),
            sessions: RefCell::new(BTreeMap::new()),
            next_session_index: Cell::new(1),
            discover_error: None,
            connect_error: None,
            connection_error: None,
            #[cfg(feature = "fake-device")]
            devices: BTreeMap::new(),
        }
    }

    pub fn with_endpoint(mut self, endpoint: LinkEndpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    pub fn with_discover_error(mut self, message: impl Into<String>) -> Self {
        self.discover_error = Some(message.into());
        self
    }

    pub fn with_connect_error(mut self, message: impl Into<String>) -> Self {
        self.connect_error = Some(message.into());
        self
    }

    pub fn with_connection_error(mut self, message: impl Into<String>) -> Self {
        self.connection_error = Some(message.into());
        self
    }

    /// Register a scripted fake device endpoint (full capability set —
    /// tests need the flash/erase paths).
    #[cfg(feature = "fake-device")]
    pub fn with_device_endpoint(
        mut self,
        endpoint_id: impl Into<LinkEndpointId>,
        label: impl Into<String>,
        script: crate::providers::fake_device::FakeDeviceScript,
    ) -> Self {
        use crate::LinkCapabilities;
        let endpoint_id = endpoint_id.into();
        self.endpoints.push(
            LinkEndpoint::new(endpoint_id.clone(), LinkProviderKind::Fake, label)
                .with_capabilities(
                    LinkCapabilities::esp32_serial_base()
                        .with_flash()
                        .with_device_erase(),
                ),
        );
        self.devices.insert(
            endpoint_id,
            crate::providers::fake_device::FakeEsp32Device::new(script),
        );
        self
    }

    /// The scripted device behind an endpoint, for test-side failure
    /// injection and assertions.
    #[cfg(feature = "fake-device")]
    pub fn device(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Option<crate::providers::fake_device::FakeEsp32Device> {
        self.devices.get(endpoint_id).cloned()
    }

    /// Drain the serial lines observed on a device-backed session (every
    /// complete line, protocol and logs alike) — the host analogue of the
    /// browser serial provider's `take_lines`.
    #[cfg(feature = "fake-device")]
    pub fn take_lines(&self, session_id: &LinkSessionId) -> Result<Vec<String>, LinkError> {
        let lines = {
            let sessions = self.sessions.borrow();
            let state = sessions
                .get(session_id)
                .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
            match &state.observed_lines {
                Some(lines) => std::sync::Arc::clone(lines),
                None => return Ok(Vec::new()),
            }
        };
        let mut lines = lines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(core::mem::take(&mut *lines))
    }

    fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<&LinkEndpoint, LinkError> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.id == *endpoint_id)
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }

    /// Open the real serial transport machinery over the device's byte
    /// stream: the same framing thread the hardware path uses, with a
    /// reset-after-open so the boot output is captured, and a line buffer
    /// standing in for the browser provider's observed-line surface.
    #[cfg(feature = "fake-device")]
    fn open_device_transport(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<Option<FakeDeviceSessionResources>, LinkError> {
        use std::sync::Arc;

        use lpa_client::transport_serial::{
            HardwareSerialOptions, SerialLineObserver,
            create_hardware_serial_transport_pair_with_options,
        };

        let Some(device) = self.devices.get(endpoint_id).cloned() else {
            return Ok(None);
        };

        struct BufferedLineObserver(Arc<std::sync::Mutex<Vec<String>>>);
        impl SerialLineObserver for BufferedLineObserver {
            fn observe_line(&self, line: &str) {
                self.0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(line.to_string());
            }
        }

        let lines = Arc::new(std::sync::Mutex::new(Vec::new()));
        let options = HardwareSerialOptions {
            reset_after_open: true,
            line_observer: Some(Arc::new(BufferedLineObserver(Arc::clone(&lines)))),
        };
        let stream = crate::providers::fake_device::FakeDeviceByteStream::new(device.clone());
        let transport = create_hardware_serial_transport_pair_with_options(
            Box::new(stream),
            endpoint_id.as_str(),
            options,
        )
        .map_err(|error| LinkError::ConnectionFailed {
            message: error.to_string(),
        })?;
        let transport: Box<dyn lpa_client::ClientTransport> = Box::new(transport);
        let server_connection: crate::LinkServerConnection =
            std::sync::Arc::new(tokio::sync::Mutex::new(transport));
        Ok(Some(FakeDeviceSessionResources {
            device,
            server_connection,
            lines,
        }))
    }
}

impl LinkProvider for FakeProvider {
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::Fake
    }

    async fn discover(&self) -> Result<Vec<LinkEndpoint>, LinkError> {
        if let Some(message) = &self.discover_error {
            return Err(LinkError::ConnectionFailed {
                message: message.clone(),
            });
        }
        Ok(self.endpoints.clone())
    }

    async fn status(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpointStatus, LinkError> {
        Ok(self.endpoint(endpoint_id)?.status.clone())
    }

    async fn connect(&self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError> {
        if let Some(message) = &self.connect_error {
            return Err(LinkError::ConnectionFailed {
                message: message.clone(),
            });
        }
        let endpoint = self.endpoint(endpoint_id)?.clone();
        let session_index = self.next_session_index.get();
        self.next_session_index.set(session_index + 1);
        let session_id = LinkSessionId::new(format!("{}:{}", endpoint_id.as_str(), session_index));

        let session = LinkSession::new(
            session_id.clone(),
            self.kind(),
            endpoint.id.clone(),
            LinkConnectionKind::Fake,
            endpoint.capabilities.clone(),
        );
        let state = FakeSessionState::new(endpoint.id, session.clone());
        #[cfg(feature = "fake-device")]
        let state = match self.open_device_transport(endpoint_id)? {
            Some(resources) => {
                let mut state = state;
                state.device = Some(resources.device);
                state.server_connection = Some(resources.server_connection);
                state.observed_lines = Some(resources.lines);
                state
            }
            None => state,
        };
        self.sessions.borrow_mut().insert(session_id, state);
        Ok(session)
    }

    async fn connection(&self, session_id: &LinkSessionId) -> Result<LinkConnection, LinkError> {
        if let Some(message) = &self.connection_error {
            return Err(LinkError::ConnectionFailed {
                message: message.clone(),
            });
        }
        let sessions = self.sessions.borrow();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        if state.session.status == LinkSessionStatus::Closed {
            return Err(LinkError::Closed);
        }
        #[cfg(feature = "fake-device")]
        if let Some(server_connection) = &state.server_connection {
            return Ok(LinkConnection::fake_device(
                state.session.endpoint_id.clone(),
                state.session.id.clone(),
                server_connection.clone(),
            ));
        }
        Ok(LinkConnection::fake(
            state.session.endpoint_id.clone(),
            state.session.id.clone(),
        ))
    }

    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError> {
        let sessions = self.sessions.borrow();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        Ok(state.logs.clone())
    }

    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError> {
        let sessions = self.sessions.borrow();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        Ok(state.diagnostics.clone())
    }

    /// Scripted management transitions on device-backed sessions
    /// (feature `fake-device`); record-level sessions keep the default
    /// unsupported behavior. Progress/log events reach callers through the
    /// default `manage_with_events`, which replays the result's logs and
    /// progress into the sink — the same event surface the browser provider
    /// feeds live.
    async fn manage(
        &self,
        session_id: &LinkSessionId,
        request: crate::LinkManagementRequest,
    ) -> Result<crate::LinkManagementResult, LinkError> {
        #[cfg(feature = "fake-device")]
        {
            let (device, observed_lines) = {
                let sessions = self.sessions.borrow();
                let state = sessions
                    .get(session_id)
                    .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
                (state.device.clone(), state.observed_lines.clone())
            };
            if let Some(device) = device {
                if let Some(message) = device.take_manage_failure() {
                    return Err(LinkError::other(message));
                }
                let latency = device.manage_latency();
                if !latency.is_zero() {
                    std::thread::sleep(latency);
                }
                // The transition reboots the device: discard boot output
                // observed before it (the browser provider clears buffered
                // input on reset the same way), so the next readiness pass
                // classifies the NEW state's boot, not stale lines.
                if let Some(lines) = &observed_lines {
                    lines
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .clear();
                }
                return manage_fake_device(&device, request);
            }
        }
        let _ = session_id;
        Err(LinkError::unsupported(format!("{:?}", request.operation())))
    }

    async fn close(&self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        // Mark the session closed and take the transport out of the state
        // BEFORE awaiting the transport close: no internal borrow may span
        // the await.
        #[cfg(feature = "fake-device")]
        let server_connection;
        {
            let mut sessions = self.sessions.borrow_mut();
            let state = sessions
                .get_mut(session_id)
                .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
            state.session.status = LinkSessionStatus::Closed;
            #[cfg(feature = "fake-device")]
            {
                server_connection = state.server_connection.take();
            }
        }
        #[cfg(feature = "fake-device")]
        if let Some(server_connection) = server_connection {
            let mut transport = server_connection.lock().await;
            lpa_client::ClientTransport::close(&mut **transport)
                .await
                .map_err(|error| LinkError::other(error.to_string()))?;
        }
        let mut sessions = self.sessions.borrow_mut();
        let state = sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        state.logs.push(LinkLogEntry::new(
            state.endpoint_id.clone(),
            Some(state.session.id.clone()),
            LinkLogLevel::Info,
            "fake link session closed",
        ));
        Ok(())
    }
}

/// Execute one scripted management transition on the fake device. Scripted
/// failure and latency were already consumed by the caller.
#[cfg(feature = "fake-device")]
fn manage_fake_device(
    device: &crate::providers::fake_device::FakeEsp32Device,
    request: crate::LinkManagementRequest,
) -> Result<crate::LinkManagementResult, LinkError> {
    use crate::providers::fake_device::FAKE_IMAGE_IDENTITY;
    use crate::{
        LinkEraseDeviceResult, LinkFirmwareFlashResult, LinkFirmwareManifest,
        LinkManagementProgress, LinkManagementRequest, LinkManagementResult,
    };

    match request {
        LinkManagementRequest::ResetRuntime => {
            device.reset_runtime();
            Ok(LinkManagementResult::ResetRuntime)
        }
        LinkManagementRequest::FlashFirmware => {
            device.fake_flash(FAKE_IMAGE_IDENTITY);
            Ok(LinkManagementResult::FlashFirmware(
                LinkFirmwareFlashResult {
                    manifest: LinkFirmwareManifest {
                        firmware_id: FAKE_IMAGE_IDENTITY.to_string(),
                        display_name: "Fake LightPlayer firmware".to_string(),
                        target_chip: "esp32c6".to_string(),
                        image_count: 1,
                        total_bytes: 0,
                        manifest_path: None,
                    },
                    chip_name: Some("ESP32-C6 (fake)".to_string()),
                    logs: vec!["fake flash: scripted transition to LightPlayer".to_string()],
                    progress: vec![
                        LinkManagementProgress::new("Writing firmware").with_percent(50),
                        LinkManagementProgress::new("Firmware written").with_percent(100),
                    ],
                },
            ))
        }
        LinkManagementRequest::EraseDeviceFlash => {
            device.fake_erase();
            Ok(LinkManagementResult::EraseDeviceFlash(
                LinkEraseDeviceResult {
                    chip_name: Some("ESP32-C6 (fake)".to_string()),
                    logs: vec!["fake erase: scripted transition to blank flash".to_string()],
                    progress: vec![LinkManagementProgress::new("Erasing flash").with_percent(100)],
                },
            ))
        }
        LinkManagementRequest::EraseRawFilesystem => {
            Err(LinkError::unsupported(format!("{:?}", request.operation())))
        }
    }
}

/// Resources built when a device-backed endpoint connects.
#[cfg(feature = "fake-device")]
struct FakeDeviceSessionResources {
    device: crate::providers::fake_device::FakeEsp32Device,
    server_connection: crate::LinkServerConnection,
    lines: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

struct FakeSessionState {
    endpoint_id: LinkEndpointId,
    session: LinkSession,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
    #[cfg(feature = "fake-device")]
    device: Option<crate::providers::fake_device::FakeEsp32Device>,
    #[cfg(feature = "fake-device")]
    server_connection: Option<crate::LinkServerConnection>,
    #[cfg(feature = "fake-device")]
    observed_lines: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
}

impl FakeSessionState {
    fn new(endpoint_id: LinkEndpointId, session: LinkSession) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(session.id.clone()),
            LinkLogLevel::Info,
            "fake link session opened",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(session.id.clone()),
            LinkDiagnosticSeverity::Info,
            "fake link session ready",
        )];
        Self {
            endpoint_id,
            session,
            logs,
            diagnostics,
            #[cfg(feature = "fake-device")]
            device: None,
            #[cfg(feature = "fake-device")]
            server_connection: None,
            #[cfg(feature = "fake-device")]
            observed_lines: None,
        }
    }
}
