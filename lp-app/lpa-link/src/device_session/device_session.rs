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
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lpc_wire::{ServerHello, TransportError, WireServerMessage};

use crate::provider::endpoint::LinkEndpointId;
use crate::{
    LinkConnection, LinkConnector, LinkError, LinkProvider, LinkServerConnection, LinkSession,
    LinkSessionStatus,
};

use super::device_client_io::DeviceClientIo;
use super::device_event::{DeviceEvent, DeviceEventSink};
use super::device_mode::{ChannelUseGuard, DeviceMode, DeviceModeGuard};
use super::device_readiness::{BootLineClassifier, HelloGate, gate_first_frame};
use super::device_snapshot::DeviceSnapshot;
use super::device_state::{DeviceState, IncompatibleReason};
use super::device_timers::{DeviceTimers, READINESS_POLL_INTERVAL};

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
        let (session, connection, transport) = match opened {
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
            transport: RefCell::new(transport),
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

    /// The connector this session owns (for P3 management operations).
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
/// `session`/`connection`/`transport` sit behind `RefCell`s because a
/// rebuild ([`Self::rebuild_link`]) swaps the whole underlying link in
/// place: channel clones read the CURRENT transport through the accessor on
/// every use, so a channel handed out before a management/reconnect cycle
/// works again after it.
pub(crate) struct DeviceShared {
    connector: Rc<LinkConnector>,
    session: RefCell<LinkSession>,
    connection: RefCell<LinkConnection>,
    transport: RefCell<LinkServerConnection>,
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

    pub(crate) fn transport(&self) -> LinkServerConnection {
        self.transport.borrow().clone()
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
    /// free for a management tool). Best-effort — the link may already be
    /// dead when this runs (Gone recovery).
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
        let (session, connection, transport) = match opened {
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
        *self.transport.borrow_mut() = transport;
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

    /// Drain non-protocol serial lines: classifier feed + console feed.
    /// Protocol (`M!`) lines are skipped — decoded frames arrive through the
    /// transport.
    pub(crate) fn pump_console_lines(&self) {
        for line in self.take_observed_lines() {
            if line.starts_with("M!") {
                continue;
            }
            self.classifier.borrow_mut().observe_line(line.as_str());
            self.sink.emit(DeviceEvent::LogLine { line });
        }
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

    /// Non-blocking poll for one decoded protocol frame from the transport.
    ///
    /// `None` means "no frame yet". A single poll with a no-op waker is
    /// sound here because the transport receive is a cancel-safe channel
    /// recv: an uncompleted poll consumes nothing, and the engine re-polls
    /// on its next pass. (Same bridge technique as the fake device's
    /// server pump; duplicated because that one is feature-private.)
    fn poll_frame(&self) -> Option<Result<WireServerMessage, TransportError>> {
        let transport = self.transport();
        poll_once(async move {
            let mut transport = transport.lock().await;
            lpa_client::ClientTransport::receive(&mut **transport).await
        })
    }

    /// Serial lines observed by connectors that surface them (the fake
    /// device today; the browser serial provider joins in P5 — host serial
    /// grows a line surface with it).
    fn take_observed_lines(&self) -> Vec<String> {
        #[cfg(feature = "fake-device")]
        #[allow(
            irrefutable_let_patterns,
            reason = "connector variants are feature-gated; with fake-device alone the enum has one variant"
        )]
        if let LinkConnector::Fake(provider) = &*self.connector {
            let session_id = self.session.borrow().id.clone();
            return provider.take_lines(&session_id).unwrap_or_default();
        }
        Vec::new()
    }
}

/// Open one device link: connector connect → provider protocol open →
/// connection handoff. The canonical home of the flow that lived in the
/// studio `LinkController`'s `open_connected_provider` (the studio copy
/// remains until P5/P6 rewire and delete it).
async fn open_device_link(
    connector: &LinkConnector,
    endpoint_id: &LinkEndpointId,
) -> Result<(LinkSession, LinkConnection, LinkServerConnection), LinkError> {
    let session = connector.connect(endpoint_id).await?;
    // Provider protocol open is a browser-serial concern (baud-rate open);
    // it moves here with the browser connector in P5. Host connectors open
    // their protocol inside connect().
    let connection = match connector.connection(session.id()).await {
        Ok(connection) => connection,
        Err(error) => {
            close_failed_session(connector, &session).await;
            return Err(error);
        }
    };
    let Some(transport) = connection.server_connection() else {
        close_failed_session(connector, &session).await;
        return Err(LinkError::other(
            "link connection exposes no host protocol channel; DeviceSession is hardware-only",
        ));
    };
    Ok((session, connection, transport))
}

async fn close_failed_session(connector: &LinkConnector, session: &LinkSession) {
    let _ = connector.close(session.id()).await;
}

/// Poll a future exactly once with a no-op waker; `None` when pending.
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
