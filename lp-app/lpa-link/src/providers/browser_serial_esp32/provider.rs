use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::management_request::LinkManagementRequest;
use crate::provider::management_result::{
    LinkEraseDeviceResult, LinkFirmwareFlashResult, LinkFirmwareManifest, LinkManagementResult,
};
use crate::provider::session::LinkSessionId;
use crate::providers::browser_serial_esp32::BrowserSerialEsp32Options;
use crate::providers::browser_serial_esp32::{
    BrowserEsp32EraseResult, BrowserEsp32FirmwareManifest, BrowserEsp32FlashProgress,
    BrowserEsp32FlashResult, BrowserEsp32ProbeResult, browser_esp32_flash, browser_serial,
};
use crate::providers::{LinkProviderDescriptor, LinkProviderKind};
use crate::{
    LinkCapabilities, LinkConnection, LinkConnectionKind, LinkDiagnostic, LinkDiagnosticSeverity,
    LinkEndpoint, LinkError, LinkLogEntry, LinkLogLevel, LinkManagementEventSink,
    LinkManagementProgress, LinkProvider, LinkSession, LinkSessionStatus,
};

const RESET_BAUD_RATE: u32 = 115_200;
const RESET_READ_WINDOW_MS: u32 = 1_500;

pub fn descriptor() -> LinkProviderDescriptor {
    LinkProviderKind::BrowserSerialEsp32.descriptor()
}

/// Browser Web Serial ESP32 provider.
///
/// Endpoint and session state live behind internal `RefCell`s. Every JS
/// future (`browser_serial::*`, `browser_esp32_flash::*`) is awaited with the
/// needed values (`port_id`, ids) copied OUT of the borrow first — no
/// internal borrow spans an await.
pub struct BrowserSerialEsp32Provider {
    endpoints: RefCell<BTreeMap<LinkEndpointId, BrowserSerialEndpointState>>,
    sessions: RefCell<BTreeMap<LinkSessionId, BrowserSerialSessionState>>,
    options: BrowserSerialEsp32Options,
    next_endpoint_index: Cell<u64>,
    next_session_index: Cell<u64>,
}

impl BrowserSerialEsp32Provider {
    pub fn new() -> Self {
        Self::with_options(BrowserSerialEsp32Options::default())
    }

    pub fn with_options(options: BrowserSerialEsp32Options) -> Self {
        Self {
            endpoints: RefCell::new(BTreeMap::new()),
            sessions: RefCell::new(BTreeMap::new()),
            options,
            next_endpoint_index: Cell::new(1),
            next_session_index: Cell::new(1),
        }
    }

    pub fn options(&self) -> &BrowserSerialEsp32Options {
        &self.options
    }

    pub fn create_granted_endpoint(
        &self,
        label: impl Into<String>,
        port_id: u32,
    ) -> LinkEndpointId {
        let endpoint_index = self.next_endpoint_index.get();
        self.next_endpoint_index.set(endpoint_index + 1);
        let endpoint_id =
            LinkEndpointId::new(format!("{}-port-{}", self.kind().key(), endpoint_index));

        let mut capabilities = LinkCapabilities::esp32_serial_base();
        if self.is_flash_supported() {
            capabilities = capabilities.with_flash().with_device_erase();
        }
        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.kind(), label)
            .with_capabilities(capabilities);
        self.endpoints.borrow_mut().insert(
            endpoint_id.clone(),
            BrowserSerialEndpointState { endpoint, port_id },
        );
        endpoint_id
    }

    pub fn is_serial_supported(&self) -> bool {
        browser_serial::is_supported()
    }

    /// Whether this origin already holds at least one granted Web Serial
    /// port (`navigator.serial.getPorts()` — no permission prompt). This is
    /// catalog-level metadata: it answers "has a device ever been granted
    /// here?" without opening anything.
    pub async fn granted_ports_available() -> bool {
        browser_serial::granted_ports_count().await > 0
    }

    pub fn is_flash_supported(&self) -> bool {
        browser_esp32_flash::is_supported()
    }

    pub async fn request_access(&self) -> Result<LinkEndpoint, LinkError> {
        let port = browser_serial::request_port().await?;
        let endpoint_id = self.create_granted_endpoint(port.label, port.id);
        self.endpoint(&endpoint_id)
    }

    pub async fn open_protocol(
        &self,
        session_id: &LinkSessionId,
        baud_rate: u32,
    ) -> Result<(), LinkError> {
        let (endpoint_id, port_id) = self.session_endpoint_and_port(session_id)?;
        let result = browser_serial::open(port_id, baud_rate).await?;
        let logs = protocol_open_result_logs(endpoint_id, session_id.clone(), result);
        let mut sessions = self.sessions.borrow_mut();
        let state = session_state_mut(&mut sessions, session_id)?;
        state.logs.extend(logs);
        state.protocol_open = true;
        Ok(())
    }

    pub async fn write_line(
        &self,
        session_id: &LinkSessionId,
        line: &str,
    ) -> Result<(), LinkError> {
        let port_id = self.session_port_id(session_id)?;
        browser_serial::write_line(port_id, line).await
    }

    pub fn take_lines(&self, session_id: &LinkSessionId) -> Result<Vec<String>, LinkError> {
        let port_id = self.session_port_id(session_id)?;
        Ok(browser_serial::take_lines(port_id))
    }

    pub fn take_errors(&self, session_id: &LinkSessionId) -> Result<Vec<String>, LinkError> {
        let port_id = self.session_port_id(session_id)?;
        Ok(browser_serial::take_errors(port_id))
    }

    pub async fn release_protocol(&self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        let port_id = self.session_port_id(session_id)?;
        browser_serial::release(port_id).await?;
        let mut sessions = self.sessions.borrow_mut();
        let state = session_state_mut(&mut sessions, session_id)?;
        state.protocol_open = false;
        Ok(())
    }

    pub async fn release_session_for_management(
        &self,
        session_id: &LinkSessionId,
    ) -> Result<(), LinkError> {
        self.release_protocol(session_id).await?;
        self.sessions.borrow_mut().remove(session_id);
        Ok(())
    }

    pub async fn load_firmware_manifest(&self) -> Result<BrowserEsp32FirmwareManifest, LinkError> {
        browser_esp32_flash::load_manifest(&self.options.firmware_manifest_path).await
    }

    pub async fn probe_target(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<BrowserEsp32ProbeResult, LinkError> {
        let port_id = self.endpoint_port_id(endpoint_id)?;
        browser_esp32_flash::probe_target(port_id, self.options.esptool_module_path()).await
    }

    pub async fn flash_firmware(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<BrowserEsp32FlashResult, LinkError> {
        self.flash_firmware_with_events(endpoint_id, LinkManagementEventSink::noop())
            .await
    }

    pub async fn flash_firmware_with_events(
        &self,
        endpoint_id: &LinkEndpointId,
        events: LinkManagementEventSink,
    ) -> Result<BrowserEsp32FlashResult, LinkError> {
        let port_id = self.endpoint_port_id(endpoint_id)?;
        browser_esp32_flash::flash_firmware_with_events(
            port_id,
            &self.options.firmware_manifest_path,
            self.options.esptool_module_path(),
            events,
        )
        .await
    }

    pub async fn erase_device_flash(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<BrowserEsp32EraseResult, LinkError> {
        self.erase_device_flash_with_events(endpoint_id, LinkManagementEventSink::noop())
            .await
    }

    pub async fn erase_device_flash_with_events(
        &self,
        endpoint_id: &LinkEndpointId,
        events: LinkManagementEventSink,
    ) -> Result<BrowserEsp32EraseResult, LinkError> {
        let port_id = self.endpoint_port_id(endpoint_id)?;
        browser_esp32_flash::erase_device_flash_with_events(
            port_id,
            self.options.esptool_module_path(),
            events,
        )
        .await
    }

    async fn manage_inner(
        &self,
        session_id: &LinkSessionId,
        request: LinkManagementRequest,
        events: LinkManagementEventSink,
    ) -> Result<LinkManagementResult, LinkError> {
        self.session_capabilities_support(session_id, &request)?;
        let (endpoint_id, port_id) = self.session_endpoint_and_port(session_id)?;
        self.release_protocol_if_open(session_id).await?;
        match request {
            LinkManagementRequest::FlashFirmware => {
                let result = self
                    .flash_firmware_with_events(&endpoint_id, events.clone())
                    .await?;
                let logs = result
                    .logs
                    .iter()
                    .map(|message| {
                        LinkLogEntry::new(
                            endpoint_id.clone(),
                            Some(session_id.clone()),
                            LinkLogLevel::Info,
                            message.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                self.extend_session_logs(session_id, logs)?;
                Ok(LinkManagementResult::FlashFirmware(
                    map_firmware_flash_result(result),
                ))
            }
            LinkManagementRequest::EraseDeviceFlash => {
                let result = self
                    .erase_device_flash_with_events(&endpoint_id, events.clone())
                    .await?;
                let logs = result
                    .logs
                    .iter()
                    .map(|message| {
                        LinkLogEntry::new(
                            endpoint_id.clone(),
                            Some(session_id.clone()),
                            LinkLogLevel::Info,
                            message.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                self.extend_session_logs(session_id, logs)?;
                Ok(LinkManagementResult::EraseDeviceFlash(
                    map_erase_device_result(result),
                ))
            }
            LinkManagementRequest::ResetRuntime => {
                events.emit(crate::LinkManagementEvent::log("Resetting device"));
                let result =
                    browser_serial::reset_and_read(port_id, RESET_BAUD_RATE, RESET_READ_WINDOW_MS)
                        .await?;
                for message in &result.logs {
                    events.emit(crate::LinkManagementEvent::log(message.clone()));
                }
                let logs = result
                    .logs
                    .iter()
                    .map(|message| {
                        LinkLogEntry::new(
                            endpoint_id.clone(),
                            Some(session_id.clone()),
                            LinkLogLevel::Info,
                            message.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                self.extend_session_logs(session_id, logs)?;
                Ok(LinkManagementResult::ResetRuntime)
            }
            LinkManagementRequest::EraseRawFilesystem => {
                Err(LinkError::unsupported(format!("{:?}", request.operation())))
            }
        }
    }

    async fn release_protocol_if_open(&self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        let protocol_open = {
            let sessions = self.sessions.borrow();
            session_state(&sessions, session_id)?.protocol_open
        };
        if protocol_open {
            self.release_protocol(session_id).await?;
        }
        Ok(())
    }

    fn session_capabilities_support(
        &self,
        session_id: &LinkSessionId,
        request: &LinkManagementRequest,
    ) -> Result<(), LinkError> {
        let sessions = self.sessions.borrow();
        let session = &session_state(&sessions, session_id)?.session;
        let operation = request.operation();
        if session.capabilities.supports(operation) {
            Ok(())
        } else {
            Err(LinkError::unsupported(format!("{operation:?}")))
        }
    }

    fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpoint, LinkError> {
        Ok(self.endpoint_state(endpoint_id)?.endpoint)
    }

    fn endpoint_state(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<BrowserSerialEndpointState, LinkError> {
        self.endpoints
            .borrow()
            .get(endpoint_id)
            .cloned()
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }

    fn endpoint_port_id(&self, endpoint_id: &LinkEndpointId) -> Result<u32, LinkError> {
        Ok(self.endpoint_state(endpoint_id)?.port_id)
    }

    fn session_port_id(&self, session_id: &LinkSessionId) -> Result<u32, LinkError> {
        let sessions = self.sessions.borrow();
        Ok(session_state(&sessions, session_id)?.port_id)
    }

    fn session_endpoint_and_port(
        &self,
        session_id: &LinkSessionId,
    ) -> Result<(LinkEndpointId, u32), LinkError> {
        let sessions = self.sessions.borrow();
        let state = session_state(&sessions, session_id)?;
        Ok((state.session.endpoint_id.clone(), state.port_id))
    }

    fn extend_session_logs(
        &self,
        session_id: &LinkSessionId,
        logs: Vec<LinkLogEntry>,
    ) -> Result<(), LinkError> {
        let mut sessions = self.sessions.borrow_mut();
        session_state_mut(&mut sessions, session_id)?
            .logs
            .extend(logs);
        Ok(())
    }
}

impl LinkProvider for BrowserSerialEsp32Provider {
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::BrowserSerialEsp32
    }

    async fn discover(&self) -> Result<Vec<LinkEndpoint>, LinkError> {
        Ok(self
            .endpoints
            .borrow()
            .values()
            .map(|state| state.endpoint.clone())
            .collect())
    }

    async fn status(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpointStatus, LinkError> {
        Ok(self.endpoint(endpoint_id)?.status)
    }

    async fn connect(&self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError> {
        let endpoint_state = self.endpoint_state(endpoint_id)?;
        let session_index = self.next_session_index.get();
        self.next_session_index.set(session_index + 1);
        let session_id = LinkSessionId::new(format!("{}:{}", endpoint_id.as_str(), session_index));
        let session = LinkSession::new(
            session_id.clone(),
            self.kind(),
            endpoint_state.endpoint.id.clone(),
            LinkConnectionKind::BrowserSerialEsp32 {
                protocol: "lp-serial-json-lines-v1".to_string(),
            },
            endpoint_state.endpoint.capabilities.clone(),
        );
        self.sessions.borrow_mut().insert(
            session_id,
            BrowserSerialSessionState::new(session.clone(), endpoint_state.port_id),
        );
        Ok(session)
    }

    async fn connection(&self, session_id: &LinkSessionId) -> Result<LinkConnection, LinkError> {
        let sessions = self.sessions.borrow();
        let state = session_state(&sessions, session_id)?;
        if state.session.status == LinkSessionStatus::Closed {
            return Err(LinkError::Closed);
        }
        Ok(LinkConnection::browser_serial_esp32(
            state.session.endpoint_id.clone(),
            state.session.id.clone(),
        ))
    }

    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError> {
        let sessions = self.sessions.borrow();
        Ok(session_state(&sessions, session_id)?.logs.clone())
    }

    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError> {
        let sessions = self.sessions.borrow();
        Ok(session_state(&sessions, session_id)?.diagnostics.clone())
    }

    async fn manage(
        &self,
        session_id: &LinkSessionId,
        request: LinkManagementRequest,
    ) -> Result<LinkManagementResult, LinkError> {
        self.manage_inner(session_id, request, LinkManagementEventSink::noop())
            .await
    }

    async fn manage_with_events(
        &self,
        session_id: &LinkSessionId,
        request: LinkManagementRequest,
        events: LinkManagementEventSink,
    ) -> Result<LinkManagementResult, LinkError> {
        self.manage_inner(session_id, request, events).await
    }

    async fn close(&self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        // Mark the session closed and copy the port id out BEFORE awaiting
        // the JS close: no internal borrow may span the await.
        let port_id = {
            let mut sessions = self.sessions.borrow_mut();
            let state = session_state_mut(&mut sessions, session_id)?;
            if state.session.status == LinkSessionStatus::Closed {
                return Ok(());
            }
            state.session.status = LinkSessionStatus::Closed;
            state.port_id
        };
        browser_serial::close(port_id).await?;
        let mut sessions = self.sessions.borrow_mut();
        let state = session_state_mut(&mut sessions, session_id)?;
        state.protocol_open = false;
        state.logs.push(LinkLogEntry::new(
            state.session.endpoint_id.clone(),
            Some(state.session.id.clone()),
            LinkLogLevel::Info,
            "browser serial ESP32 session closed",
        ));
        Ok(())
    }
}

fn session_state<'a>(
    sessions: &'a BTreeMap<LinkSessionId, BrowserSerialSessionState>,
    session_id: &LinkSessionId,
) -> Result<&'a BrowserSerialSessionState, LinkError> {
    sessions
        .get(session_id)
        .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
}

fn session_state_mut<'a>(
    sessions: &'a mut BTreeMap<LinkSessionId, BrowserSerialSessionState>,
    session_id: &LinkSessionId,
) -> Result<&'a mut BrowserSerialSessionState, LinkError> {
    sessions
        .get_mut(session_id)
        .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
}

fn map_firmware_flash_result(result: BrowserEsp32FlashResult) -> LinkFirmwareFlashResult {
    LinkFirmwareFlashResult {
        manifest: LinkFirmwareManifest {
            firmware_id: result.manifest.firmware_id,
            display_name: result.manifest.display_name,
            target_chip: result.manifest.target_chip,
            image_count: result.manifest.image_count,
            total_bytes: result.manifest.total_bytes,
            manifest_path: result.manifest.manifest_path,
        },
        chip_name: result.chip_name,
        logs: result.logs,
        progress: map_progress(result.progress),
    }
}

fn map_erase_device_result(result: BrowserEsp32EraseResult) -> LinkEraseDeviceResult {
    LinkEraseDeviceResult {
        chip_name: result.chip_name,
        logs: result.logs,
        progress: map_progress(result.progress),
    }
}

fn protocol_open_result_logs(
    endpoint_id: LinkEndpointId,
    session_id: LinkSessionId,
    result: browser_serial::BrowserSerialProtocolOpenResult,
) -> Vec<LinkLogEntry> {
    let mut logs = result
        .logs
        .into_iter()
        .map(|message| {
            LinkLogEntry::new(
                endpoint_id.clone(),
                Some(session_id.clone()),
                LinkLogLevel::Info,
                message,
            )
        })
        .collect::<Vec<_>>();
    logs.extend(result.progress.into_iter().map(|progress| {
        LinkLogEntry::new(
            endpoint_id.clone(),
            Some(session_id.clone()),
            LinkLogLevel::Info,
            progress.label,
        )
    }));
    logs
}

fn map_progress(progress: Vec<BrowserEsp32FlashProgress>) -> Vec<LinkManagementProgress> {
    progress
        .into_iter()
        .map(|entry| LinkManagementProgress {
            label: entry.label,
            completed_steps: entry.completed_steps,
            total_steps: entry.total_steps,
            percent: entry.percent,
        })
        .collect()
}

#[derive(Clone, Debug)]
struct BrowserSerialEndpointState {
    endpoint: LinkEndpoint,
    port_id: u32,
}

#[derive(Clone, Debug)]
struct BrowserSerialSessionState {
    session: LinkSession,
    port_id: u32,
    protocol_open: bool,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl BrowserSerialSessionState {
    fn new(session: LinkSession, port_id: u32) -> Self {
        let logs = vec![LinkLogEntry::new(
            session.endpoint_id.clone(),
            Some(session.id.clone()),
            LinkLogLevel::Info,
            "browser serial ESP32 session created",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            session.endpoint_id.clone(),
            Some(session.id.clone()),
            LinkDiagnosticSeverity::Info,
            "browser serial session owns Web Serial resources in lpa-link",
        )];
        Self {
            session,
            port_id,
            protocol_open: false,
            logs,
            diagnostics,
        }
    }
}
