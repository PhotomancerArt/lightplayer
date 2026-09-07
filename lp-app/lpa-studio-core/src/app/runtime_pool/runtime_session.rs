//! One attached runtime: its payload, wire client, and per-session state.
//!
//! A [`RuntimeSession`] bundles: the runtime attachment (the payload), the
//! wire client it owns, the server protocol state, the per-session console
//! tail, and this session's refresh/heartbeat pacing. The card's ▶ feed is
//! the roster's lane, not this one's (PD9).
//!
//! **There is exactly ONE payload** (PD9, "always a device"): the editor is
//! a LENS on a roster device, and a sim is a roster device like any other.
//! D22's "the sim is not a device" rule — which used to live in this file's
//! type system as a second arm — is retired by the ADR
//! `2026-09-07-always-a-device-target-real-emu-sim`.
//!
//! The device itself lives in the `lpa-devices` roster: its identity,
//! evidence, activities and link all stay there. This payload is only the
//! lens's handle on that device — which roster device, which link the lens
//! borrowed, how that link is reached ([`LinkTransport`]), and the facts
//! the editor needs at attach (uid, build features). The old per-device
//! reconcile bundle (`device_sync`, `hardware_id`, drift times, the
//! in-flight `operation` flag) is NOT back: those were parallel stores
//! (invariant I8), and the fold owns their facts now.
//!
//! There is no `None` payload: absence of a runtime is absence from the
//! [`RuntimePool`](super::RuntimePool).

use core::time::Duration;
use std::collections::VecDeque;

use lpa_client::BackoffPolicy;

use crate::app::studio::refresh_cadence::{
    DEVICE_HEARTBEAT_INTERVAL, PASSIVE_REFRESH_BACKOFF_BASE, PASSIVE_REFRESH_BACKOFF_MAX,
    REFRESH_DUE_SLACK, RefreshCadence,
};
use crate::{
    RuntimeId, ServerFailureKind, ServerState, StudioServerClient, UiError, UiIssue, UiLogDraft,
    UiLogEntry, UiLogLevel,
};

/// How many stamped lines the per-session console tail retains (D42: the
/// card's console is a bounded ring, not the full history).
pub const CONSOLE_TAIL_LEN: usize = 40;

/// How the lens device's link is reached.
///
/// Not a kind of device — every device is a device (PD9) — but a fact about
/// the WIRE, and the only thing the editor's pacing and probe policy have
/// ever actually forked on: an in-process worker channel has no bandwidth
/// bound, a serial port does. Derived from the link's endpoint at attach,
/// never stored twice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkTransport {
    /// A sim's worker channel (`sim:<uid>`): in-process, free.
    Sim,
    /// A serial port: the 150 ms floor and the focused-only subscription
    /// exist because of it.
    Serial,
}

impl LinkTransport {
    /// Read the transport off a link's endpoint key. Everything that is not
    /// a `sim:` endpoint is a wire — a serial port today, and anything that
    /// arrives over one tomorrow.
    pub fn from_endpoint(endpoint: &str) -> Self {
        match crate::uid_from_sim_endpoint(endpoint) {
            Some(_) => Self::Sim,
            None => Self::Serial,
        }
    }
}

/// The lens's handle on a roster device (round-2 M5).
///
/// The board is the roster's; this is what the EDITOR needs to know about
/// the one it is looking through. The wire itself is borrowed from the
/// roster's link for the lens's lifetime (the effects layer's
/// exclusive-borrow discipline) and given back at detach — the pool never
/// owns a port.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceLensAttachment {
    /// The roster device the lens is on.
    pub device: lpa_devices::DeviceId,
    /// The roster link the lens borrowed (the port under the session's
    /// wire client).
    pub link: lpa_devices::LinkId,
    /// The device's registered `dev…` uid — the `/device/<uid>` address.
    pub uid: String,
    /// The device's display name at attach (the card's title).
    pub name: String,
    /// The board the device reports (registry `vendor/product`
    /// vocabulary), when known.
    pub board_id: Option<String>,
    /// How the borrowed link is reached — the cadence and probe-policy
    /// fork (PD9).
    pub transport: LinkTransport,
    /// The build features the device's hello reported — the add-node
    /// picker's "Not on this device" gate.
    pub features: Option<Vec<lpc_model::LpFeature>>,
}

/// The runtime a session is attached to.
///
/// One arm, kept as an enum only so the pool's API reads the same as it
/// did: every runtime is a roster device the editor is a lens on.
pub enum RuntimePayload {
    /// A roster device the editor is a lens on.
    Device(DeviceLensAttachment),
}

/// One runtime session in the pool: the attached runtime, its wire client
/// (each session owns its OWN [`StudioServerClient`]), and the server
/// protocol state.
pub struct RuntimeSession {
    id: RuntimeId,
    payload: RuntimePayload,
    client: Option<StudioServerClient>,
    /// This session's server-protocol standing: opening, connected, or
    /// failed. A fact about the LENS's conversation, not about the device —
    /// the device's own standing is the fold's.
    server_state: ServerState,
    /// The last log level Studio asked this session's server to apply,
    /// shown optimistically in the console's device-level selector (there
    /// is no read-back on the wire). Reset to the init default (`Info`)
    /// whenever a connection is (re)established.
    requested_log_level: UiLogLevel,
    /// This session's passive-refresh backoff (runtime-pool P2: the shared
    /// actor singleton became per-session). Only the LENS session's
    /// advances — only the lens runs the fallible project pull.
    backoff: BackoffPolicy,
    /// Passive pulls that failed back to back (reset by the first success).
    /// A device lens reads this as its dead-wire backstop: the browser can
    /// take minutes to notice a USB loss (bench, 2026-09-02: 8.5 min),
    /// and until it does every pull just times out.
    consecutive_refresh_failures: u32,
    /// When the last status heartbeat ran (injected-clock epoch seconds).
    /// `None` = never: the first heartbeat is immediately due.
    last_heartbeat_at: Option<f64>,
    /// When the last passive project pull COMPLETED (injected-clock epoch
    /// seconds). `None` = never: the first pull is immediately due.
    /// Completion-based pacing — the next pull is due one cadence gap after
    /// this stamp, so a pull slower than the gap pushes the next one out
    /// instead of running back-to-back.
    last_refresh_completed_at: Option<f64>,
    /// The per-session console tail (D42): the last [`CONSOLE_TAIL_LEN`]
    /// stamped lines this session's drains produced. The card's console
    /// strip + tab render this; it dies with the session (the console is
    /// the session's, not the app's).
    console_tail: VecDeque<UiLogEntry>,
}

impl RuntimeSession {
    /// A fresh session around an attachment: no wire client yet, server
    /// protocol `Disconnected` until [`Self::attach_server`] runs.
    pub(crate) fn new(id: RuntimeId, payload: RuntimePayload) -> Self {
        Self {
            id,
            payload,
            client: None,
            server_state: ServerState::Disconnected,
            requested_log_level: UiLogLevel::Info,
            backoff: BackoffPolicy::new(PASSIVE_REFRESH_BACKOFF_BASE, PASSIVE_REFRESH_BACKOFF_MAX),
            consecutive_refresh_failures: 0,
            last_heartbeat_at: None,
            last_refresh_completed_at: None,
            console_tail: VecDeque::new(),
        }
    }

    pub fn id(&self) -> RuntimeId {
        self.id
    }

    pub fn payload(&self) -> &RuntimePayload {
        &self.payload
    }

    /// How this session's link is reached (PD9: the one fork left).
    pub fn transport(&self) -> LinkTransport {
        self.attachment().transport
    }

    /// The lens's device handle.
    pub fn attachment(&self) -> &DeviceLensAttachment {
        let RuntimePayload::Device(device) = &self.payload;
        device
    }

    /// The build features the lens device reported at attach (`None` for a
    /// device whose hello carried none, and until `read_device_build` ran).
    pub fn device_features(&self) -> Option<&[lpc_model::LpFeature]> {
        self.attachment().features.as_deref()
    }

    /// Record the build features the lens device's hello reported (read
    /// off the wire at attach).
    pub fn set_device_features(&mut self, features: Vec<lpc_model::LpFeature>) {
        let RuntimePayload::Device(device) = &mut self.payload;
        device.features = Some(features);
    }

    /// Tear the session apart into its attachment (teardown: the wire
    /// client and per-session state drop here).
    pub fn into_payload(self) -> RuntimePayload {
        self.payload
    }

    /// The latest heartbeat-reported per-wire output status, if one has
    /// arrived on this session yet.
    pub fn output_wire_status(&self) -> Option<&[lpc_wire::server::OutputWireStatus]> {
        self.client
            .as_ref()
            .and_then(StudioServerClient::output_wire_status)
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
    /// runtime-level selector disables itself on `None`).
    pub fn requested_log_level(&self) -> Option<UiLogLevel> {
        self.is_connected().then_some(self.requested_log_level)
    }

    /// Record a successfully applied log level for optimistic display.
    pub fn set_requested_log_level(&mut self, level: UiLogLevel) {
        self.requested_log_level = level;
    }

    /// Install a wire client the lens attach flow built over the borrowed
    /// device wire. The pool never opens a port — for a sim either: its
    /// "port" is the worker, and powering it on is the transport's job.
    pub fn attach_device_client(&mut self, client: StudioServerClient) {
        self.install_client(client);
    }

    /// The engine fps the latest heartbeat on this session reported — the
    /// number the card's ▶ meta row shows next to the frame age.
    pub fn engine_fps(&self) -> Option<f32> {
        self.client
            .as_ref()
            .and_then(StudioServerClient::engine_fps)
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

    /// Append stamped lines to this session's console tail (D42), keeping
    /// only the newest [`CONSOLE_TAIL_LEN`].
    ///
    /// The tail carries **Info and up** — the retired global console's
    /// default display floor. Trace/debug diagnostics (the sim worker's
    /// per-tick lines, wire chatter) would drown the 40-line ring in
    /// noise; they still reach the devtools mirror, which fires on the
    /// full drain before this filter. The floor is fixed until a
    /// per-runtime level control lands (flagged at the P2 review).
    pub fn push_console_tail(&mut self, entries: impl IntoIterator<Item = UiLogEntry>) {
        self.console_tail.extend(
            entries
                .into_iter()
                .filter(|entry| !matches!(entry.level, UiLogLevel::Trace | UiLogLevel::Debug)),
        );
        while self.console_tail.len() > CONSOLE_TAIL_LEN {
            self.console_tail.pop_front();
        }
    }

    /// The per-session console tail, oldest first (D42: the card's console
    /// strip shows the last line; the Console tab shows the whole tail).
    pub fn console_tail(&self) -> &VecDeque<UiLogEntry> {
        &self.console_tail
    }

    // -----------------------------------------------------------------
    // Tick policy (runtime-pool P2: per-session cadence/backoff/heartbeat)
    // -----------------------------------------------------------------

    /// The passive project-refresh completion-gap while the lens is on this
    /// session: the sim's tight loop over an in-process channel, or the
    /// serial gap (the wire is the bound, and the 150 ms floor is what
    /// keeps a board answering heartbeats while the editor pulls).
    pub fn cadence_interval(&self) -> Duration {
        match self.transport() {
            LinkTransport::Sim => RefreshCadence::simulator().interval(),
            LinkTransport::Serial => RefreshCadence::device().interval(),
        }
    }

    /// Stamp a passive pull's completion (injected-clock epoch seconds);
    /// the next pull becomes due `gap` after this moment.
    pub(crate) fn mark_refresh_complete(&mut self, now: f64) {
        self.last_refresh_completed_at = Some(now);
    }

    /// Time until the next passive pull is due under `gap`, for the actor's
    /// min-over-sessions delay. A session that never pulled is due at once.
    pub(crate) fn refresh_due_in(&self, now: f64, gap: Duration) -> Duration {
        match self.last_refresh_completed_at {
            None => Duration::ZERO,
            Some(last) => {
                let elapsed = (now - last).max(0.0);
                gap.saturating_sub(Duration::from_secs_f64(elapsed))
            }
        }
    }

    /// Whether a passive pull is due under `gap`. The slack absorbs the UI
    /// timer's millisecond truncation so an on-time tick is not bounced as
    /// early (see [`REFRESH_DUE_SLACK`]).
    pub(crate) fn refresh_due(&self, now: f64, gap: Duration) -> bool {
        self.refresh_due_in(now, gap) <= REFRESH_DUE_SLACK
    }

    /// This session's current passive-refresh backoff delay.
    pub fn backoff_delay(&self) -> Duration {
        self.backoff.current_delay()
    }

    pub(crate) fn record_refresh_success(&mut self) {
        self.backoff.record_success();
        self.consecutive_refresh_failures = 0;
    }

    pub(crate) fn record_refresh_failure(&mut self) {
        self.backoff.record_failure();
        self.consecutive_refresh_failures = self.consecutive_refresh_failures.saturating_add(1);
    }

    /// Passive pulls that failed back to back, for the device lens's
    /// dead-wire backstop.
    pub fn consecutive_refresh_failures(&self) -> u32 {
        self.consecutive_refresh_failures
    }

    /// Whether a status heartbeat is due at `now` (injected-clock epoch
    /// seconds). A session that never heartbeated is due immediately.
    pub(crate) fn heartbeat_due(&self, now: f64) -> bool {
        match self.last_heartbeat_at {
            None => true,
            Some(last) => now - last >= DEVICE_HEARTBEAT_INTERVAL.as_secs_f64(),
        }
    }

    /// Time until this session's next heartbeat is due, for the actor's
    /// min-over-sessions delay.
    pub(crate) fn heartbeat_due_in(&self, now: f64) -> Duration {
        match self.last_heartbeat_at {
            None => Duration::ZERO,
            Some(last) => {
                let elapsed = (now - last).max(0.0);
                DEVICE_HEARTBEAT_INTERVAL.saturating_sub(Duration::from_secs_f64(elapsed))
            }
        }
    }

    pub(crate) fn mark_heartbeat(&mut self, now: f64) {
        self.last_heartbeat_at = Some(now);
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.fail_with_kind(message, ServerFailureKind::Unknown);
    }

    pub fn fail_with_kind(&mut self, message: impl Into<String>, kind: ServerFailureKind) {
        self.client = None;
        self.server_state = ServerState::Failed {
            issue: UiIssue::new(message),
            kind,
        };
    }

    /// Detach the server protocol (drop the wire client) while keeping the
    /// runtime attachment.
    pub fn disconnect_server(&mut self) {
        self.client = None;
        self.server_state = ServerState::Disconnected;
    }
}

/// Test seams: stubbed payloads and direct state injection for
/// view/derivation tests that must not stand up a whole worker.
#[cfg(test)]
impl RuntimeSession {
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
impl DeviceLensAttachment {
    /// A stubbed lens handle for view/derivation tests: a named device on a
    /// fixed roster id/link over a serial wire, no client.
    pub(crate) fn stub_for_test(uid: &str) -> Self {
        Self {
            device: lpa_devices::DeviceId(1),
            link: lpa_devices::LinkId(1),
            uid: uid.to_string(),
            name: "XIAO ESP32-C6 · Sep 1".to_string(),
            board_id: Some("seeed/xiao-esp32-c6".to_string()),
            transport: LinkTransport::Serial,
            features: None,
        }
    }

    /// The same stub over a SIM's worker channel — a Desktop sim, the shape
    /// the open path now produces for a library card.
    pub(crate) fn sim_stub_for_test(uid: &str) -> Self {
        Self {
            device: lpa_devices::DeviceId(1),
            link: lpa_devices::LinkId(1),
            uid: uid.to_string(),
            name: "Desktop sim".to_string(),
            board_id: Some("lightplayer/desktop".to_string()),
            transport: LinkTransport::Sim,
            features: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refresh_cadence_follows_the_link_transport() {
        let sim = RuntimeSession::new(
            RuntimeId::new(1),
            RuntimePayload::Device(DeviceLensAttachment::sim_stub_for_test("devsim")),
        );
        assert_eq!(sim.transport(), LinkTransport::Sim);
        assert_eq!(
            sim.cadence_interval(),
            RefreshCadence::simulator().interval()
        );

        let device = RuntimeSession::new(
            RuntimeId::new(2),
            RuntimePayload::Device(DeviceLensAttachment::stub_for_test("devabc")),
        );
        assert_eq!(device.transport(), LinkTransport::Serial);
        assert_eq!(
            device.cadence_interval(),
            RefreshCadence::device().interval(),
            "a lens on a serial wire pulls at the serial-safe gap"
        );
        assert!(
            device.device_features().is_none(),
            "no hello features recorded on the stub"
        );
    }

    #[test]
    fn the_transport_is_read_off_the_endpoint() {
        assert_eq!(
            LinkTransport::from_endpoint(&crate::sim_endpoint("dev123").0),
            LinkTransport::Sim
        );
        assert_eq!(
            LinkTransport::from_endpoint("usb-serial-0"),
            LinkTransport::Serial
        );
    }
}
