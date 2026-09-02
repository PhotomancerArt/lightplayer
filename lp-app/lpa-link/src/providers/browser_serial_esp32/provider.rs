use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use lpa_devices::link::ResetKind;

use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::management_request::LinkManagementRequest;
use crate::provider::management_result::{
    LinkEraseDeviceResult, LinkFirmwareFlashResult, LinkFirmwareManifest, LinkManagementResult,
    LinkRawFilesystemReadResult,
};
use crate::provider::session::LinkSessionId;
use crate::providers::browser_serial_esp32::BrowserSerialEsp32Options;
use crate::providers::browser_serial_esp32::{
    BrowserEsp32EraseResult, BrowserEsp32FirmwareManifest, BrowserEsp32FlashProgress,
    BrowserEsp32FlashResult, BrowserEsp32ProbeResult, browser_esp32_flash, browser_serial,
};
use crate::providers::{LinkProviderDescriptor, LinkProviderKind};
use crate::{
    LinkBootControlResult, LinkCapabilities, LinkConnection, LinkConnectionKind, LinkDiagnostic,
    LinkDiagnosticSeverity, LinkEndpoint, LinkError, LinkLogEntry, LinkLogLevel,
    LinkManagementEventSink, LinkManagementProgress, LinkProvider, LinkSession, LinkSessionStatus,
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
        self.create_granted_endpoint_with_usb(label, port_id, None)
    }

    /// [`Self::create_granted_endpoint`], recording the port's USB
    /// vendor:product pair when the browser exposed one — what lets a
    /// grant be matched against a board's declared `usb_bridge` (D7).
    pub fn create_granted_endpoint_with_usb(
        &self,
        label: impl Into<String>,
        port_id: u32,
        usb_vid_pid: Option<(u16, u16)>,
    ) -> LinkEndpointId {
        let endpoint_index = self.next_endpoint_index.get();
        self.next_endpoint_index.set(endpoint_index + 1);
        let endpoint_id =
            LinkEndpointId::new(format!("{}-port-{}", self.kind().key(), endpoint_index));

        let mut capabilities = LinkCapabilities::esp32_serial_base();
        if self.is_flash_supported() {
            capabilities = capabilities
                .with_flash()
                .with_device_erase()
                .with_boot_control()
                // READ only (M6): restore is M7's, and advertising the write
                // half early would surface a button that answers `unsupported`.
                .with_raw_filesystem_read();
        }
        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.kind(), label)
            .with_capabilities(capabilities);
        self.endpoints.borrow_mut().insert(
            endpoint_id.clone(),
            BrowserSerialEndpointState {
                endpoint,
                port_id,
                usb_vid_pid,
            },
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
        browser_serial::granted_ports()
            .await
            .is_ok_and(|ports| !ports.is_empty())
    }

    /// Mint endpoints for every port this origin was ALREADY granted
    /// (`navigator.serial.getPorts()`) — no chooser is shown, and `.open()`
    /// on a granted port needs no user gesture. Ports that already carry an
    /// endpoint keep it; new grants get one via
    /// [`Self::create_granted_endpoint`]. This is the one-click reconnect
    /// path (M1): which physical device a grant belongs to is unknowable
    /// pre-connect, so callers connect first and reconcile identity from
    /// the hello.
    pub async fn discover_granted_endpoints(&self) -> Result<Vec<LinkEndpoint>, LinkError> {
        Ok(self
            .discover_granted_endpoints_with_usb()
            .await?
            .into_iter()
            .map(|granted| granted.endpoint)
            .collect())
    }

    /// [`Self::discover_granted_endpoints`], keeping each grant's USB
    /// vendor:product pair alongside its endpoint — the D7 grant-aware
    /// picker's enumeration (a board's `usb_bridge` is matched against
    /// these pairs, never against label prose).
    pub async fn discover_granted_endpoints_with_usb(
        &self,
    ) -> Result<Vec<GrantedSerialEndpoint>, LinkError> {
        let ports = browser_serial::granted_ports().await?;
        let mut endpoints = Vec::with_capacity(ports.len());
        for port in ports {
            let usb_vid_pid = port.usb_vid_pid();
            let endpoint_id = match self.endpoint_id_for_port(port.id) {
                Some(endpoint_id) => endpoint_id,
                None => self.create_granted_endpoint_with_usb(port.label, port.id, usb_vid_pid),
            };
            endpoints.push(GrantedSerialEndpoint {
                endpoint: self.endpoint(&endpoint_id)?,
                usb_vid_pid: self.endpoint_state(&endpoint_id)?.usb_vid_pid,
            });
        }
        Ok(endpoints)
    }

    pub fn is_flash_supported(&self) -> bool {
        browser_esp32_flash::is_supported()
    }

    pub async fn request_access(&self) -> Result<LinkEndpoint, LinkError> {
        let port = browser_serial::request_port().await?;
        // Re-picking an already-granted port resolves to ITS endpoint
        // (the multi-board L1 defect): the JS layer already returns the
        // existing session for a known SerialPort, and a second endpoint
        // over the same port would put two Rust sessions on one
        // reader/writer. With the pool's one-session-per-endpoint rule,
        // re-picking a connected port now REPLACES its session cleanly.
        let usb_vid_pid = port.usb_vid_pid();
        let endpoint_id = match self.endpoint_id_for_port(port.id) {
            Some(endpoint_id) => endpoint_id,
            None => self.create_granted_endpoint_with_usb(port.label, port.id, usb_vid_pid),
        };
        self.endpoint(&endpoint_id)
    }

    /// `reset: None` opens the port WITHOUT any reset. Identify relies on
    /// it: a USB-Serial-JTAG chip (C6) re-enumerates on hard reset, which
    /// kills the port that was just opened — the model's mid-stream hello
    /// request needs no reset at all (G1 finding, 2026-08-31: identify
    /// wedged at "Not responding" on every C6 open).
    ///
    /// `Some(kind)` runs that reset sequence as part of the open; see
    /// `browser_serial::reset_kind_js_name` for what each kind drives.
    pub async fn open_protocol(
        &self,
        session_id: &LinkSessionId,
        baud_rate: u32,
        reset: Option<ResetKind>,
    ) -> Result<(), LinkError> {
        let (endpoint_id, port_id) = self.session_endpoint_and_port(session_id)?;
        let result = browser_serial::open(port_id, baud_rate, reset).await?;
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

    /// Read a packaged build's manifest without touching the device.
    pub async fn load_firmware_manifest(
        &self,
        build_id: Option<&str>,
    ) -> Result<BrowserEsp32FirmwareManifest, LinkError> {
        let build_id = require_build_id(build_id)?;
        browser_esp32_flash::load_manifest(&self.options.firmware_manifest_path(build_id)).await
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
        build_id: Option<&str>,
    ) -> Result<BrowserEsp32FlashResult, LinkError> {
        self.flash_firmware_with_events(endpoint_id, build_id, LinkManagementEventSink::noop())
            .await
    }

    pub async fn flash_firmware_with_events(
        &self,
        endpoint_id: &LinkEndpointId,
        build_id: Option<&str>,
        events: LinkManagementEventSink,
    ) -> Result<BrowserEsp32FlashResult, LinkError> {
        let build_id = require_build_id(build_id)?;
        let port_id = self.endpoint_port_id(endpoint_id)?;
        browser_esp32_flash::flash_firmware_with_events(
            port_id,
            &self.options.firmware_manifest_path(build_id),
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
            LinkManagementRequest::FlashFirmware { ref build_id } => {
                let result = self
                    .flash_firmware_with_events(&endpoint_id, build_id.as_deref(), events.clone())
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
                let result = browser_serial::reset_and_read(
                    port_id,
                    RESET_BAUD_RATE,
                    RESET_READ_WINDOW_MS,
                    // The management verb has always meant the plain
                    // hard reset; kind selection belongs to the link
                    // adapter, which asks for it per board.
                    ResetKind::Normal,
                )
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
            LinkManagementRequest::SetBootControl { flags } => {
                let port_id = self.endpoint_port_id(&endpoint_id)?;
                let result = browser_esp32_flash::write_boot_control_with_events(
                    port_id,
                    self.options.esptool_module_path(),
                    lp_bootctl::BootFlags::from_bits(flags),
                    events.clone(),
                )
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
                Ok(LinkManagementResult::SetBootControl(
                    LinkBootControlResult {
                        flags,
                        chip_name: result.chip_name,
                        logs: result.logs,
                        progress: map_progress(result.progress),
                    },
                ))
            }
            LinkManagementRequest::ReadRawFilesystem => {
                let result = browser_esp32_flash::read_raw_filesystem_with_events(
                    port_id,
                    self.options.esptool_module_path(),
                    events.clone(),
                )
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
                Ok(LinkManagementResult::ReadRawFilesystem(
                    LinkRawFilesystemReadResult {
                        image: result.image,
                        region: result.region,
                        chip_name: result.chip_name,
                        logs: result.logs,
                        progress: map_progress(result.progress),
                    },
                ))
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

    fn endpoint_id_for_port(&self, port_id: u32) -> Option<LinkEndpointId> {
        self.endpoints
            .borrow()
            .values()
            .find(|state| state.port_id == port_id)
            .map(|state| state.endpoint.id.clone())
    }

    /// Write `bytes` to `path` on the device over the app protocol, on the
    /// raw line framing (round 2's coarse-effect seam; first consumer is
    /// the flash activity's `/hardware.json` stamp, D4).
    ///
    /// ⚠️ The caller owns the exclusive-borrow discipline: the model's link
    /// pump for this endpoint must be paused while this runs, or the two
    /// drainers split the frames between them.
    pub async fn write_device_file(
        &self,
        endpoint_id: &LinkEndpointId,
        path: &str,
        bytes: &[u8],
        events: LinkManagementEventSink,
    ) -> Result<(), LinkError> {
        let port_id = self.endpoint_port_id(endpoint_id)?;
        super::port_client_io::write_device_file(port_id, path, bytes, events).await
    }

    /// Push a project onto the device over the app protocol (round 2's
    /// second coarse effect): find the storage dir the board runs from,
    /// replace it, load it, and verify the package hash.
    ///
    /// ⚠️ Same exclusive-borrow rule as [`Self::write_device_file`]: the
    /// model's link pump for this endpoint must be paused while this runs.
    pub async fn push_device_project(
        &self,
        endpoint_id: &LinkEndpointId,
        files: &[(String, Vec<u8>)],
        expected_hash: &str,
        fallback_storage_id: &str,
        events: LinkManagementEventSink,
    ) -> Result<lpa_client::PushReport, LinkError> {
        let port_id = self.endpoint_port_id(endpoint_id)?;
        super::port_client_io::push_device_project(
            port_id,
            files,
            expected_hash,
            fallback_storage_id,
            events,
        )
        .await
    }

    /// A long-lived `lpa-client` io over an endpoint's open port for the
    /// editor lens (round-2 M5), with every drained line teed to `tap`.
    ///
    /// ⚠️ Same exclusive-borrow rule as [`Self::write_device_file`], held
    /// for the lens's lifetime: the model's link pump for this endpoint must
    /// stay paused until the lens gives the wire back.
    pub fn lens_client_io(
        &self,
        endpoint_id: &LinkEndpointId,
        tap: std::rc::Rc<dyn Fn(String)>,
        events: LinkManagementEventSink,
    ) -> Result<Box<dyn lpa_client::ClientIo>, LinkError> {
        let port_id = self.endpoint_port_id(endpoint_id)?;
        Ok(super::port_client_io::lens_client_io(port_id, tap, events))
    }

    /// Take the loaded project off the device over the app protocol (the
    /// card's "Remove project"): ask what it runs from, stop it, delete that
    /// dir. The firmware is untouched, so the board comes back on the empty
    /// face rather than needing a re-flash.
    ///
    /// ⚠️ Same exclusive-borrow rule as [`Self::write_device_file`]: the
    /// model's link pump for this endpoint must be paused while this runs.
    pub async fn remove_device_project(
        &self,
        endpoint_id: &LinkEndpointId,
        fallback_storage_id: &str,
        events: LinkManagementEventSink,
    ) -> Result<lpa_client::RemoveReport, LinkError> {
        let port_id = self.endpoint_port_id(endpoint_id)?;
        super::port_client_io::remove_device_project(port_id, fallback_storage_id, events).await
    }

    /// Session-scoped [`Self::probe_target`], for the connector's
    /// mode-detection escalation. Releases the app-protocol port first: the
    /// SYNC handshake needs the wire to itself and reboots the device.
    pub async fn probe_target_for_session(
        &self,
        session_id: &LinkSessionId,
    ) -> Result<BrowserEsp32ProbeResult, LinkError> {
        self.release_protocol_if_open(session_id).await?;
        let port_id = self.session_port_id(session_id)?;
        browser_esp32_flash::probe_target(port_id, self.options.esptool_module_path()).await
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

    /// Revoke the endpoint's Web Serial grant and forget the endpoint.
    ///
    /// This is the ONE place a grant dies. Without it, "forget this
    /// device" was undone by the next page load: the grant survived, the
    /// auto-connect sweep re-enumerated the port, and the silicon-anchored
    /// identity re-derived the same `dev` uid, so the sighting write
    /// recreated the registry row the user had just deleted.
    ///
    /// Endpoint state goes first so a failed `forget()` cannot leave this
    /// provider handing out an endpoint over a port it no longer trusts;
    /// re-granting mints a fresh endpoint through `request_access`.
    /// `Ok(false)` = the grant survives (browser without `forget()`).
    async fn forget_endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<bool, LinkError> {
        let port_id = self.endpoint_port_id(endpoint_id)?;
        self.endpoints.borrow_mut().remove(endpoint_id);
        self.sessions
            .borrow_mut()
            .retain(|_, state| state.port_id != port_id);
        browser_serial::forget(port_id).await
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

/// A flash request must NAME its image. There is no fallback build
/// (Yona, 2026-08-03: "there shouldn't be a fallback for firmware.
/// either it matches, or its a fail case") — the deployment default
/// silently aimed a classic ESP32 at the C6 image and left the
/// flash-time chip guard to refuse a build nobody had chosen.
fn require_build_id(build_id: Option<&str>) -> Result<&str, LinkError> {
    build_id.ok_or_else(|| {
        LinkError::other(
            "no firmware image matches this device: pick your board in the setup form, \
             and if it is not listed, this Studio build ships no image for that chip",
        )
    })
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
        probed_mac: result.base_mac,
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

/// One granted Web Serial port: its endpoint plus the USB identity the
/// browser exposed for it (`getInfo()` — VID:PID only, which is why two
/// identical bridge chips stay indistinguishable until opened).
#[derive(Clone, Debug)]
pub struct GrantedSerialEndpoint {
    pub endpoint: LinkEndpoint,
    pub usb_vid_pid: Option<(u16, u16)>,
}

#[derive(Clone, Debug)]
struct BrowserSerialEndpointState {
    endpoint: LinkEndpoint,
    port_id: u32,
    usb_vid_pid: Option<(u16, u16)>,
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
