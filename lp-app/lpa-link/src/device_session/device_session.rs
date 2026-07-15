//! The `DeviceSession` type: one owned hardware link, end to end.
//!
//! Construction ([`DeviceSession::connect`]) performs the connector connect,
//! any provider protocol open, and the connection handoff, then enters
//! [`DeviceState::Booting`]. Readiness is driven ON DEMAND — there is no
//! background task: `wait_ready()` or the handed-out channel's first use
//! runs the readiness engine inside the caller's own poll, consistent with
//! the single-actor world on both wasm and host.
//!
//! RefCell discipline: every interior borrow is scoped to a synchronous
//! section; the only awaits are injected timers and the (tokio-`Mutex`d)
//! transport, and no `RefCell` borrow is held across either.

use std::cell::{Cell, RefCell};
#[cfg(feature = "device-session-host")]
use std::future::Future;
use std::rc::Rc;
#[cfg(feature = "device-session-host")]
use std::sync::Arc;
#[cfg(feature = "device-session-host")]
use std::task::{Context, Poll, Wake, Waker};

use lpc_wire::{ServerHello, TransportError, WireServerMessage};

use crate::provider::endpoint::LinkEndpointId;
use crate::{
    LinkConnection, LinkConnector, LinkError, LinkProvider, LinkSession, LinkSessionStatus,
};

use super::device_client_io::DeviceClientIo;
use super::device_event::{DeviceEvent, DeviceEventSink, DeviceLineOrigin};
use super::device_mode::{ChannelUseGuard, DeviceMode, DeviceModeGuard};
use super::device_readiness::{BootLineClassifier, HelloGate, gate_first_frame};
use super::device_snapshot::DeviceSnapshot;
use super::device_state::{DeviceState, IncompatibleReason};
use super::device_timers::{DeviceTimers, READINESS_POLL_INTERVAL};
use super::device_wire::DeviceWire;

/// One owned hardware device link: connector + link session/connection +
/// state machine + the readiness-gated app-protocol channel.
pub struct DeviceSession {
    pub(super) shared: Rc<DeviceShared>,
}

impl DeviceSession {
    /// Open the device link and enter [`DeviceState::Booting`].
    ///
    /// Performs the connector `connect`, provider protocol open, and
    /// connection handoff under the `connect` deadline. Readiness is NOT
    /// awaited here — call [`Self::wait_ready`] or just use the channel.
    pub async fn connect(
        connector: Rc<LinkConnector>,
        endpoint_id: &LinkEndpointId,
        timers: DeviceTimers,
        sink: DeviceEventSink,
    ) -> Result<Self, LinkError> {
        let budget = timers.deadlines().connect;
        let opened = timers
            .with_deadline(budget, open_device_link(&connector, endpoint_id))
            .await;
        let (session, connection, wire) = match opened {
            Some(result) => result?,
            None => {
                return Err(LinkError::ConnectionFailed {
                    message: format!(
                        "timed out opening the device link after {:.1}s",
                        budget.as_secs_f64()
                    ),
                });
            }
        };
        let shared = Rc::new(DeviceShared {
            connector,
            session: RefCell::new(session),
            connection: RefCell::new(connection),
            wire: RefCell::new(wire),
            timers,
            sink,
            state: RefCell::new(DeviceState::Booting),
            mode: Rc::new(Cell::new(DeviceMode::AppProtocol)),
            channel_busy: Cell::new(0),
            classifier: RefCell::new(BootLineClassifier::new()),
        });
        shared.sink.emit(DeviceEvent::State {
            state: DeviceState::Booting,
        });
        Ok(Self { shared })
    }

    /// Current state (cloned).
    pub fn state(&self) -> DeviceState {
        self.shared.state()
    }

    /// The hello payload, when the session reached [`DeviceState::Ready`].
    pub fn hello(&self) -> Option<ServerHello> {
        self.shared.state.borrow().hello().cloned()
    }

    /// Point-in-time snapshot for pull-model consumers.
    pub fn snapshot(&self) -> DeviceSnapshot {
        let state = self.shared.state();
        let session = self.shared.session.borrow().clone();
        DeviceSnapshot {
            endpoint_status: DeviceSnapshot::derive_endpoint_status(&state, &session),
            state,
            session,
            recent_lines: self.shared.classifier.borrow().recent_lines().to_vec(),
        }
    }

    /// Drive the readiness engine until the state leaves
    /// [`DeviceState::Booting`] or the `ready` deadline expires, and return
    /// where it landed. Idempotent: outside `Booting` this returns
    /// immediately.
    pub async fn wait_ready(&self) -> DeviceState {
        self.shared.drive_readiness().await
    }

    /// The readiness-gated app-protocol channel.
    ///
    /// The first `send` drives readiness if it has not run yet; every use is
    /// gated on `Ready` + [`DeviceMode::AppProtocol`] and errors cleanly
    /// otherwise (nothing is ever written to a device that is not ready).
    pub fn client_io(&self) -> Box<dyn lpa_client::ClientIo> {
        Box::new(DeviceClientIo::new(Rc::clone(&self.shared)))
    }

    /// Current wire ownership mode.
    pub fn mode(&self) -> DeviceMode {
        self.shared.mode.get()
    }

    /// Take exclusive management ownership of the wire ([`Self::manage`]'s
    /// entry point). While the returned guard lives, the app-protocol
    /// channel errors cleanly; dropping it releases the wire.
    ///
    /// Refused when a management operation already holds the wire OR an
    /// app-protocol request is mid-flight on the channel (the wire cannot
    /// be torn down under a request).
    pub fn try_begin_management(&self) -> Result<DeviceModeGuard, LinkError> {
        if self.shared.mode.get() != DeviceMode::AppProtocol {
            return Err(LinkError::other(
                "a device management operation is already in progress",
            ));
        }
        if self.shared.channel_busy.get() > 0 {
            return Err(LinkError::other(
                "an app-protocol request is in flight on the device wire",
            ));
        }
        self.shared.mode.set(DeviceMode::Management);
        Ok(DeviceModeGuard::new(Rc::clone(&self.shared.mode)))
    }

    /// The connector this session owns. Consumers use it for
    /// connector-level metadata and log surfaces (labels, capabilities,
    /// session logs); the wire itself is only reachable through the
    /// session's own gated surfaces.
    pub fn connector(&self) -> Rc<LinkConnector> {
        Rc::clone(&self.shared.connector)
    }

    /// The underlying link session record, including its live status.
    pub fn session(&self) -> LinkSession {
        self.shared.session.borrow().clone()
    }

    /// The link connection handoff record.
    pub fn connection(&self) -> LinkConnection {
        self.shared.connection.borrow().clone()
    }

    /// Close the link session cleanly: the state becomes
    /// [`DeviceState::Gone`] and the session record `Closed`.
    pub async fn close(self) -> Result<(), LinkError> {
        let session_id = self.shared.session.borrow().id.clone();
        // Mark closed BEFORE the await so channel clones observe Gone
        // immediately and the status is not upgraded to Error by set_state.
        self.shared.session.borrow_mut().status = LinkSessionStatus::Closed;
        self.shared.set_state(DeviceState::Gone);
        self.shared.connector.close(&session_id).await
    }
}

/// State shared between the session handle and its channel clones.
///
/// `session`/`connection`/`wire` sit behind `RefCell`s because a rebuild
/// ([`Self::rebuild_link`]) swaps the whole underlying link in place:
/// channel clones read the CURRENT wire through the accessors on every use,
/// so a channel handed out before a management/reconnect cycle works again
/// after it.
pub(crate) struct DeviceShared {
    connector: Rc<LinkConnector>,
    session: RefCell<LinkSession>,
    connection: RefCell<LinkConnection>,
    wire: RefCell<DeviceWire>,
    timers: DeviceTimers,
    sink: DeviceEventSink,
    state: RefCell<DeviceState>,
    mode: Rc<Cell<DeviceMode>>,
    /// App-protocol requests currently inside a transport send/receive.
    /// Management refuses to take the wire while this is nonzero.
    channel_busy: Cell<u32>,
    classifier: RefCell<BootLineClassifier>,
}

impl DeviceShared {
    pub(crate) fn state(&self) -> DeviceState {
        self.state.borrow().clone()
    }

    pub(crate) fn timers(&self) -> &DeviceTimers {
        &self.timers
    }

    pub(super) fn connector(&self) -> &Rc<LinkConnector> {
        &self.connector
    }

    pub(super) fn session_id(&self) -> crate::LinkSessionId {
        self.session.borrow().id.clone()
    }

    /// Mark one app-protocol request as in flight for the guard's lifetime.
    pub(crate) fn begin_channel_use(&self) -> ChannelUseGuard<'_> {
        ChannelUseGuard::new(&self.channel_busy)
    }

    /// Release the current link: close the provider session so its transport
    /// shuts down (the host serial framing thread ENDS and the port is
    /// free for a management tool; the browser provider closes its Web
    /// Serial port). Best-effort — the link may already be dead when this
    /// runs (Gone recovery).
    pub(super) async fn release_link(&self) {
        let session_id = self.session_id();
        let _ = self.connector.close(&session_id).await;
    }

    /// Reconnect = rebuild: open a NEW link (fresh provider session, fresh
    /// transport/serial thread) on the same endpoint, swap it in for the old
    /// one, clear the observed-line state, and re-enter `Booting`.
    ///
    /// The state machine's terminal states stay sticky under passive
    /// observation; this is the one deliberate way out — each rebuild starts
    /// a new link generation whose readiness runs from scratch.
    pub(super) async fn rebuild_link(&self) -> Result<(), LinkError> {
        let endpoint_id = self.session.borrow().endpoint_id.clone();
        let budget = self.timers.deadlines().connect;
        let opened = self
            .timers
            .with_deadline(budget, open_device_link(&self.connector, &endpoint_id))
            .await;
        let (session, connection, wire) = match opened {
            Some(Ok(opened)) => opened,
            Some(Err(error)) => {
                self.record_link_failure(&format!("device link rebuild failed: {error}"));
                return Err(error);
            }
            None => {
                let message = format!(
                    "timed out reopening the device link after {:.1}s",
                    budget.as_secs_f64()
                );
                self.record_link_failure(&message);
                return Err(LinkError::ConnectionFailed { message });
            }
        };
        *self.session.borrow_mut() = session;
        *self.connection.borrow_mut() = connection;
        *self.wire.borrow_mut() = wire;
        // Clear the observed-line state: stale lines from the previous
        // link's boot must not classify the new one (the M3 lesson — old
        // blank-flash lines would misclassify a post-flash reconnect).
        *self.classifier.borrow_mut() = BootLineClassifier::new();
        self.set_state(DeviceState::Booting);
        Ok(())
    }

    /// Record a failed management/rebuild: the session record's status
    /// carries the message and the state lands on `Gone` (the wire was
    /// released; only [`DeviceSession::reconnect`] re-arms it).
    pub(super) fn record_link_failure(&self, message: &str) {
        {
            let mut session = self.session.borrow_mut();
            if session.status == LinkSessionStatus::Open {
                session.status = LinkSessionStatus::Error {
                    message: message.to_string(),
                };
            }
        }
        self.set_state(DeviceState::Gone);
    }

    /// Gate for the app-protocol channel: drive readiness if it has not
    /// completed, then require `Ready` + `AppProtocol`.
    pub(crate) async fn ensure_app_protocol(&self) -> Result<(), TransportError> {
        if matches!(self.state(), DeviceState::Booting) {
            self.drive_readiness().await;
        }
        if self.mode.get() != DeviceMode::AppProtocol {
            return Err(TransportError::Other(
                "device wire is owned by a management operation".to_string(),
            ));
        }
        match self.state().unavailable_message() {
            None => Ok(()),
            Some(message) => Err(TransportError::Other(message)),
        }
    }

    /// The readiness engine: pump lines + frames every poll interval until
    /// the state leaves `Booting` or the `ready` deadline expires.
    pub(crate) async fn drive_readiness(&self) -> DeviceState {
        if !matches!(self.state(), DeviceState::Booting) {
            return self.state();
        }
        let interval = READINESS_POLL_INTERVAL;
        let budget = self.timers.deadlines().ready;
        let attempts = (budget.as_micros() / interval.as_micros().max(1)).max(1);
        for _ in 0..attempts {
            self.pump();
            let state = self.state();
            if !matches!(state, DeviceState::Booting) {
                return state;
            }
            self.timers.sleep(interval).await;
        }
        self.on_ready_deadline();
        self.state()
    }

    /// One synchronous readiness pass: drain observed lines into the
    /// classifier/console feed, gate protocol frames on the hello, then
    /// fail fast on a no-firmware diagnosis.
    fn pump(&self) {
        self.pump_console_lines();

        // Frames: the hello gate. Only consumed while Booting — once ready,
        // frames belong to the app-protocol channel.
        while matches!(self.state(), DeviceState::Booting) {
            match self.poll_frame() {
                None => break,
                Some(Err(_error)) => {
                    self.mark_gone("device stream ended before the session became ready");
                    return;
                }
                Some(Ok(frame)) => match gate_first_frame(frame) {
                    HelloGate::Ready(hello) => {
                        self.set_state(DeviceState::Ready { hello });
                        return;
                    }
                    HelloGate::Incompatible(reason) => {
                        self.set_state(DeviceState::Incompatible { reason });
                        return;
                    }
                },
            }
        }

        // No hello yet: a no-firmware boot signature is terminal enough to
        // fail fast instead of burning the whole deadline.
        if matches!(self.state(), DeviceState::Booting) {
            let reason = {
                let classifier = self.classifier.borrow();
                classifier
                    .no_firmware_detected()
                    .then(|| classifier.no_firmware_reason())
            };
            if let Some(reason) = reason {
                use super::device_readiness::NoFirmwareReason;
                let state = match reason {
                    NoFirmwareReason::BlankOrErasedFlash => DeviceState::BlankFlash,
                    NoFirmwareReason::RomDownloadMode => DeviceState::Bootloader,
                    NoFirmwareReason::SafeToReplaceFirmware => DeviceState::ForeignFirmware,
                };
                self.set_state(state);
            }
        }
    }

    /// Drain observed serial lines: classifier feed + console feed.
    ///
    /// `M!` protocol lines depend on the wire: on the host wires they are a
    /// tap COPY (decoded frames arrive through the transport) and are
    /// dropped; on the browser line wire they ARE the frames and are decoded
    /// into the pending queue.
    pub(crate) fn pump_console_lines(&self) {
        for line in self.take_observed_lines() {
            if let Some(_frame_json) = line.strip_prefix("M!") {
                #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
                self.queue_browser_frame(_frame_json);
                continue;
            }
            self.classifier.borrow_mut().observe_line(line.as_str());
            self.sink.emit(DeviceEvent::LogLine {
                line,
                origin: DeviceLineOrigin::Device,
            });
        }
        #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
        self.surface_browser_errors();
    }

    /// Deadline expiry while still `Booting`: a started-but-silent server is
    /// pre-hello firmware (`Incompatible`); anything else is `Unresponsive`
    /// with the classifier's diagnosis.
    fn on_ready_deadline(&self) {
        let (server_started, diagnosis) = {
            let classifier = self.classifier.borrow();
            (classifier.server_started(), classifier.classify_timeout())
        };
        if server_started {
            self.set_state(DeviceState::Incompatible {
                reason: IncompatibleReason::NoHello,
            });
        } else {
            self.set_state(DeviceState::Unresponsive { diagnosis });
        }
    }

    /// Record an unexpected end of the device stream.
    pub(crate) fn mark_gone(&self, message: &str) {
        if matches!(self.state(), DeviceState::Gone) {
            return;
        }
        {
            let mut session = self.session.borrow_mut();
            if session.status == LinkSessionStatus::Open {
                session.status = LinkSessionStatus::Error {
                    message: message.to_string(),
                };
            }
        }
        self.set_state(DeviceState::Gone);
    }

    /// Transition the state machine, keep the link session record's status
    /// vocabulary in sync, and notify the sink.
    fn set_state(&self, next: DeviceState) {
        let failure_message = match &next {
            DeviceState::Incompatible { reason } => Some(reason.message()),
            DeviceState::Unresponsive { diagnosis } => Some(diagnosis.message()),
            DeviceState::Gone => next.unavailable_message(),
            _ => None,
        };
        if let Some(message) = failure_message {
            let mut session = self.session.borrow_mut();
            if session.status == LinkSessionStatus::Open {
                session.status = LinkSessionStatus::Error { message };
            }
        }
        *self.state.borrow_mut() = next.clone();
        self.sink.emit(DeviceEvent::State { state: next });
    }

    /// Send one app-protocol frame on the current wire.
    pub(crate) async fn send_frame(
        &self,
        msg: lpc_wire::ClientMessage,
    ) -> Result<(), TransportError> {
        #[cfg(feature = "device-session-host")]
        if let Some(transport) = self.host_transport() {
            let mut transport = transport.lock().await;
            return lpa_client::ClientTransport::send(&mut **transport, msg).await;
        }
        #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
        if self.is_browser_wire() {
            return self.browser_write_frame(&msg).await;
        }
        let _ = msg;
        Err(TransportError::Other(
            "device session has no app-protocol wire".to_string(),
        ))
    }

    /// Wait for one app-protocol frame on the current wire (no deadline —
    /// the caller wraps this in the `request_idle` budget).
    pub(crate) async fn recv_frame(&self) -> Result<WireServerMessage, TransportError> {
        #[cfg(feature = "device-session-host")]
        if let Some(transport) = self.host_transport() {
            let mut transport = transport.lock().await;
            return lpa_client::ClientTransport::receive(&mut **transport).await;
        }
        #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
        if self.is_browser_wire() {
            loop {
                self.pump_console_lines();
                if let Some(frame) = self.pop_browser_frame() {
                    return Ok(frame);
                }
                if let Some(message) = self.state().unavailable_message() {
                    // pump_console_lines surfaced a serial error → Gone
                    return Err(TransportError::Other(message));
                }
                self.timers.sleep(READINESS_POLL_INTERVAL).await;
            }
        }
        Err(TransportError::Other(
            "device session has no app-protocol wire".to_string(),
        ))
    }

    /// Close the current wire (the channel-facing close; the SESSION close
    /// releases the provider resources).
    pub(crate) async fn close_wire(&self) -> Result<(), TransportError> {
        #[cfg(feature = "device-session-host")]
        if let Some(transport) = self.host_transport() {
            let mut transport = transport.lock().await;
            return lpa_client::ClientTransport::close(&mut **transport).await;
        }
        // Browser line wire: the provider session owns the port; nothing to
        // close at the channel level.
        Ok(())
    }

    #[cfg(feature = "device-session-host")]
    fn host_transport(&self) -> Option<crate::LinkServerConnection> {
        match &*self.wire.borrow() {
            DeviceWire::Transport(transport) => Some(transport.clone()),
            #[allow(
                unreachable_patterns,
                reason = "wire variants are feature-gated; host-only builds have one variant"
            )]
            _ => None,
        }
    }

    /// Non-blocking poll for one decoded protocol frame from the wire.
    ///
    /// `None` means "no frame yet". For the host transport a single poll
    /// with a no-op waker is sound because the transport receive is a
    /// cancel-safe channel recv: an uncompleted poll consumes nothing, and
    /// the engine re-polls on its next pass. (Same bridge technique as the
    /// fake device's server pump; duplicated because that one is
    /// feature-private.) The browser line wire pops the pending queue that
    /// [`Self::pump_console_lines`] fills.
    fn poll_frame(&self) -> Option<Result<WireServerMessage, TransportError>> {
        #[cfg(feature = "device-session-host")]
        if let Some(transport) = self.host_transport() {
            return poll_once(async move {
                let mut transport = transport.lock().await;
                lpa_client::ClientTransport::receive(&mut **transport).await
            });
        }
        #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
        if self.is_browser_wire() {
            return self.pop_browser_frame().map(Ok);
        }
        None
    }

    /// Serial lines observed by connectors that surface them: the fake
    /// device and host serial providers tap the framing thread's line
    /// splitter; the browser serial provider's JS controller splits lines
    /// natively.
    fn take_observed_lines(&self) -> Vec<String> {
        // Underscore-named: unused in feature combinations where every
        // line-surfacing connector arm below is cfg'd out.
        let _session_id = || self.session.borrow().id.clone();
        #[allow(
            unreachable_patterns,
            irrefutable_let_patterns,
            reason = "connector variants are feature-gated; some builds have one variant"
        )]
        match &*self.connector {
            #[cfg(feature = "fake-device")]
            LinkConnector::Fake(provider) => {
                provider.take_lines(&_session_id()).unwrap_or_default()
            }
            #[cfg(feature = "host-serial-esp32")]
            LinkConnector::HostSerialEsp32(provider) => {
                provider.take_lines(&_session_id()).unwrap_or_default()
            }
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            LinkConnector::BrowserSerialEsp32(provider) => {
                provider.take_lines(&_session_id()).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }
}

/// Browser line-wire internals (wasm only): `M!` lines are the protocol
/// frames, decoded here into the wire's pending queue; serial errors from
/// the JS controller surface as `Gone`.
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
impl DeviceShared {
    fn is_browser_wire(&self) -> bool {
        matches!(&*self.wire.borrow(), DeviceWire::BrowserLines { .. })
    }

    /// Decode one `M!` frame body into the pending queue. A malformed frame
    /// is reported as a link log line; when another `M!` marker is embedded
    /// (interleaved device output corrupted the line) decoding resyncs at
    /// it, mirroring the retired studio browser io.
    fn queue_browser_frame(&self, frame_json: &str) {
        match lpc_wire::json::from_str::<WireServerMessage>(frame_json) {
            Ok(frame) => {
                #[allow(
                    irrefutable_let_patterns,
                    reason = "wire variants are feature-gated; wasm-only builds have one variant"
                )]
                if let DeviceWire::BrowserLines { pending } = &mut *self.wire.borrow_mut() {
                    pending.push_back(frame);
                }
            }
            Err(error) => {
                self.sink.emit(DeviceEvent::LogLine {
                    line: format!("malformed M! frame: {error}"),
                    origin: DeviceLineOrigin::Link,
                });
                if let Some(offset) = frame_json.find("M!").filter(|offset| *offset > 0) {
                    self.queue_browser_frame(&frame_json[offset + 2..]);
                }
            }
        }
    }

    fn pop_browser_frame(&self) -> Option<WireServerMessage> {
        match &mut *self.wire.borrow_mut() {
            DeviceWire::BrowserLines { pending } => pending.pop_front(),
            #[allow(
                unreachable_patterns,
                reason = "wire variants are feature-gated; wasm-only builds have one variant"
            )]
            _ => None,
        }
    }

    /// Surface JS serial controller errors: the port died underneath us.
    fn surface_browser_errors(&self) {
        let LinkConnector::BrowserSerialEsp32(provider) = &*self.connector else {
            return;
        };
        let session_id = self.session.borrow().id.clone();
        let Ok(errors) = provider.take_errors(&session_id) else {
            return;
        };
        for error in errors {
            self.sink.emit(DeviceEvent::LogLine {
                line: format!("browser serial error: {error}"),
                origin: DeviceLineOrigin::Link,
            });
            self.mark_gone(&format!("browser serial error: {error}"));
        }
    }

    async fn browser_write_frame(
        &self,
        msg: &lpc_wire::ClientMessage,
    ) -> Result<(), TransportError> {
        let frame = lpc_wire::json::to_string(msg)
            .map_err(|error| TransportError::Serialization(error.to_string()))?;
        let session_id = self.session.borrow().id.clone();
        let LinkConnector::BrowserSerialEsp32(provider) = &*self.connector else {
            return Err(TransportError::Other(
                "browser line wire holds a non-browser connector".to_string(),
            ));
        };
        provider
            .write_line(&session_id, &format!("M!{frame}\n"))
            .await
            .map_err(|error| TransportError::Other(error.to_string()))
    }
}

/// Open one device link: connector connect → provider protocol open →
/// connection handoff → wire selection. The canonical home of the studio's
/// pre-M4 open-connected-provider flow.
async fn open_device_link(
    connector: &LinkConnector,
    endpoint_id: &LinkEndpointId,
) -> Result<(LinkSession, LinkConnection, DeviceWire), LinkError> {
    let session = connector.connect(endpoint_id).await?;
    // Browser serial: the provider needs an explicit protocol open at the
    // app baud rate before lines flow. Host connectors open their protocol
    // inside connect().
    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    if let LinkConnector::BrowserSerialEsp32(provider) = connector {
        if let Err(error) = provider
            .open_protocol(session.id(), lpc_model::DEFAULT_SERIAL_BAUD_RATE)
            .await
        {
            close_failed_session(connector, &session).await;
            return Err(error);
        }
    }
    let connection = match connector.connection(session.id()).await {
        Ok(connection) => connection,
        Err(error) => {
            close_failed_session(connector, &session).await;
            return Err(error);
        }
    };
    #[cfg(feature = "device-session-host")]
    if let Some(transport) = connection.server_connection() {
        return Ok((session, connection, DeviceWire::Transport(transport)));
    }
    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    if matches!(connector, LinkConnector::BrowserSerialEsp32(_)) {
        return Ok((
            session,
            connection,
            DeviceWire::BrowserLines {
                pending: std::collections::VecDeque::new(),
            },
        ));
    }
    let _ = &connection;
    close_failed_session(connector, &session).await;
    Err(LinkError::other(
        "link connection exposes no host protocol channel; DeviceSession is hardware-only",
    ))
}

async fn close_failed_session(connector: &LinkConnector, session: &LinkSession) {
    let _ = connector.close(session.id()).await;
}

/// Poll a future exactly once with a no-op waker; `None` when pending.
#[cfg(feature = "device-session-host")]
fn poll_once<F: Future>(future: F) -> Option<F::Output> {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => Some(output),
        Poll::Pending => None,
    }
}
