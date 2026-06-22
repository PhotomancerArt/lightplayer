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
    LinkEndpoint, LinkError, LinkLogEntry, LinkLogLevel, LinkManagementProgress, LinkProvider,
    LinkSession, LinkSessionStatus,
};

pub fn descriptor() -> LinkProviderDescriptor {
    LinkProviderKind::BrowserSerialEsp32.descriptor()
}

pub struct BrowserSerialEsp32Provider {
    endpoints: BTreeMap<LinkEndpointId, BrowserSerialEndpointState>,
    sessions: BTreeMap<LinkSessionId, BrowserSerialSessionState>,
    options: BrowserSerialEsp32Options,
    next_endpoint_index: u64,
    next_session_index: u64,
}

impl BrowserSerialEsp32Provider {
    pub fn new() -> Self {
        Self::with_options(BrowserSerialEsp32Options::default())
    }

    pub fn with_options(options: BrowserSerialEsp32Options) -> Self {
        Self {
            endpoints: BTreeMap::new(),
            sessions: BTreeMap::new(),
            options,
            next_endpoint_index: 1,
            next_session_index: 1,
        }
    }

    pub fn options(&self) -> &BrowserSerialEsp32Options {
        &self.options
    }

    pub fn create_granted_endpoint(
        &mut self,
        label: impl Into<String>,
        port_id: u32,
    ) -> LinkEndpointId {
        let endpoint_id = LinkEndpointId::new(format!(
            "{}-port-{}",
            self.kind().key(),
            self.next_endpoint_index
        ));
        self.next_endpoint_index += 1;

        let mut capabilities = LinkCapabilities::esp32_serial_base();
        if self.is_flash_supported() {
            capabilities = capabilities.with_flash().with_device_erase();
        }
        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.kind(), label)
            .with_capabilities(capabilities);
        self.endpoints.insert(
            endpoint_id.clone(),
            BrowserSerialEndpointState { endpoint, port_id },
        );
        endpoint_id
    }

    pub fn is_serial_supported(&self) -> bool {
        browser_serial::is_supported()
    }

    pub fn is_flash_supported(&self) -> bool {
        browser_esp32_flash::is_supported()
    }

    pub async fn request_access(&mut self) -> Result<LinkEndpoint, LinkError> {
        let port = browser_serial::request_port().await?;
        let endpoint_id = self.create_granted_endpoint(port.label, port.id);
        Ok(self.endpoint(&endpoint_id)?.clone())
    }

    pub async fn open_protocol(
        &mut self,
        session_id: &LinkSessionId,
        baud_rate: u32,
    ) -> Result<(), LinkError> {
        let state = self.session_mut(session_id)?;
        browser_serial::open(state.port_id, baud_rate).await?;
        state.protocol_open = true;
        Ok(())
    }

    pub async fn write_line(
        &self,
        session_id: &LinkSessionId,
        line: &str,
    ) -> Result<(), LinkError> {
        let state = self.session(session_id)?;
        browser_serial::write_line(state.port_id, line).await
    }

    pub fn take_lines(&self, session_id: &LinkSessionId) -> Result<Vec<String>, LinkError> {
        let state = self.session(session_id)?;
        Ok(browser_serial::take_lines(state.port_id))
    }

    pub fn take_errors(&self, session_id: &LinkSessionId) -> Result<Vec<String>, LinkError> {
        let state = self.session(session_id)?;
        Ok(browser_serial::take_errors(state.port_id))
    }

    pub async fn release_protocol(&mut self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        let state = self.session_mut(session_id)?;
        browser_serial::release(state.port_id).await?;
        state.protocol_open = false;
        Ok(())
    }

    pub async fn release_session_for_management(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<(), LinkError> {
        self.release_protocol(session_id).await?;
        self.sessions.remove(session_id);
        Ok(())
    }

    pub async fn load_firmware_manifest(&self) -> Result<BrowserEsp32FirmwareManifest, LinkError> {
        browser_esp32_flash::load_manifest(&self.options.firmware_manifest_path).await
    }

    pub async fn probe_target(
        &mut self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<BrowserEsp32ProbeResult, LinkError> {
        let port_id = self.endpoint_state(endpoint_id)?.port_id;
        browser_esp32_flash::probe_target(port_id, self.options.esptool_module_path()).await
    }

    pub async fn flash_firmware(
        &mut self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<BrowserEsp32FlashResult, LinkError> {
        let port_id = self.endpoint_state(endpoint_id)?.port_id;
        browser_esp32_flash::flash_firmware(
            port_id,
            &self.options.firmware_manifest_path,
            self.options.esptool_module_path(),
        )
        .await
    }

    pub async fn erase_device_flash(
        &mut self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<BrowserEsp32EraseResult, LinkError> {
        let port_id = self.endpoint_state(endpoint_id)?.port_id;
        browser_esp32_flash::erase_device_flash(port_id, self.options.esptool_module_path()).await
    }

    async fn release_protocol_if_open(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<(), LinkError> {
        if self.session(session_id)?.protocol_open {
            self.release_protocol(session_id).await?;
        }
        Ok(())
    }

    fn session_capabilities_support(
        &self,
        session_id: &LinkSessionId,
        request: &LinkManagementRequest,
    ) -> Result<(), LinkError> {
        let session = &self.session(session_id)?.session;
        let operation = request.operation();
        if session.capabilities.supports(operation) {
            Ok(())
        } else {
            Err(LinkError::unsupported(format!("{operation:?}")))
        }
    }

    fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<&LinkEndpoint, LinkError> {
        Ok(&self.endpoint_state(endpoint_id)?.endpoint)
    }

    fn endpoint_state(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<&BrowserSerialEndpointState, LinkError> {
        self.endpoints
            .get(endpoint_id)
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }

    fn session(&self, session_id: &LinkSessionId) -> Result<&BrowserSerialSessionState, LinkError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }

    fn session_mut(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<&mut BrowserSerialSessionState, LinkError> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }
}

impl LinkProvider for BrowserSerialEsp32Provider {
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::BrowserSerialEsp32
    }

    async fn discover(&mut self) -> Result<Vec<LinkEndpoint>, LinkError> {
        Ok(self
            .endpoints
            .values()
            .map(|state| state.endpoint.clone())
            .collect())
    }

    async fn status(
        &mut self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<LinkEndpointStatus, LinkError> {
        Ok(self.endpoint(endpoint_id)?.status.clone())
    }

    async fn connect(&mut self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError> {
        let endpoint_state = self.endpoint_state(endpoint_id)?.clone();
        let session_id = LinkSessionId::new(format!(
            "{}:{}",
            endpoint_id.as_str(),
            self.next_session_index
        ));
        self.next_session_index += 1;
        let session = LinkSession::new(
            session_id.clone(),
            self.kind(),
            endpoint_state.endpoint.id.clone(),
            LinkConnectionKind::BrowserSerialEsp32 {
                protocol: "lp-serial-json-lines-v1".to_string(),
            },
            endpoint_state.endpoint.capabilities.clone(),
        );
        self.sessions.insert(
            session_id,
            BrowserSerialSessionState::new(session.clone(), endpoint_state.port_id),
        );
        Ok(session)
    }

    async fn connection(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<LinkConnection, LinkError> {
        let state = self.session(session_id)?;
        if state.session.status == LinkSessionStatus::Closed {
            return Err(LinkError::Closed);
        }
        Ok(LinkConnection::browser_serial_esp32(
            state.session.endpoint_id.clone(),
            state.session.id.clone(),
        ))
    }

    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError> {
        Ok(self.session(session_id)?.logs.clone())
    }

    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError> {
        Ok(self.session(session_id)?.diagnostics.clone())
    }

    async fn manage(
        &mut self,
        session_id: &LinkSessionId,
        request: LinkManagementRequest,
    ) -> Result<LinkManagementResult, LinkError> {
        self.session_capabilities_support(session_id, &request)?;
        let endpoint_id = self.session(session_id)?.session.endpoint_id.clone();
        self.release_protocol_if_open(session_id).await?;
        match request {
            LinkManagementRequest::FlashFirmware => {
                let result = self.flash_firmware(&endpoint_id).await?;
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
                self.session_mut(session_id)?.logs.extend(logs);
                Ok(LinkManagementResult::FlashFirmware(
                    map_firmware_flash_result(result),
                ))
            }
            LinkManagementRequest::EraseDeviceFlash => {
                let result = self.erase_device_flash(&endpoint_id).await?;
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
                self.session_mut(session_id)?.logs.extend(logs);
                Ok(LinkManagementResult::EraseDeviceFlash(
                    map_erase_device_result(result),
                ))
            }
            LinkManagementRequest::ResetRuntime | LinkManagementRequest::EraseRawFilesystem => {
                Err(LinkError::unsupported(format!("{:?}", request.operation())))
            }
        }
    }

    async fn close(&mut self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        let state = self.session_mut(session_id)?;
        if state.session.status == LinkSessionStatus::Closed {
            return Ok(());
        }
        state.session.status = LinkSessionStatus::Closed;
        browser_serial::close(state.port_id).await?;
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
