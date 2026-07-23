//! One attached runtime: its payload, wire client, and per-session state.
//!
//! A [`RuntimeSession`] bundles what used to be smeared across two single
//! slots — `DeviceController.attachment` (the runtime payload) and the
//! retired `ServerController` (the wire client + server protocol state) —
//! plus the per-device reconcile bundle that used to live on
//! `StudioController`. The payload keeps D22's rule in the type system
//! (the sim is not a device):
//!
//! - [`RuntimePayload::Sim`] — the browser-worker simulator. Connect +
//!   worker io, no boot, no readiness states, no management plane.
//! - [`RuntimePayload::Device`] — real hardware, owned end to end by an
//!   `lpa_link` [`DeviceSession`] (state machine, hello-first readiness,
//!   management, reconnect-that-rebuilds).
//!
//! There is no `None` arm: absence of a runtime is absence from the
//! [`RuntimePool`](super::RuntimePool).

use std::rc::Rc;

use lpa_link::{DeviceSession, DeviceState, LinkConnection, LinkConnector, LinkSession};

use crate::app::places::DeviceSyncState;
use crate::{
    RuntimeId, ServerFailureKind, ServerState, StudioServerClient, UiError, UiIssue, UiLogDraft,
    UiLogLevel, UxUpdateSink,
};

/// What kind of runtime a session is attached to (D22: the sim is not a
/// device). Derived from the payload; never stored separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeKind {
    /// The browser-worker simulator.
    Sim,
    /// Real hardware behind a [`DeviceSession`].
    Device,
}

/// The runtime a session is attached to.
pub enum RuntimePayload {
    /// The browser-worker simulator (BrowserWorker): a live provider
    /// session whose server io is the worker post-message channel.
    Sim(SimAttachment),
    /// Attached hardware, driven through its [`DeviceSession`].
    Device(DeviceHandle),
}

impl RuntimePayload {
    pub fn kind(&self) -> RuntimeKind {
        match self {
            Self::Sim(_) => RuntimeKind::Sim,
            Self::Device(_) => RuntimeKind::Device,
        }
    }

    /// The wire-level link session record behind this payload.
    pub(crate) fn link_session(&self) -> Option<LinkSession> {
        match self {
            Self::Sim(sim) => Some(sim.session.clone()),
            Self::Device(handle) => handle.session().map(DeviceSession::session),
        }
    }
}

/// The simulator attachment: connector + session + connection handoff.
/// No states — boot-ready IS the session (D22).
pub struct SimAttachment {
    pub connector: Rc<LinkConnector>,
    pub session: LinkSession,
    pub connection: LinkConnection,
}

/// A hardware attachment. In product code this is always a live
/// [`DeviceSession`]; tests that only exercise view/derivation logic can
/// stub the session with a fixed [`DeviceState`] instead of scripting a
/// whole fake device.
pub enum DeviceHandle {
    Session(DeviceSession),
    #[cfg(test)]
    Stub(StubDevice),
}

/// Test-only hardware stand-in: a fixed device state, no wire.
#[cfg(test)]
pub struct StubDevice {
    pub state: DeviceState,
}

impl DeviceHandle {
    /// The session's current device state.
    pub fn state(&self) -> DeviceState {
        match self {
            Self::Session(session) => session.state(),
            #[cfg(test)]
            Self::Stub(stub) => stub.state.clone(),
        }
    }

    /// The live session, when this handle holds one.
    pub fn session(&self) -> Option<&DeviceSession> {
        match self {
            Self::Session(session) => Some(session),
            #[cfg(test)]
            Self::Stub(_) => None,
        }
    }

    /// Close the underlying session (attachment teardown).
    pub async fn close(self) -> Result<(), UiError> {
        match self {
            Self::Session(session) => session
                .close()
                .await
                .map_err(|error| UiError::Link(error.to_string())),
            #[cfg(test)]
            Self::Stub(_) => Ok(()),
        }
    }
}

/// One runtime session in the pool: the attached runtime payload, its wire
/// client (each session owns its OWN [`StudioServerClient`]), the server
/// protocol state, and — for device sessions — the connect-as-pull
/// reconcile bundle.
pub struct RuntimeSession {
    id: RuntimeId,
    payload: RuntimePayload,
    client: Option<StudioServerClient>,
    server_state: ServerState,
    /// The last log level Studio asked this session's server to apply,
    /// shown optimistically in the console's device-level selector (there
    /// is no read-back on the wire). Reset to the device's init default
    /// (`Info`) whenever a connection is (re)established, since a reboot
    /// reverts it.
    requested_log_level: UiLogLevel,
    /// What the attached DEVICE holds, computed by connect-as-pull (D8)
    /// right after the server protocol attaches to hardware.
    device_sync: Option<DeviceSyncState>,
    /// The device copy's and local head's version numbers on the project
    /// line (`ProjectHistory::version_number`), computed alongside
    /// `device_sync` when the pull classifies against a known project —
    /// the roster's "Running vN"/"Push vN" evidence. `(None, None)`
    /// otherwise; only read while `device_sync` holds a `Known` content.
    device_versions: (Option<usize>, Option<usize>),
    /// The project storage dir the attached device actually runs from
    /// (discovered from its loaded project at connect) — pull and push
    /// target it so one dir replaces in place.
    device_storage_id: Option<String>,
}

impl RuntimeSession {
    /// A fresh session around a payload: no wire client yet, server
    /// protocol `Disconnected` until [`Self::attach_server`] runs.
    pub(crate) fn new(id: RuntimeId, payload: RuntimePayload) -> Self {
        Self {
            id,
            payload,
            client: None,
            server_state: ServerState::Disconnected,
            requested_log_level: UiLogLevel::Info,
            device_sync: None,
            device_versions: (None, None),
            device_storage_id: None,
        }
    }

    pub fn id(&self) -> RuntimeId {
        self.id
    }

    pub fn kind(&self) -> RuntimeKind {
        self.payload.kind()
    }

    pub fn is_sim(&self) -> bool {
        matches!(self.kind(), RuntimeKind::Sim)
    }

    pub fn is_device(&self) -> bool {
        matches!(self.kind(), RuntimeKind::Device)
    }

    pub fn payload(&self) -> &RuntimePayload {
        &self.payload
    }

    /// Tear the session apart into its payload (attachment teardown: the
    /// wire client and per-session state drop here).
    pub fn into_payload(self) -> RuntimePayload {
        self.payload
    }

    /// The attached hardware's device state, when this is a device session.
    pub fn device_state(&self) -> Option<DeviceState> {
        match &self.payload {
            RuntimePayload::Device(handle) => Some(handle.state()),
            RuntimePayload::Sim(_) => None,
        }
    }

    /// The live hardware [`DeviceSession`], when this is a device session
    /// holding one.
    pub fn hardware_session(&self) -> Option<&DeviceSession> {
        match &self.payload {
            RuntimePayload::Device(handle) => handle.session(),
            RuntimePayload::Sim(_) => None,
        }
    }

    /// The `dev_` uid this session is associated with, once known.
    ///
    /// A device session's identity rides the wire hello: the association
    /// exists exactly from the moment the hello lands (the session state
    /// reaches `Ready { hello }`). The sim never has one (D22). The
    /// [`RuntimeId`] stays the stable pool key either way.
    pub fn device_uid(&self) -> Option<String> {
        match self.device_state() {
            Some(DeviceState::Ready { hello }) => hello.device_uid,
            _ => None,
        }
    }

    // -----------------------------------------------------------------
    // Server protocol (the retired ServerController's surface)
    // -----------------------------------------------------------------

    pub fn server_state(&self) -> &ServerState {
        &self.server_state
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.server_state, ServerState::Connected { .. }) && self.client.is_some()
    }

    /// The log level Studio last requested from this session's server, or
    /// `None` when the server protocol is not connected (the console's
    /// device-level selector disables itself on `None`).
    pub fn requested_log_level(&self) -> Option<UiLogLevel> {
        self.is_connected().then_some(self.requested_log_level)
    }

    /// Record a successfully applied device log level for optimistic
    /// display.
    pub fn set_requested_log_level(&mut self, level: UiLogLevel) {
        self.requested_log_level = level;
    }

    /// Attach the server protocol to this session's runtime: the hardware
    /// session hands over its readiness-gated channel; the sim keeps its
    /// worker io.
    pub fn attach_server(&mut self, updates: UxUpdateSink) -> Result<(), UiError> {
        match &self.payload {
            RuntimePayload::Sim(sim) => {
                // Direct field write (not a &mut self helper) so the state
                // transition can happen while the payload is borrowed.
                self.server_state = connecting_state();
                let client = StudioServerClient::from_sim_connection(
                    Rc::clone(&sim.connector),
                    &sim.connection,
                    updates,
                )?;
                self.install_client(client);
                Ok(())
            }
            RuntimePayload::Device(handle) => match handle.session() {
                Some(session) => {
                    self.server_state = connecting_state();
                    let client = StudioServerClient::from_device_session(session);
                    self.install_client(client);
                    Ok(())
                }
                None => Err(UiError::MissingSession(
                    "hardware attachment has no live device session".to_string(),
                )),
            },
        }
    }

    fn install_client(&mut self, client: StudioServerClient) {
        let protocol = client.protocol().to_string();
        self.client = Some(client);
        self.server_state = ServerState::Connected { protocol };
        // A fresh connection means a fresh server process/boot: its effective
        // log level is back at the init default.
        self.requested_log_level = UiLogLevel::Info;
    }

    /// The session's wire client, or the `MissingSession` surface every
    /// network op reports while no server protocol is connected.
    pub fn client_mut(&mut self) -> Result<&mut StudioServerClient, UiError> {
        self.client
            .as_mut()
            .ok_or_else(|| UiError::MissingSession("server client is not connected".to_string()))
    }

    /// Drain wire-carried log lines buffered on the client.
    pub fn take_pending_logs(&mut self) -> Vec<UiLogDraft> {
        self.client
            .as_mut()
            .map(StudioServerClient::take_pending_logs)
            .unwrap_or_default()
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.fail_with_kind(message, ServerFailureKind::Unknown);
    }

    pub fn fail_no_firmware(&mut self) {
        self.fail_with_kind(
            "No LightPlayer firmware detected.",
            ServerFailureKind::NoFirmware,
        );
    }

    pub fn fail_with_kind(&mut self, message: impl Into<String>, kind: ServerFailureKind) {
        self.client = None;
        self.server_state = ServerState::Failed {
            issue: UiIssue::new(message),
            kind,
        };
    }

    /// Detach the server protocol (drop the wire client) while keeping the
    /// runtime payload attached.
    pub fn disconnect_server(&mut self) {
        self.client = None;
        self.server_state = ServerState::Disconnected;
    }

    // -----------------------------------------------------------------
    // Reconcile bundle (connect-as-pull state, device sessions only)
    // -----------------------------------------------------------------

    pub fn device_sync(&self) -> Option<&DeviceSyncState> {
        self.device_sync.as_ref()
    }

    pub fn device_sync_mut(&mut self) -> Option<&mut DeviceSyncState> {
        self.device_sync.as_mut()
    }

    pub fn set_device_sync(&mut self, sync: Option<DeviceSyncState>) {
        self.device_sync = sync;
    }

    pub fn device_versions(&self) -> (Option<usize>, Option<usize>) {
        self.device_versions
    }

    pub fn set_device_versions(&mut self, versions: (Option<usize>, Option<usize>)) {
        self.device_versions = versions;
    }

    pub fn device_storage_id(&self) -> Option<&str> {
        self.device_storage_id.as_deref()
    }

    pub fn set_device_storage_id(&mut self, storage_id: Option<String>) {
        self.device_storage_id = storage_id;
    }

    /// Reset the whole reconcile bundle (a fresh pull is about to run, or
    /// the server protocol detached).
    pub fn clear_reconcile(&mut self) {
        self.device_sync = None;
        self.device_versions = (None, None);
        self.device_storage_id = None;
    }
}

/// Test seams: stubbed payloads and direct state injection for
/// view/derivation tests that must not script a whole fake device.
#[cfg(test)]
impl RuntimeSession {
    /// Replace the payload in place, keeping the wire client and server
    /// state (the retired slots were independently settable in tests).
    pub(crate) fn set_payload_for_test(&mut self, payload: RuntimePayload) {
        self.payload = payload;
    }

    pub(crate) fn set_server_state_for_test(&mut self, state: ServerState) {
        self.server_state = state;
    }

    pub(crate) fn set_client_for_test(&mut self, client: StudioServerClient) {
        let protocol = client.protocol().to_string();
        self.client = Some(client);
        self.server_state = ServerState::Connected { protocol };
        self.requested_log_level = UiLogLevel::Info;
    }
}

#[cfg(test)]
impl RuntimePayload {
    /// A stubbed hardware payload in the given device state.
    pub(crate) fn stub_device_for_test(state: DeviceState) -> Self {
        Self::Device(DeviceHandle::Stub(StubDevice { state }))
    }

    /// A stubbed SIMULATOR payload (record-level fake connector, synthetic
    /// session records) — the "connected but not hardware" fixture. The
    /// connector holds no real session, so flows that close it will error;
    /// fixtures using this only read views and speak through an injected
    /// server client.
    pub(crate) fn stub_sim_for_test() -> Self {
        use lpa_link::providers::fake::FakeProvider;
        use lpa_link::{LinkCapabilities, LinkConnectionKind, LinkProviderKind};
        Self::Sim(SimAttachment {
            connector: Rc::new(LinkConnector::Fake(FakeProvider::new())),
            session: LinkSession::new(
                "fake-session",
                LinkProviderKind::Fake,
                "fake-runtime",
                LinkConnectionKind::Fake,
                LinkCapabilities::esp32_serial_base(),
            ),
            connection: LinkConnection::fake("fake-runtime", "fake-session"),
        })
    }
}

/// The "Opening server protocol" connecting state every attach passes
/// through (the retired `ServerController::mark_connecting` label).
fn connecting_state() -> ServerState {
    ServerState::Connecting {
        progress: crate::ProgressState::new("Opening server protocol"),
    }
}

/// A `Ready { hello }` device state for stubbed hardware in tests.
#[cfg(test)]
pub fn ready_state_for_test() -> DeviceState {
    DeviceState::Ready {
        hello: lpc_wire::ServerHello {
            proto: lpc_wire::WIRE_PROTO_VERSION,
            fw: lpc_wire::FwProvenance {
                package: "fw-test".to_string(),
                commit: "0000000".to_string(),
                dirty: false,
                profile: "test".to_string(),
            },
            device_uid: None,
        },
    }
}
