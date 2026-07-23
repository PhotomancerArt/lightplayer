//! The editor's DEVICE pane (D23) + the runtime attachment it manages.
//!
//! The pre-M5 four-step connect wizard (Select connection / Connect
//! device / Connect LightPlayer / Open project, with Provider/Endpoint/
//! Session rows) is gone — the simulator is the ever-present fallback
//! runtime a project simply runs in, and "device" means actual hardware
//! (D22). What remains is a pane about the hardware:
//!
//! - **Disconnected** (or the runtime is the sim): where this project
//!   usually lives (registry association), the ambient runtime line
//!   ("Running in the simulator"), and the door to the deploy dialog.
//! - **Connected**: the device's identity and its contents related to
//!   the library (connect-as-pull, D8), push/dialog actions, and a
//!   visually separate firmware section (flash / erase — D15).
//!
//! Since M4/P5 the controller also owns the CONNECT FLOW: the provider
//! catalog (picker state, expressed as [`ConnectFlowState`] for the views).
//! Since the runtime pool (M4 of the device-UX roadmap) it is the session
//! FACTORY: connect flows build and return a
//! [`RuntimePayload`](crate::RuntimePayload) — a [`DeviceSession`] for
//! hardware, a worker-io [`SimAttachment`](crate::SimAttachment) for the
//! browser simulator — and `StudioController` installs it into the
//! [`RuntimePool`](crate::RuntimePool). The controller itself is slotless:
//! sessions live in the pool, the server protocol lives on the session.
//!
//! Connect/endpoint flows live inside the deploy dialog (M5); this pane
//! never renders provider plumbing.

use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::providers::{LinkEnv, LinkProviderRegistry};
use lpa_link::{
    DeviceSession, DeviceState, DeviceTimers, LinkConnector, LinkEndpointId, LinkProvider,
    LinkProviderKind,
};

use crate::app::device::DeployOp;
use crate::app::device::connect_choices::{provider_auto_connects, provider_choices};
use crate::app::device::device_event_adapter::console_event_sink;
use crate::app::device::link_ux::{link_session_logs, map_link_error};
use crate::app::runtime_pool::runtime_session::DeviceHandle;
use crate::core::view::steps_view::{UiStepState, UiStepView};
use crate::{
    ConnectFlowState, ConnectedDeviceSummary, Controller, ControllerId, DeviceOp, EndpointChoice,
    ProgressState, RuntimePayload, ServerFailureKind, ServerState, SimAttachment, UiAction,
    UiError, UiIssue, UiLogDraft, UiMetric, UiPaneView, UiStatus, UiStepsView, UiViewContent,
};

use crate::app::places::{DeviceContent, DeviceSyncState};

pub struct DeviceController {
    /// Catalog + factory: consulted when a flow needs the picker list or
    /// the kind's shared connector (memoized per kind, so endpoint state
    /// minted by one flow is visible to the next); never borrowed across an
    /// await (its methods are synchronous and it is owned by value).
    registry: LinkProviderRegistry,
    /// The connect-flow view state (picker/progress/failure). `Connected`
    /// is entered exactly when a connect flow hands a live
    /// [`RuntimePayload`] to the caller.
    flow: ConnectFlowState,
    /// The remembered device a one-click reconnect was aimed at: the
    /// connect window renders on THAT card (no transient twin) until the
    /// identity read lands or the flow resets. Cleared with the flow.
    pending_reconnect_uid: Option<String>,
    /// Injected timer factory for [`DeviceSession`] deadlines. The default
    /// is IMMEDIATE-READY sleeps (deadlines fire instantly) — fine for
    /// builds with no hardware connectors; the web shell installs its
    /// gloo-backed timers at startup and tests install poll timers.
    timers: DeviceTimers,
    /// The MOST RECENT hardware connect's console-log buffer. A fresh
    /// buffer is minted per connect and travels with the session payload
    /// (per-session routing, runtime-pool P2); this alias covers the
    /// window before the payload lands in the pool — and failed connects,
    /// whose captured boot chatter would otherwise be lost.
    pending_device_logs: Rc<RefCell<Vec<UiLogDraft>>>,
}

/// The runtime evidence the device pane derives its sections from, read
/// off the [`RuntimePool`](crate::RuntimePool) by `StudioController` (the
/// controller itself is slotless).
pub(crate) struct DeviceRuntimeEvidence {
    /// The attached runtime is real hardware (the sim is not a device —
    /// D22).
    pub is_hardware: bool,
    /// The attached runtime is the simulator.
    pub is_sim: bool,
    /// The attached hardware's device state, when hardware is attached.
    pub device_state: Option<DeviceState>,
    /// The lens session's server protocol state (`Disconnected` when no
    /// session exists).
    pub server_state: ServerState,
}

impl DeviceRuntimeEvidence {
    fn has_lightplayer_state(&self) -> bool {
        matches!(self.server_state, ServerState::Connected { .. })
    }
}

/// Outcome of [`DeviceController::open_provider`].
pub enum DeviceOpenOutcome {
    /// Endpoint discovery finished; the picker state carries the choices.
    Opened,
    /// A single endpoint auto-connected; the payload is the live session
    /// material for the pool.
    Connected {
        payload: RuntimePayload,
        logs: Vec<UiLogDraft>,
    },
    /// The user cancelled (browser port picker).
    Cancelled { message: String },
}

impl DeviceController {
    pub const NODE_ID: &'static str = "studio|device";
    /// The pane's single device section — also the activity target the
    /// connect/flash/push flows report progress against.
    pub const SECTION_DEVICE: &'static str = "device";
    /// Firmware operations, visually separate from project deploy (D15).
    pub const SECTION_FIRMWARE: &'static str = "firmware";

    pub fn new() -> Self {
        Self::with_registry(LinkProviderRegistry::from_env(LinkEnv::default()))
    }

    pub fn with_registry(registry: LinkProviderRegistry) -> Self {
        let flow = ConnectFlowState::SelectingProvider {
            providers: provider_choices(&registry),
            issue: None,
        };
        Self {
            registry,
            flow,
            timers: DeviceTimers::new(|_| Box::pin(std::future::ready(()))),
            pending_device_logs: Rc::new(RefCell::new(Vec::new())),
            pending_reconnect_uid: None,
        }
    }

    /// Install the platform's timer factory for device-session deadlines
    /// (gloo timers on the web, poll timers in host tests). Install it
    /// before any hardware connect; the constructor default makes every
    /// deadline fire immediately.
    pub fn set_timers(&mut self, timers: DeviceTimers) {
        self.timers = timers;
    }

    /// The connect-flow view state (picker/progress/failure).
    pub fn flow_state(&self) -> &ConnectFlowState {
        &self.flow
    }

    /// Hardware device classes in this build's catalog (descriptors whose
    /// class can flash firmware), for the deploy dialog's connect actions.
    pub(crate) fn hardware_device_kinds(&self) -> Vec<LinkProviderKind> {
        crate::app::device::connect_choices::hardware_device_descriptors(&self.registry)
            .into_iter()
            .map(|descriptor| descriptor.kind)
            .collect()
    }

    /// Drain the console drafts buffered by the session's event sink.
    pub(crate) fn take_pending_device_logs(&mut self) -> Vec<UiLogDraft> {
        core::mem::take(&mut *self.pending_device_logs.borrow_mut())
    }

    // -----------------------------------------------------------------
    // Connect flow (hardware lands on a DeviceSession, BrowserWorker on
    // a SimAttachment)
    // -----------------------------------------------------------------

    /// Reset to the provider catalog WITHOUT a provider close
    /// (`RefreshConnections` semantics; the caller drops the pool session).
    pub fn refresh_provider_catalog(&mut self) {
        self.reset_to_provider_selection(None);
    }

    fn reset_to_provider_selection(&mut self, issue: Option<UiIssue>) {
        self.pending_reconnect_uid = None;
        self.flow = ConnectFlowState::SelectingProvider {
            providers: provider_choices(&self.registry),
            issue,
        };
    }

    fn recover_to_provider_selection(&mut self, message: impl Into<String>) {
        self.pending_reconnect_uid = None;
        self.reset_to_provider_selection(Some(UiIssue::new(message)));
    }

    /// Mark the flow failed (surfaced as the gallery issue chip).
    pub fn fail(&mut self, message: impl Into<String>) {
        self.flow = ConnectFlowState::Failed {
            issue: UiIssue::new(message),
        };
    }

    /// Open a provider: discover endpoints into the picker state, and
    /// auto-connect when the provider has exactly one endpoint and is an
    /// auto-connecting kind (BrowserWorker/HostProcess). Browser serial
    /// goes through the port-permission picker instead of discovery.
    pub async fn open_provider(
        &mut self,
        provider_id: LinkProviderKind,
    ) -> Result<DeviceOpenOutcome, UiError> {
        if provider_id == LinkProviderKind::BrowserSerialEsp32 {
            return self.open_browser_serial_provider().await;
        }

        self.discover_provider_endpoints(provider_id).await?;
        let endpoints = match &self.flow {
            ConnectFlowState::SelectingEndpoint { endpoints, .. } => endpoints.clone(),
            _ => Vec::new(),
        };
        if endpoints.len() == 1 && provider_auto_connects(provider_id) {
            let endpoint_id = endpoints[0].id.clone();
            let (payload, logs) = self.connect_endpoint(provider_id, endpoint_id).await?;
            return Ok(DeviceOpenOutcome::Connected { payload, logs });
        }
        Ok(DeviceOpenOutcome::Opened)
    }

    async fn discover_provider_endpoints(
        &mut self,
        provider_id: LinkProviderKind,
    ) -> Result<(), UiError> {
        self.flow = ConnectFlowState::DiscoveringEndpoints {
            provider_id,
            progress: ProgressState::new("Discovering endpoints"),
        };

        let result = match self.registry.create_connector(provider_id) {
            Ok(connector) => connector.discover().await.map_err(map_link_error),
            Err(error) => Err(map_link_error(error)),
        };
        let endpoints = match result {
            Ok(endpoints) => endpoints,
            Err(error) => {
                self.recover_to_provider_selection(error.message());
                return Err(error);
            }
        };
        if endpoints.is_empty() {
            let message = format!("{} did not report any endpoints", provider_id.label());
            self.recover_to_provider_selection(message.clone());
            return Err(UiError::Link(message));
        }

        self.flow = ConnectFlowState::SelectingEndpoint {
            provider_id,
            endpoints: endpoints
                .into_iter()
                .map(EndpointChoice::from_endpoint)
                .collect(),
        };
        Ok(())
    }

    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    async fn open_browser_serial_provider(&mut self) -> Result<DeviceOpenOutcome, UiError> {
        self.flow = ConnectFlowState::DiscoveringEndpoints {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            progress: ProgressState::new("Requesting browser serial access"),
        };

        let result = match self
            .registry
            .create_connector(LinkProviderKind::BrowserSerialEsp32)
        {
            Ok(connector) => match &*connector {
                LinkConnector::BrowserSerialEsp32(provider) => {
                    provider.request_access().await.map_err(map_link_error)
                }
                _ => Err(UiError::Link(
                    "browser serial connector has the wrong provider type".to_string(),
                )),
            },
            Err(error) => Err(map_link_error(error)),
        };
        let endpoint = match result {
            Ok(endpoint) => endpoint,
            Err(UiError::Cancelled(message)) => {
                self.reset_to_provider_selection(None);
                return Ok(DeviceOpenOutcome::Cancelled { message });
            }
            Err(error) => {
                self.recover_to_provider_selection(error.message());
                return Err(error);
            }
        };
        let endpoint_choice = EndpointChoice::from_endpoint(endpoint);
        let endpoint_id = endpoint_choice.id.clone();
        self.flow = ConnectFlowState::SelectingEndpoint {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            endpoints: vec![endpoint_choice],
        };
        let (payload, logs) = self
            .connect_endpoint(LinkProviderKind::BrowserSerialEsp32, endpoint_id)
            .await?;
        Ok(DeviceOpenOutcome::Connected { payload, logs })
    }

    #[cfg(not(all(feature = "browser-serial-esp32", target_arch = "wasm32")))]
    async fn open_browser_serial_provider(&mut self) -> Result<DeviceOpenOutcome, UiError> {
        Err(UiError::UnsupportedFeature(
            "browser serial ESP32 access requires the browser-serial-esp32 feature on wasm"
                .to_string(),
        ))
    }

    /// The remembered device a one-click reconnect targets, while the
    /// connect window is open (cleared with the flow): the roster renders
    /// the connect narration ON that card instead of a transient twin.
    pub fn pending_reconnect_uid(&self) -> Option<&str> {
        self.pending_reconnect_uid.as_deref()
    }

    /// One-click reconnect (M1): connect through a serial port this origin
    /// was ALREADY granted — no chooser. Which physical device a grant
    /// belongs to is unknowable pre-connect, so the first granted endpoint
    /// is connected (single-grant is the common case) and identity is
    /// reconciled from the hello. Falls back to the permission chooser when
    /// no grant exists yet.
    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    pub async fn reconnect_granted_device(
        &mut self,
        uid: Option<String>,
    ) -> Result<DeviceOpenOutcome, UiError> {
        self.pending_reconnect_uid = uid;
        self.flow = ConnectFlowState::DiscoveringEndpoints {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            progress: ProgressState::new("Finding granted serial ports"),
        };

        let result = match self
            .registry
            .create_connector(LinkProviderKind::BrowserSerialEsp32)
        {
            Ok(connector) => match &*connector {
                LinkConnector::BrowserSerialEsp32(provider) => provider
                    .discover_granted_endpoints()
                    .await
                    .map_err(map_link_error),
                _ => Err(UiError::Link(
                    "browser serial connector has the wrong provider type".to_string(),
                )),
            },
            Err(error) => Err(map_link_error(error)),
        };
        let endpoints = match result {
            Ok(endpoints) => endpoints,
            Err(error) => {
                self.recover_to_provider_selection(error.message());
                return Err(error);
            }
        };
        let Some(endpoint) = endpoints.into_iter().next() else {
            return self.open_browser_serial_provider().await;
        };
        let endpoint_choice = EndpointChoice::from_endpoint(endpoint);
        let endpoint_id = endpoint_choice.id.clone();
        self.flow = ConnectFlowState::SelectingEndpoint {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            endpoints: vec![endpoint_choice],
        };
        let (payload, logs) = self
            .connect_endpoint(LinkProviderKind::BrowserSerialEsp32, endpoint_id)
            .await?;
        Ok(DeviceOpenOutcome::Connected { payload, logs })
    }

    #[cfg(not(all(feature = "browser-serial-esp32", target_arch = "wasm32")))]
    pub async fn reconnect_granted_device(
        &mut self,
        _uid: Option<String>,
    ) -> Result<DeviceOpenOutcome, UiError> {
        Err(UiError::UnsupportedFeature(
            "browser serial ESP32 access requires the browser-serial-esp32 feature on wasm"
                .to_string(),
        ))
    }

    /// Connect one endpoint: BrowserWorker becomes a [`SimAttachment`]
    /// payload; every other kind becomes a hardware [`DeviceSession`]
    /// payload (readiness is NOT awaited here — the server attach's first
    /// request drives it). The caller installs the returned payload into
    /// the pool.
    pub async fn connect_endpoint(
        &mut self,
        provider_id: LinkProviderKind,
        endpoint_id: LinkEndpointId,
    ) -> Result<(RuntimePayload, Vec<UiLogDraft>), UiError> {
        let endpoint = self
            .endpoint_choice(provider_id, &endpoint_id)
            .unwrap_or_else(|| EndpointChoice {
                provider_id,
                id: endpoint_id.clone(),
                label: endpoint_id.as_str().to_string(),
                summary: "Open this endpoint.".to_string(),
                status: lpa_link::LinkEndpointStatus::Available,
            });
        self.flow = ConnectFlowState::Connecting {
            endpoint: endpoint.clone(),
            progress: ProgressState::new("Opening link session"),
        };

        let connector = match self.registry.create_connector(provider_id) {
            Ok(connector) => connector,
            Err(error) => {
                let error = map_link_error(error);
                self.recover_to_provider_selection(error.message());
                return Err(error);
            }
        };
        let result = if provider_id == LinkProviderKind::BrowserWorker {
            open_sim_attachment(connector, &endpoint_id).await
        } else {
            // Per-session console-log routing (runtime-pool P2): mint a
            // fresh buffer for this connect; the session payload carries
            // it, and the controller field aliases it for the window
            // before the pool holds the session (and for failed connects).
            let console_logs = Rc::new(RefCell::new(Vec::new()));
            self.pending_device_logs = Rc::clone(&console_logs);
            let sink = console_event_sink(Rc::clone(&console_logs));
            match DeviceSession::connect(connector, &endpoint_id, self.timers.clone(), sink).await {
                Ok(session) => {
                    let connector = session.connector();
                    let logs = link_session_logs(&connector, session.session().id())?;
                    Ok((
                        RuntimePayload::Device(DeviceHandle::Session {
                            session,
                            console_logs,
                        }),
                        logs,
                    ))
                }
                Err(error) => Err(map_link_error(error)),
            }
        };

        let (payload, logs) = match result {
            Ok(result) => result,
            Err(error) => {
                self.recover_to_provider_selection(error.message());
                return Err(error);
            }
        };
        let session = payload
            .link_session()
            .unwrap_or_else(|| unreachable!("connect_endpoint builds live sessions only"));
        self.flow = ConnectFlowState::Connected {
            device: ConnectedDeviceSummary::new(
                provider_id,
                session.endpoint_id.as_str(),
                session.id().as_str(),
                endpoint.label,
            ),
        };
        Ok((payload, logs))
    }

    /// Attachment teardown: close the given session payload (taken out of
    /// the pool by the caller) and return to the provider catalog (failure
    /// lands on the flow's `Failed` state).
    pub async fn disconnect(&mut self, payload: Option<RuntimePayload>) -> Result<(), UiError> {
        self.pending_reconnect_uid = None;
        let result = match payload {
            None => Ok(()),
            Some(RuntimePayload::Sim(sim)) => sim
                .connector
                .close(&sim.session.id)
                .await
                .map_err(map_link_error),
            Some(RuntimePayload::Device(handle)) => handle.close().await,
        };
        match result {
            Ok(()) => {
                self.refresh_provider_catalog();
                Ok(())
            }
            Err(error) => {
                self.fail(error.message());
                Err(error)
            }
        }
    }

    fn endpoint_choice(
        &self,
        provider_id: LinkProviderKind,
        endpoint_id: &LinkEndpointId,
    ) -> Option<EndpointChoice> {
        match &self.flow {
            ConnectFlowState::SelectingEndpoint {
                provider_id: state_provider,
                endpoints,
            } if *state_provider == provider_id => endpoints
                .iter()
                .find(|endpoint| endpoint.id == *endpoint_id)
                .cloned(),
            ConnectFlowState::Connecting { endpoint, .. }
                if endpoint.provider_id == provider_id && endpoint.id == *endpoint_id =>
            {
                Some(endpoint.clone())
            }
            _ => None,
        }
    }

    // -----------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------

    /// The editor's device pane (D23). `runtime` is the pool-derived
    /// runtime evidence; `device_sync` is the connect-time pull result;
    /// `usual_device` names where this project usually lives when nothing
    /// is connected.
    pub(crate) fn view(
        &self,
        runtime: &DeviceRuntimeEvidence,
        device_sync: Option<&DeviceSyncState>,
        usual_device: Option<String>,
    ) -> UiPaneView {
        let sections = if runtime.is_hardware {
            let mut sections = vec![self.connected_device_section(runtime, device_sync)];
            sections.push(self.firmware_section());
            sections
        } else {
            vec![self.disconnected_device_section(runtime, usual_device)]
        };
        UiPaneView::new(
            Self::NODE_ID,
            "Device",
            self.status(runtime, device_sync),
            UiViewContent::Stack(Box::new(UiStepsView::new(sections))),
            Vec::new(),
        )
    }

    fn status(
        &self,
        runtime: &DeviceRuntimeEvidence,
        device_sync: Option<&DeviceSyncState>,
    ) -> UiStatus {
        if !runtime.is_hardware {
            return UiStatus::neutral("No device");
        }
        // Incompatible firmware outranks the server state: the ONE
        // affordance is reflashing (explicit, never automatic).
        if matches!(runtime.device_state, Some(DeviceState::Incompatible { .. })) {
            return UiStatus::attention("Reflash needed");
        }
        match &runtime.server_state {
            ServerState::Failed {
                kind: ServerFailureKind::NoFirmware,
                ..
            } => UiStatus::attention("Ready to flash"),
            ServerState::Failed { .. } => UiStatus::error("Needs attention"),
            ServerState::Connecting { .. } => UiStatus::working("Connecting"),
            ServerState::Connected { .. } => {
                match device_sync.and_then(|sync| sync.identity.as_ref()) {
                    Some(identity) => UiStatus::good(identity.name.clone()),
                    None => UiStatus::good("Connected"),
                }
            }
            ServerState::Disconnected => UiStatus::working("Connecting"),
        }
    }

    /// The pane when no hardware is attached: association line, ambient
    /// runtime line, and the dialog entry.
    fn disconnected_device_section(
        &self,
        runtime: &DeviceRuntimeEvidence,
        usual_device: Option<String>,
    ) -> UiStepView {
        let mut lines = Vec::new();
        if let Some(usual) = usual_device {
            lines.push(usual);
        }
        if runtime.is_sim && runtime.has_lightplayer_state() {
            // D16: name where you are — ambient, not a "device"
            lines.push("Running in the simulator.".to_string());
        }
        if lines.is_empty() {
            lines.push("No device connected.".to_string());
        }
        UiStepView::new(Self::SECTION_DEVICE, "Device", UiStepState::Pending)
            .with_body(UiViewContent::text(lines.join("\n")))
            .with_actions(vec![UiAction::from_op(
                ControllerId::new(crate::app::device::DEPLOY_NODE_ID),
                DeployOp::OpenDialog { target_key: None },
            )])
    }

    /// The pane when hardware is attached: identity, contents relation,
    /// and the push/disconnect actions.
    fn connected_device_section(
        &self,
        runtime: &DeviceRuntimeEvidence,
        device_sync: Option<&DeviceSyncState>,
    ) -> UiStepView {
        // Incompatible surfaces here with its reflash-only explanation; the
        // firmware section below carries the affordance.
        if let Some(DeviceState::Incompatible { reason }) = &runtime.device_state {
            return UiStepView::new(Self::SECTION_DEVICE, "Device", UiStepState::NeedsAttention)
                .with_body(UiViewContent::Issue(UiIssue::new(reason.message())))
                .with_actions(self.connected_device_actions());
        }
        let state = match &runtime.server_state {
            ServerState::Failed { .. } => UiStepState::NeedsAttention,
            ServerState::Connecting { .. } | ServerState::Disconnected => UiStepState::Active,
            ServerState::Connected { .. } => UiStepState::Complete,
        };
        let mut metrics = Vec::new();
        if let Some(sync) = device_sync {
            if let Some(identity) = &sync.identity {
                metrics.push(UiMetric::new("Name", &identity.name));
            }
            metrics.push(UiMetric::new("Holds", content_line(&sync.content)));
        }
        if let ServerState::Connected { protocol } = &runtime.server_state {
            metrics.push(UiMetric::new("Protocol", protocol));
        }
        let body = if metrics.is_empty() {
            match &runtime.server_state {
                ServerState::Failed {
                    kind: ServerFailureKind::NoFirmware,
                    ..
                } => UiViewContent::text("No LightPlayer firmware is running on this device."),
                ServerState::Failed { issue, .. } => UiViewContent::Issue(issue.clone()),
                _ => UiViewContent::text("Connecting to the device…"),
            }
        } else {
            UiViewContent::Metrics(metrics)
        };
        UiStepView::new(Self::SECTION_DEVICE, "Device", state)
            .with_body(body)
            .with_actions(self.connected_device_actions())
    }

    fn connected_device_actions(&self) -> Vec<UiAction> {
        vec![
            UiAction::from_op(
                ControllerId::new(crate::app::device::DEPLOY_NODE_ID),
                DeployOp::OpenDialog { target_key: None },
            )
            .with_label("Push to device…")
            .with_summary("Review and push a project to this device.")
            .with_icon("upload"),
            UiAction::from_op(self.node_id(), DeviceOp::DisconnectDevice)
                .with_label("Disconnect")
                .with_summary("Close the device session."),
        ]
    }

    /// Firmware ops, visually separate from deploy (D15).
    fn firmware_section(&self) -> UiStepView {
        UiStepView::new(Self::SECTION_FIRMWARE, "Firmware", UiStepState::Complete)
            .with_body(UiViewContent::text(
                "Firmware operations are separate from project deploys.",
            ))
            .with_actions(vec![
                UiAction::from_op(self.node_id(), DeviceOp::ProvisionFirmware)
                    .with_label("Update firmware")
                    .with_summary("Flash the bundled LightPlayer firmware.")
                    .with_icon("zap"),
                UiAction::from_op(self.node_id(), DeviceOp::ResetToBlank)
                    .with_label("Erase device")
                    .with_summary("Erase the device's flash storage entirely.")
                    .with_icon("remove"),
            ])
    }
}

/// Test seams: stubbed connect-flow state for view/derivation tests that
/// must not script a whole fake device (the stub PAYLOADS live on
/// [`RuntimePayload`]'s test constructors; `StudioController` installs
/// them into the pool).
#[cfg(test)]
impl DeviceController {
    /// Mark the flow `Connected` (Fake provider vocabulary, matching the
    /// old `set_state(Connected) + set_active_connection` seam).
    pub(crate) fn set_stub_connected_flow_for_test(&mut self) {
        self.flow = ConnectFlowState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        };
    }

    /// Poll timers for host tests: each sleep completes when its wall-clock
    /// deadline passes, checked per poll (works under noop-waker harnesses
    /// that re-poll on a cadence).
    pub(crate) fn test_poll_timers() -> DeviceTimers {
        DeviceTimers::new(|duration| {
            let deadline = std::time::Instant::now() + duration;
            Box::pin(std::future::poll_fn(move |_context| {
                if std::time::Instant::now() >= deadline {
                    std::task::Poll::Ready(())
                } else {
                    std::task::Poll::Pending
                }
            }))
        })
    }
}

/// Open the simulator attachment: connect + connection handoff (no
/// readiness — boot-ready IS the session, D22).
async fn open_sim_attachment(
    connector: Rc<LinkConnector>,
    endpoint_id: &LinkEndpointId,
) -> Result<(RuntimePayload, Vec<UiLogDraft>), UiError> {
    let session = connector
        .connect(endpoint_id)
        .await
        .map_err(map_link_error)?;
    let connection = match connector.connection(session.id()).await {
        Ok(connection) => connection,
        Err(error) => {
            let _ = connector.close(session.id()).await;
            return Err(map_link_error(error));
        }
    };
    let logs = match link_session_logs(&connector, session.id()) {
        Ok(logs) => logs,
        Err(error) => {
            let _ = connector.close(session.id()).await;
            return Err(error);
        }
    };
    Ok((
        RuntimePayload::Sim(SimAttachment {
            connector,
            session,
            connection,
        }),
        logs,
    ))
}

/// One line for what the device holds, from the connect-time pull.
fn content_line(content: &DeviceContent) -> String {
    match content {
        DeviceContent::Empty => "Nothing yet".to_string(),
        DeviceContent::Known { slug, relation, .. } => match relation {
            lpc_history::SyncRelation::AtHead => format!("{slug} — up to date"),
            lpc_history::SyncRelation::Behind => format!("{slug} — behind your copy"),
            lpc_history::SyncRelation::Diverged => format!("{slug} — edited elsewhere"),
        },
        DeviceContent::Adopted { slug, .. } => format!("{slug} — pulled into your library"),
        DeviceContent::PendingIdentity { .. } => {
            "A project — name this device to keep it".to_string()
        }
        DeviceContent::Unreadable { .. } => "Contents unreadable".to_string(),
    }
}

impl Controller for DeviceController {
    type Op = DeviceOp;

    fn node_id(&self) -> ControllerId {
        ControllerId::new(Self::NODE_ID)
    }
}

impl Default for DeviceController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use lpa_link::providers::LinkProviderRegistry;
    use lpa_link::providers::fake::FakeProvider;
    use lpa_link::{LinkEndpoint, LinkEndpointId, LinkProviderKind};

    use super::*;

    #[test]
    fn new_controller_projects_provider_catalog_into_the_flow() {
        let device = DeviceController::with_registry(registry_with_fake_endpoint());

        assert!(matches!(
            device.flow_state(),
            ConnectFlowState::SelectingProvider { providers, .. }
                if providers.len() == 1 && providers[0].id == LinkProviderKind::Fake
        ));
    }

    #[test]
    fn connect_flow_uses_the_connector_instance_the_registry_already_handed_out() {
        // Regression for the browser-serial "link endpoint not found" bug:
        // the connect flow must land on the SAME factory-built connector a
        // previous flow (request_access analog) got from the registry. State
        // armed on the externally held instance is only visible to
        // `connect_endpoint` when the registry memoizes.
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());
        let shared = registry.create_connector(LinkProviderKind::Fake).unwrap();
        let mut device = DeviceController::with_registry(registry);
        match &*shared {
            LinkConnector::Fake(provider) => {
                provider.set_connect_error(Some("armed on the shared instance".to_string()));
            }
            _ => unreachable!("factory Fake kind builds a fake connector"),
        }

        let result = block_on_ready(
            device.connect_endpoint(LinkProviderKind::Fake, LinkEndpointId::new("fake-runtime")),
        );

        // A per-call fresh provider would never see the armed error and
        // would fail with endpoint-not-found instead.
        assert!(matches!(
            &result,
            Err(UiError::Link(message)) if message.contains("armed on the shared instance")
        ));
    }

    #[test]
    fn failed_endpoint_discovery_returns_to_provider_selection_with_issue() {
        let mut device = DeviceController::with_registry(registry_with_fake(
            FakeProvider::new()
                .with_endpoint(fake_endpoint())
                .with_discover_error("serial discovery failed"),
        ));

        let result = block_on_ready(device.open_provider(LinkProviderKind::Fake));

        assert!(matches!(result, Err(UiError::Link(_))));
        assert!(matches!(
            device.flow_state(),
            ConnectFlowState::SelectingProvider {
                issue: Some(issue),
                ..
            } if issue.message.contains("serial discovery failed")
        ));
    }

    #[test]
    fn failed_connection_handoff_returns_to_provider_selection_with_issue() {
        let mut device = DeviceController::with_registry(registry_with_fake(
            FakeProvider::new()
                .with_endpoint(fake_endpoint())
                .with_connection_error("server handoff failed"),
        ));

        let result = block_on_ready(
            device.connect_endpoint(LinkProviderKind::Fake, LinkEndpointId::new("fake-runtime")),
        );

        assert!(matches!(result, Err(UiError::Link(_))));
        assert!(matches!(
            device.flow_state(),
            ConnectFlowState::SelectingProvider {
                issue: Some(issue),
                ..
            } if issue.message.contains("server handoff failed")
        ));
        // The factory returned no payload, so no session material exists —
        // the retired `has_runtime_attachment` field read is structural now.
    }

    fn fake_endpoint() -> LinkEndpoint {
        LinkEndpoint::new("fake-runtime", LinkProviderKind::Fake, "Fake runtime")
    }

    fn registry_with_fake_endpoint() -> LinkProviderRegistry {
        registry_with_fake(FakeProvider::new().with_endpoint(fake_endpoint()))
    }

    fn registry_with_fake(provider: FakeProvider) -> LinkProviderRegistry {
        let mut registry = LinkProviderRegistry::new();
        registry.insert(provider);
        registry
    }

    fn block_on_ready<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
