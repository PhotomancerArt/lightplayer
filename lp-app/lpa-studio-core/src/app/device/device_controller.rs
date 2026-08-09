//! The editor's DEVICE pane (D23) + the connect flow / session factory.
//!
//! The pre-M5 four-step connect wizard (Select connection / Connect
//! device / Connect LightPlayer / Open project, with Provider/Endpoint/
//! Session rows) is gone — the simulator is the ever-present fallback
//! runtime a project simply runs in, and "device" means actual hardware
//! (D22). What remains is a pane about the hardware:
//!
//! - **Disconnected** (or the runtime is the sim): where this project
//!   usually lives (registry association), the ambient runtime line
//!   ("Running in the simulator"), and the connect affordance.
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
//! Connect/endpoint flows narrate on the gallery cards (M6/M8′); this pane
//! never renders provider plumbing.

use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::providers::{LinkEnv, LinkProviderRegistry};
use lpa_link::{
    DeviceSession, DeviceTimers, LinkConnector, LinkEndpointId, LinkProvider, LinkProviderKind,
};

use crate::app::device::connect_choices::{provider_auto_connects, provider_choices};
use crate::app::device::device_event_adapter::console_event_sink;
use crate::app::device::link_ux::{link_session_logs, map_link_error};
use crate::app::runtime_pool::runtime_session::DeviceHandle;
use crate::core::log::{DeviceEventKind, DeviceEventRecorder};
use crate::{
    ConnectFlowState, ConnectedDeviceSummary, Controller, ControllerId, DeviceOp, EndpointChoice,
    ProgressState, RuntimePayload, SimAttachment, UiError, UiIssue, UiLogDraft,
};

use crate::app::device::link_ux::is_port_held_error;

/// The breath between the ladder's two connect attempts (M6): long
/// enough for the OS to release the port after a failed open, short
/// enough that the "Resetting…" narration doesn't feel stuck.
const CONNECT_RETRY_BACKOFF: core::time::Duration = core::time::Duration::from_millis(750);

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
    /// The device event log's recording handle: every flow transition is
    /// recorded through [`Self::set_flow`], and each hardware connect's
    /// console sink records through a clone. Noop until the studio
    /// controller installs the shared recorder.
    events: DeviceEventRecorder,
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
    /// The connect ladder ended without a session (M6): the flow narrates
    /// the honest state on the CARD (`PortHeld` → In-use-elsewhere,
    /// `Unresponsive` → Not-responding). Soft by design — no error
    /// propagates, no toast, no issue chip (D32 / the D31 replacement).
    SoftFailed,
}

/// What phase one of a port request produced
/// ([`DeviceController::choose_provider_endpoint`]).
///
/// No derives, for the same reason [`DeviceOpenOutcome`] has none: it
/// carries one.
pub enum PortChoice {
    /// An endpoint is in hand, and phase two will connect it. A caller can
    /// say so before starting that wait — those are the several seconds a
    /// user otherwise spends reading the chooser's copy.
    Endpoint(LinkEndpointId),
    /// Nothing to connect: the picker is showing its choices, or the user
    /// dismissed it. Carries the outcome `open_provider` reports.
    Ended(DeviceOpenOutcome),
}

/// One already-granted Web Serial port, as the D7 grant sweep sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantedPortSummary {
    pub endpoint_id: LinkEndpointId,
    pub label: String,
    /// `getInfo()`'s VID:PID, when the browser exposed one — all a grant
    /// can say about itself before it is opened.
    pub usb_vid_pid: Option<(u16, u16)>,
}

/// The D7 grant ladder's verdict for one port request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantPortPlan {
    /// Exactly one unambiguous grant: adopt it — visibly and reversibly,
    /// which is the CALLER's job to narrate.
    Adopt {
        endpoint_id: LinkEndpointId,
        label: String,
    },
    /// Grants exist but none is unambiguous: offer them as a list and let
    /// the user (or a driving agent) pick deliberately.
    Offer { ports: Vec<GrantedPortSummary> },
    /// Nothing grant-derived applies: the browser's own chooser.
    Chooser,
}

/// The D7 ladder, pure so it is testable on the host: which of adopt /
/// offer / chooser a strategy and a grant list add up to.
///
/// `board_usb` is the BOARD_FIRST refinement: when the user already named
/// the board, its `usb_bridge` VID:PID narrows the list — one match adopts
/// even when unrelated grants exist; zero matches means the board's own
/// port has never been granted, and only the chooser can grant it.
/// `getInfo()` cannot tell two identical bridge chips apart, so several
/// matches stay an offer, never a guess.
pub fn plan_for_granted_ports(
    strategy: crate::PortRequestStrategy,
    board_usb: Option<(u16, u16)>,
    ports: Vec<GrantedPortSummary>,
) -> GrantPortPlan {
    use crate::PortRequestStrategy as Strategy;
    if strategy == Strategy::ChooserOnly || ports.is_empty() {
        return GrantPortPlan::Chooser;
    }
    let candidates: Vec<GrantedPortSummary> = match board_usb {
        Some(usb) => {
            let matches: Vec<GrantedPortSummary> = ports
                .iter()
                .filter(|port| port.usb_vid_pid == Some(usb))
                .cloned()
                .collect();
            if matches.is_empty() {
                // The named board's bridge is not among the grants: no
                // grant can become its port, so the chooser is the only
                // honest answer.
                return GrantPortPlan::Chooser;
            }
            matches
        }
        None => ports,
    };
    match (strategy, candidates.len()) {
        (Strategy::AutoAdopt, 1) => {
            let port = candidates.into_iter().next().expect("len checked");
            GrantPortPlan::Adopt {
                endpoint_id: port.endpoint_id,
                label: port.label,
            }
        }
        // ListOnly, or several candidates: never guess between grants.
        _ => GrantPortPlan::Offer { ports: candidates },
    }
}

impl DeviceController {
    pub const NODE_ID: &'static str = "studio|device";

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
            events: DeviceEventRecorder::noop(),
        }
    }

    /// Install the shared device event recorder (studio controller wiring).
    pub(crate) fn set_event_recorder(&mut self, events: DeviceEventRecorder) {
        self.events = events;
    }

    /// The one funnel for flow transitions: records `from → to` in the
    /// device event log, then applies. Every non-constructor flow
    /// assignment goes through here so a trace shows the connect flow's
    /// whole story.
    fn set_flow(&mut self, next: ConnectFlowState) {
        let from = self.flow.label();
        let to = next.label();
        if from != to {
            self.events.record(
                None,
                None,
                DeviceEventKind::Flow {
                    from: from.to_string(),
                    to: to.to_string(),
                },
            );
        }
        self.flow = next;
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
        self.set_flow(ConnectFlowState::SelectingProvider {
            providers: provider_choices(&self.registry),
            issue,
        });
    }

    fn recover_to_provider_selection(&mut self, message: impl Into<String>) {
        self.pending_reconnect_uid = None;
        self.reset_to_provider_selection(Some(UiIssue::new(message)));
    }

    /// Mark the flow failed (surfaced as the gallery issue chip).
    pub fn fail(&mut self, message: impl Into<String>) {
        self.set_flow(ConnectFlowState::Failed {
            issue: UiIssue::new(message),
        });
    }

    /// Open a provider: discover endpoints into the picker state, and
    /// auto-connect when the provider has exactly one endpoint and is an
    /// auto-connecting kind (BrowserWorker/HostProcess). Browser serial
    /// goes through the port-permission picker instead of discovery.
    ///
    /// The two phases below, back to back — the composition every caller
    /// that has nothing to say in between should use.
    pub async fn open_provider(
        &mut self,
        provider_id: LinkProviderKind,
    ) -> Result<DeviceOpenOutcome, UiError> {
        match self.choose_provider_endpoint(provider_id).await? {
            PortChoice::Endpoint(endpoint_id) => {
                self.connect_endpoint(provider_id, endpoint_id).await
            }
            PortChoice::Ended(outcome) => Ok(outcome),
        }
    }

    /// **Phase one** of [`Self::open_provider`]: the browser's port chooser
    /// (or endpoint discovery, for every other provider), stopping the
    /// moment an endpoint is in hand — before the multi-second connect that
    /// [`Self::connect_endpoint`] performs.
    ///
    /// The split exists so a caller can NARRATE the two apart. They took
    /// several seconds under one label, which left the setup wizard still
    /// saying "the browser is asking which port…" through the whole reset,
    /// boot wait and hello deadline (bench, 2026-08-08). Behaviour is
    /// unchanged: `open_provider` above is exactly these two, back to back.
    pub async fn choose_provider_endpoint(
        &mut self,
        provider_id: LinkProviderKind,
    ) -> Result<PortChoice, UiError> {
        if provider_id == LinkProviderKind::BrowserSerialEsp32 {
            return self.choose_browser_serial_endpoint().await;
        }

        self.discover_provider_endpoints(provider_id).await?;
        let endpoints = match &self.flow {
            ConnectFlowState::SelectingEndpoint { endpoints, .. } => endpoints.clone(),
            _ => Vec::new(),
        };
        if endpoints.len() == 1 && provider_auto_connects(provider_id) {
            return Ok(PortChoice::Endpoint(endpoints[0].id.clone()));
        }
        // Several endpoints (or a provider that does not auto-connect):
        // the picker state IS the answer, and no connect follows.
        Ok(PortChoice::Ended(DeviceOpenOutcome::Opened))
    }

    async fn discover_provider_endpoints(
        &mut self,
        provider_id: LinkProviderKind,
    ) -> Result<(), UiError> {
        self.set_flow(ConnectFlowState::DiscoveringEndpoints {
            provider_id,
            progress: ProgressState::new("Discovering endpoints"),
        });

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

        self.set_flow(ConnectFlowState::SelectingEndpoint {
            provider_id,
            endpoints: endpoints
                .into_iter()
                .map(EndpointChoice::from_endpoint)
                .collect(),
        });
        Ok(())
    }

    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    async fn choose_browser_serial_endpoint(&mut self) -> Result<PortChoice, UiError> {
        self.set_flow(ConnectFlowState::DiscoveringEndpoints {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            progress: ProgressState::new("Requesting browser serial access"),
        });

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
                return Ok(PortChoice::Ended(DeviceOpenOutcome::Cancelled { message }));
            }
            Err(error) => {
                self.recover_to_provider_selection(error.message());
                return Err(error);
            }
        };
        let endpoint_choice = EndpointChoice::from_endpoint(endpoint);
        let endpoint_id = endpoint_choice.id.clone();
        self.set_flow(ConnectFlowState::SelectingEndpoint {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            endpoints: vec![endpoint_choice],
        });
        Ok(PortChoice::Endpoint(endpoint_id))
    }

    #[cfg(not(all(feature = "browser-serial-esp32", target_arch = "wasm32")))]
    async fn choose_browser_serial_endpoint(&mut self) -> Result<PortChoice, UiError> {
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
        self.set_flow(ConnectFlowState::DiscoveringEndpoints {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            progress: ProgressState::new("Finding granted serial ports"),
        });

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
            // No grant yet: fall back to the chooser, then connect what it
            // hands over — this caller narrates the two as one connect.
            return match self.choose_browser_serial_endpoint().await? {
                PortChoice::Endpoint(endpoint_id) => {
                    self.connect_endpoint(LinkProviderKind::BrowserSerialEsp32, endpoint_id)
                        .await
                }
                PortChoice::Ended(outcome) => Ok(outcome),
            };
        };
        let endpoint_choice = EndpointChoice::from_endpoint(endpoint);
        let endpoint_id = endpoint_choice.id.clone();
        self.set_flow(ConnectFlowState::SelectingEndpoint {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            endpoints: vec![endpoint_choice],
        });
        self.connect_endpoint(LinkProviderKind::BrowserSerialEsp32, endpoint_id)
            .await
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

    /// The D7 grant sweep: enumerate the ports this origin already holds
    /// (`getPorts()`, no prompt) and run [`plan_for_granted_ports`] over
    /// them. Failures degrade to `Chooser` — a sweep that cannot even
    /// enumerate must not block the browser's own dialog. On `Adopt` /
    /// `Offer` the flow state carries the endpoints, so a follow-up
    /// [`Self::connect_endpoint`] resolves their labels.
    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    pub async fn plan_granted_ports(
        &mut self,
        strategy: crate::PortRequestStrategy,
        board_usb: Option<(u16, u16)>,
    ) -> GrantPortPlan {
        if strategy == crate::PortRequestStrategy::ChooserOnly {
            return GrantPortPlan::Chooser;
        }
        let Ok(connector) = self
            .registry
            .create_connector(LinkProviderKind::BrowserSerialEsp32)
        else {
            return GrantPortPlan::Chooser;
        };
        let LinkConnector::BrowserSerialEsp32(provider) = &*connector else {
            return GrantPortPlan::Chooser;
        };
        let Ok(granted) = provider.discover_granted_endpoints_with_usb().await else {
            return GrantPortPlan::Chooser;
        };
        let choices: Vec<EndpointChoice> = granted
            .iter()
            .map(|entry| EndpointChoice::from_endpoint(entry.endpoint.clone()))
            .collect();
        let ports = granted
            .into_iter()
            .map(|entry| GrantedPortSummary {
                endpoint_id: entry.endpoint.id.clone(),
                label: entry.endpoint.label.clone(),
                usb_vid_pid: entry.usb_vid_pid,
            })
            .collect();
        let plan = plan_for_granted_ports(strategy, board_usb, ports);
        if !matches!(plan, GrantPortPlan::Chooser) {
            self.set_flow(ConnectFlowState::SelectingEndpoint {
                provider_id: LinkProviderKind::BrowserSerialEsp32,
                endpoints: choices,
            });
        }
        plan
    }

    /// Host builds have no Web Serial: the sweep finds nothing and the
    /// request falls through to the (equally unsupported) chooser, which
    /// is today's behaviour exactly.
    #[cfg(not(all(feature = "browser-serial-esp32", target_arch = "wasm32")))]
    pub async fn plan_granted_ports(
        &mut self,
        _strategy: crate::PortRequestStrategy,
        _board_usb: Option<(u16, u16)>,
    ) -> GrantPortPlan {
        GrantPortPlan::Chooser
    }

    /// D32 auto-connect (M6): the attach sweep. Like
    /// [`Self::reconnect_granted_device`] but strictly silent — no grant
    /// means no chooser and no error (the sweep simply has nothing to
    /// do), and discovery failures reset the flow quietly. `pending_uid`
    /// is the best-effort card attribution (the most-recently-seen
    /// registered device — grants can't be mapped to identities
    /// pre-connect, so the hello reconciles the truth).
    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    pub async fn auto_connect_granted(
        &mut self,
        pending_uid: Option<String>,
    ) -> Result<DeviceOpenOutcome, UiError> {
        self.pending_reconnect_uid = pending_uid;
        self.set_flow(ConnectFlowState::DiscoveringEndpoints {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            progress: ProgressState::new("Finding granted serial ports"),
        });
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
            Err(_) => {
                // soft and silent (D32): a sweep that cannot even
                // enumerate grants leaves no trace
                self.reset_to_provider_selection(None);
                return Ok(DeviceOpenOutcome::Opened);
            }
        };
        let Some(endpoint) = endpoints.into_iter().next() else {
            self.reset_to_provider_selection(None);
            return Ok(DeviceOpenOutcome::Opened);
        };
        let endpoint_choice = EndpointChoice::from_endpoint(endpoint);
        let endpoint_id = endpoint_choice.id.clone();
        self.set_flow(ConnectFlowState::SelectingEndpoint {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            endpoints: vec![endpoint_choice],
        });
        self.connect_endpoint(LinkProviderKind::BrowserSerialEsp32, endpoint_id)
            .await
    }

    #[cfg(not(all(feature = "browser-serial-esp32", target_arch = "wasm32")))]
    pub async fn auto_connect_granted(
        &mut self,
        _pending_uid: Option<String>,
    ) -> Result<DeviceOpenOutcome, UiError> {
        // host builds have no Web Serial: the sweep is a silent no-op
        Ok(DeviceOpenOutcome::Opened)
    }

    /// Connect one endpoint: BrowserWorker becomes a [`SimAttachment`]
    /// payload; every other kind becomes a hardware [`DeviceSession`]
    /// payload (readiness is NOT awaited here — the server attach's first
    /// request drives it). The caller installs the returned payload into
    /// the pool.
    ///
    /// Hardware connects walk the RETRY LADDER (M6, the D31 replacement):
    /// try → on failure the automatic reset runs and a second, narrated
    /// attempt follows (opening a session resets the board) → on repeated
    /// failure the honest `Unresponsive` state lands on the card. A port
    /// held by another holder short-circuits to `PortHeld` (D32 soft
    /// failure — the quiet periodic retry takes over). Ladder endings are
    /// [`DeviceOpenOutcome::SoftFailed`], never errors.
    pub async fn connect_endpoint(
        &mut self,
        provider_id: LinkProviderKind,
        endpoint_id: LinkEndpointId,
    ) -> Result<DeviceOpenOutcome, UiError> {
        let endpoint = self
            .endpoint_choice(provider_id, &endpoint_id)
            .unwrap_or_else(|| EndpointChoice {
                provider_id,
                id: endpoint_id.clone(),
                label: endpoint_id.as_str().to_string(),
                summary: "Open this endpoint.".to_string(),
                status: lpa_link::LinkEndpointStatus::Available,
            });
        self.set_flow(ConnectFlowState::Connecting {
            endpoint: endpoint.clone(),
            progress: ProgressState::new("Opening link session"),
        });

        let connector = match self.registry.create_connector(provider_id) {
            Ok(connector) => connector,
            Err(error) => {
                let error = map_link_error(error);
                self.recover_to_provider_selection(error.message());
                return Err(error);
            }
        };
        let (payload, logs) = if provider_id == LinkProviderKind::BrowserWorker {
            match open_sim_attachment(connector, &endpoint_id).await {
                Ok(result) => result,
                Err(error) => {
                    self.recover_to_provider_selection(error.message());
                    return Err(error);
                }
            }
        } else {
            let mut retried = false;
            loop {
                match self
                    .connect_hardware_session(&connector, &endpoint_id, &endpoint.label)
                    .await
                {
                    Ok(result) => break result,
                    Err(error) if is_port_held_error(&error) => {
                        // D32 soft failure: the card shows In-use-
                        // elsewhere; the tick-cadence retry takes over.
                        self.set_flow(ConnectFlowState::PortHeld {
                            endpoint: endpoint.clone(),
                        });
                        return Ok(DeviceOpenOutcome::SoftFailed);
                    }
                    Err(error) if !retried => {
                        // Rung 2: the reconnect ITSELF resets the board
                        // (DTR/RTS on open) — narrate it and go again
                        // after a breath so the OS releases the port.
                        retried = true;
                        self.pending_device_logs.borrow_mut().push(UiLogDraft::new(
                            crate::UiLogLevel::Info,
                            crate::UiLogOrigin::Link,
                            format!(
                                "connect failed ({}); resetting and retrying",
                                error.message()
                            ),
                        ));
                        self.set_flow(ConnectFlowState::Retrying {
                            endpoint: endpoint.clone(),
                            progress: ProgressState::new("Resetting and retrying"),
                        });
                        self.timers.sleep(CONNECT_RETRY_BACKOFF).await;
                    }
                    Err(error) => {
                        // Ladder exhausted: the honest card state, not a
                        // toast (explicit and auto connects alike).
                        self.pending_device_logs.borrow_mut().push(UiLogDraft::new(
                            crate::UiLogLevel::Warn,
                            crate::UiLogOrigin::Link,
                            format!("device not responding: {}", error.message()),
                        ));
                        self.set_flow(ConnectFlowState::Unresponsive {
                            endpoint: endpoint.clone(),
                        });
                        return Ok(DeviceOpenOutcome::SoftFailed);
                    }
                }
            }
        };

        let session = payload
            .link_session()
            .unwrap_or_else(|| unreachable!("connect_endpoint builds live sessions only"));
        self.set_flow(ConnectFlowState::Connected {
            device: ConnectedDeviceSummary::new(
                provider_id,
                session.endpoint_id.as_str(),
                session.id().as_str(),
                endpoint.label,
            ),
        });
        Ok(DeviceOpenOutcome::Connected { payload, logs })
    }

    /// One hardware connect attempt (one ladder rung): mint the
    /// per-session console buffer, open the [`DeviceSession`].
    async fn connect_hardware_session(
        &mut self,
        connector: &Rc<LinkConnector>,
        endpoint_id: &LinkEndpointId,
        endpoint_label: &str,
    ) -> Result<(RuntimePayload, Vec<UiLogDraft>), UiError> {
        // Per-session console-log routing (runtime-pool P2): a fresh
        // buffer per attempt; the session payload carries it, and the
        // controller field aliases it for the window before the pool
        // holds the session (and for failed connects, whose captured
        // boot chatter would otherwise be lost).
        let console_logs = Rc::new(RefCell::new(Vec::new()));
        self.pending_device_logs = Rc::clone(&console_logs);
        let sink = console_event_sink(
            Rc::clone(&console_logs),
            self.events.clone(),
            Some(endpoint_id.as_str().to_string()),
        );
        match DeviceSession::connect(Rc::clone(connector), endpoint_id, self.timers.clone(), sink)
            .await
        {
            Ok(session) => {
                let connector = session.connector();
                let logs = link_session_logs(&connector, session.session().id())?;
                Ok((
                    RuntimePayload::Device(DeviceHandle::Session {
                        session,
                        console_logs,
                        endpoint_label: endpoint_label.to_string(),
                    }),
                    logs,
                ))
            }
            Err(error) => Err(map_link_error(error)),
        }
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
        self.set_flow(ConnectFlowState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });
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

    fn granted(endpoint: &str, label: &str, usb: Option<(u16, u16)>) -> GrantedPortSummary {
        GrantedPortSummary {
            endpoint_id: LinkEndpointId::new(endpoint),
            label: label.to_string(),
            usb_vid_pid: usb,
        }
    }

    const CH340: (u16, u16) = (0x1A86, 0x7523);
    const FTDI: (u16, u16) = (0x0403, 0x6001);

    #[test]
    fn the_grant_ladder_matches_d7() {
        use crate::PortRequestStrategy as S;
        let one = || vec![granted("p1", "Serial device (1a86:7523)", Some(CH340))];
        let two = || {
            vec![
                granted("p1", "Serial device (1a86:7523)", Some(CH340)),
                granted("p2", "Serial device (0403:6001)", Some(FTDI)),
            ]
        };

        // 0 grants → the chooser, exactly as today.
        assert_eq!(
            plan_for_granted_ports(S::AutoAdopt, None, Vec::new()),
            GrantPortPlan::Chooser
        );
        // 1 grant → adopt, visibly (the caller narrates the label).
        assert_eq!(
            plan_for_granted_ports(S::AutoAdopt, None, one()),
            GrantPortPlan::Adopt {
                endpoint_id: LinkEndpointId::new("p1"),
                label: "Serial device (1a86:7523)".to_string(),
            }
        );
        // several → never guess.
        assert_eq!(
            plan_for_granted_ports(S::AutoAdopt, None, two()),
            GrantPortPlan::Offer { ports: two() }
        );
        // The chooser can always be demanded by name.
        assert_eq!(
            plan_for_granted_ports(S::ChooserOnly, None, two()),
            GrantPortPlan::Chooser
        );
        // A return trip lists even a single grant: re-adopting the port
        // that just disappointed is the one silent move D7 forbids, and
        // a list stays clickable for a driving agent.
        assert_eq!(
            plan_for_granted_ports(S::ListOnly, None, one()),
            GrantPortPlan::Offer { ports: one() }
        );
    }

    #[test]
    fn the_board_first_filter_narrows_before_the_ladder_judges() {
        use crate::PortRequestStrategy as S;
        let mixed = vec![
            granted("p1", "Serial device (1a86:7523)", Some(CH340)),
            granted("p2", "Serial device (0403:6001)", Some(FTDI)),
        ];
        // One match among unrelated grants → adopt it (D7's refinement).
        assert_eq!(
            plan_for_granted_ports(S::AutoAdopt, Some(CH340), mixed.clone()),
            GrantPortPlan::Adopt {
                endpoint_id: LinkEndpointId::new("p1"),
                label: "Serial device (1a86:7523)".to_string(),
            }
        );
        // Zero matches → only the chooser can grant the board's own port.
        assert_eq!(
            plan_for_granted_ports(S::AutoAdopt, Some((0x10C4, 0xEA60)), mixed.clone()),
            GrantPortPlan::Chooser
        );
        // Two identical bridges → indistinguishable until opened: offer
        // the matches, never a guess.
        let twins = vec![
            granted("p1", "Serial device (1a86:7523)", Some(CH340)),
            granted("p2", "Serial device (1a86:7523)", Some(CH340)),
            granted("p3", "Serial device (0403:6001)", Some(FTDI)),
        ];
        assert_eq!(
            plan_for_granted_ports(S::AutoAdopt, Some(CH340), twins.clone()),
            GrantPortPlan::Offer {
                ports: twins[..2].to_vec()
            }
        );
    }

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
        #[allow(
            unreachable_patterns,
            reason = "providers beyond Fake are feature/target-gated, so the \
                      wildcard arm is unreachable in some test configurations"
        )]
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
        // would fail with endpoint-not-found instead. Under the M6 ladder
        // the armed error ends SOFT (Unresponsive), with the error text
        // preserved in the connect's console drafts.
        assert!(matches!(result, Ok(DeviceOpenOutcome::SoftFailed)));
        assert!(matches!(
            device.flow_state(),
            ConnectFlowState::Unresponsive { .. }
        ));
        assert!(
            device
                .take_pending_device_logs()
                .iter()
                .any(|draft| draft.message.contains("armed on the shared instance"))
        );
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
    fn failed_connection_handoff_walks_the_ladder_to_unresponsive() {
        let mut device = DeviceController::with_registry(registry_with_fake(
            FakeProvider::new()
                .with_endpoint(fake_endpoint())
                .with_connection_error("server handoff failed"),
        ));

        let result = block_on_ready(
            device.connect_endpoint(LinkProviderKind::Fake, LinkEndpointId::new("fake-runtime")),
        );

        // M6: hardware connect failures end SOFT on the card, not as an
        // error — the ladder retried once (reset rides the reconnect).
        assert!(matches!(result, Ok(DeviceOpenOutcome::SoftFailed)));
        assert!(matches!(
            device.flow_state(),
            ConnectFlowState::Unresponsive { .. }
        ));
        // the error text survives in the connect's console drafts
        assert!(
            device
                .take_pending_device_logs()
                .iter()
                .any(|draft| draft.message.contains("server handoff failed"))
        );
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
