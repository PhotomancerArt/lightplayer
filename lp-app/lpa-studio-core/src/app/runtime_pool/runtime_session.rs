//! One attached runtime: its payload, wire client, and per-session state.
//!
//! A [`RuntimeSession`] bundles: the runtime attachment (the payload), the
//! wire client it owns, the server protocol state, the per-session console
//! tail, the card frame feed, and this session's refresh/heartbeat pacing.
//! The payload keeps D22's rule in the type system (the sim is not a
//! device):
//!
//! - [`RuntimePayload::Sim`] — the browser-worker simulator. Connect +
//!   worker io, no boot, no readiness states.
//! - [`RuntimePayload::Device`] — a board the editor is a LENS on
//!   (device-model round 2, M5). The board itself lives in the
//!   `lpa-devices` roster: its identity, evidence, activities and link all
//!   stay there. This payload is only the lens's handle on that device —
//!   which roster device, which link the lens borrowed, and the facts the
//!   editor needs at attach (uid, build features). The old per-device
//!   reconcile bundle (`device_sync`, `hardware_id`, drift times, the
//!   in-flight `operation` flag) is NOT back: those were parallel stores
//!   (invariant I8), and the fold owns their facts now.
//!
//! There is no `None` payload: absence of a runtime is absence from the
//! [`RuntimePool`](super::RuntimePool).

use core::time::Duration;
use std::collections::VecDeque;
use std::rc::Rc;

use lpa_client::BackoffPolicy;
use lpa_link::{LinkConnection, LinkConnector, LinkSession};

use crate::app::runtime_pool::card_feed::CardFeedState;
use crate::app::studio::refresh_cadence::{
    DEVICE_CARD_FEED_INTERVAL, DEVICE_HEARTBEAT_INTERVAL, PASSIVE_REFRESH_BACKOFF_BASE,
    PASSIVE_REFRESH_BACKOFF_MAX, REFRESH_DUE_SLACK, RefreshCadence,
};
use crate::{
    RuntimeId, ServerFailureKind, ServerState, StudioServerClient, UiError, UiIssue, UiLogDraft,
    UiLogEntry, UiLogLevel, UxUpdateSink,
};

/// How many stamped lines the per-session console tail retains (D42: the
/// card's console is a bounded ring, not the full history).
pub const CONSOLE_TAIL_LEN: usize = 40;

/// The project a SIM session currently runs — identity for the live sim
/// card's chip (D36) and the project card's "Running in simulator"
/// indication (the D28 grammar's sim arm). Recorded by load-as-push when
/// the studio opens a library project on the sim; it outlives the editor
/// lens (the sim keeps running detached) and dies with the session.
#[derive(Clone, Debug, PartialEq)]
pub struct SimLoadedProject {
    /// `prj…` uid — thumbnail seed and the project-card pairing key.
    pub uid: String,
    /// Display name (the library slug at open time).
    pub name: String,
}

/// The simulator attachment: connector + session + connection handoff.
/// No states — boot-ready IS the session (D22).
pub struct SimAttachment {
    pub connector: Rc<LinkConnector>,
    pub session: LinkSession,
    pub connection: LinkConnection,
}

/// What kind of runtime a session is attached to (D22: the sim is not a
/// device). Derived from the payload; never stored separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeKind {
    /// The browser-worker simulator.
    Sim,
    /// A real board the editor is a lens on.
    Device,
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
    /// The build features the device's hello reported — the add-node
    /// picker's "Not on this device" gate.
    pub features: Option<Vec<lpc_model::LpFeature>>,
}

/// The runtime a session is attached to.
pub enum RuntimePayload {
    /// The browser-worker simulator (BrowserWorker): a live provider
    /// session whose server io is the worker post-message channel.
    Sim(SimAttachment),
    /// A roster device the editor is a lens on.
    Device(DeviceLensAttachment),
}

impl RuntimePayload {
    pub fn kind(&self) -> RuntimeKind {
        match self {
            Self::Sim(_) => RuntimeKind::Sim,
            Self::Device(_) => RuntimeKind::Device,
        }
    }
}

/// One runtime session in the pool: the attached runtime, its wire client
/// (each session owns its OWN [`StudioServerClient`]), and the server
/// protocol state.
pub struct RuntimeSession {
    id: RuntimeId,
    payload: RuntimePayload,
    client: Option<StudioServerClient>,
    /// The sim's server-protocol standing: opening, connected, or failed.
    ///
    /// Kept through the device teardown because it is the SIM's own state,
    /// not a device store: [`ServerFailureKind::SimCrashed`] is how a
    /// poisoned worker instance reaches the user, and the reboot-under-a-
    /// flap-guard recovery reads it.
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
    /// What this SIM session runs (see [`SimLoadedProject`]).
    sim_loaded_project: Option<SimLoadedProject>,
    /// The board the SIM session claims to be (gallery-rework vision D4),
    /// in the registry's `vendor/product` vocabulary — the same strings as
    /// `RegisteredDevice.board_id` and `ProjectManifest.target`.
    ///
    /// Advisory context ONLY: nothing about the worker changes. It feeds
    /// the card's "as \<board\>" line and the output face's pin diagram.
    ///
    /// INHERITED from the project the sim runs: load-as-push sets it from
    /// that project's manifest `target`, so the persisted fact lives in
    /// `project.json` and is re-derived on every load (the sim itself
    /// persists nothing — D22, its card exists only while the session
    /// does). It is also settable directly, for the moment at sim
    /// (re)creation before any project has landed. `None` — no board known
    /// — is the ordinary default.
    sim_board_id: Option<String>,
    /// The per-session console tail (D42): the last [`CONSOLE_TAIL_LEN`]
    /// stamped lines this session's drains produced. The card's console
    /// strip + tab render this; it dies with the session (the console is
    /// the session's, not the app's).
    console_tail: VecDeque<UiLogEntry>,
    /// This session's live frame feed — the ▶ card tab's state
    /// (honest-device preview P2). Deliberately NOT cleared when the server
    /// protocol detaches: the last in-session frame is what a stopped card
    /// shows (Q4). Only its connection-scoped half (project handle,
    /// geometry) is invalidated, in [`Self::disconnect_server`] and at each
    /// fresh attach.
    card_feed: CardFeedState,
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
            sim_loaded_project: None,
            sim_board_id: None,
            console_tail: VecDeque::new(),
            card_feed: CardFeedState::default(),
        }
    }

    pub fn id(&self) -> RuntimeId {
        self.id
    }

    pub fn payload(&self) -> &RuntimePayload {
        &self.payload
    }

    /// What kind of runtime this session is attached to.
    pub fn kind(&self) -> RuntimeKind {
        self.payload.kind()
    }

    /// The simulator attachment, when this is a SIM session.
    pub fn sim_payload(&self) -> Option<&SimAttachment> {
        match &self.payload {
            RuntimePayload::Sim(sim) => Some(sim),
            RuntimePayload::Device(_) => None,
        }
    }

    /// The lens's device handle, when this is a DEVICE session.
    pub fn device_attachment(&self) -> Option<&DeviceLensAttachment> {
        match &self.payload {
            RuntimePayload::Device(device) => Some(device),
            RuntimePayload::Sim(_) => None,
        }
    }

    /// The build features the lens device reported at attach (`None` for
    /// the sim, and for a device whose hello carried none).
    pub fn device_features(&self) -> Option<&[lpc_model::LpFeature]> {
        self.device_attachment()?.features.as_deref()
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

    /// The project this SIM session runs, when one has been pushed onto it.
    pub fn sim_loaded_project(&self) -> Option<&SimLoadedProject> {
        self.sim_loaded_project.as_ref()
    }

    /// Record what load-as-push put on this session (the live sim card's
    /// identity evidence).
    pub fn set_sim_loaded_project(&mut self, project: Option<SimLoadedProject>) {
        self.sim_loaded_project = project;
    }

    /// The board this SIM session claims to be (see [`Self::sim_board_id`]'s
    /// field doc).
    pub fn sim_board_id(&self) -> Option<&str> {
        self.sim_board_id.as_deref()
    }

    /// Give the session a board identity (D4), or clear it. Advisory: no
    /// engine or worker behavior follows.
    pub fn set_sim_board_id(&mut self, board_id: Option<String>) {
        self.sim_board_id = board_id;
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

    /// Attach the server protocol to this session's runtime: the sim's
    /// worker io becomes the wire.
    ///
    /// SIM only. A device session's client is built by the lens attach
    /// flow over the borrowed roster wire and installed with
    /// [`Self::attach_device_client`] — the pool never opens a port.
    pub fn attach_server(&mut self, updates: UxUpdateSink) -> Result<(), UiError> {
        let RuntimePayload::Sim(sim) = &self.payload else {
            return Err(UiError::MissingSession(
                "a device session attaches through its lens wire, not the sim path".to_string(),
            ));
        };
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

    /// Install a wire client the lens attach flow built over a borrowed
    /// device wire (device sessions only; the sim attaches through
    /// [`Self::attach_server`]).
    pub fn attach_device_client(&mut self, client: StudioServerClient) {
        self.install_client(client);
    }

    // -----------------------------------------------------------------
    // Card feed (honest-device preview P2)
    // -----------------------------------------------------------------

    /// This session's live frame feed (the ▶ card tab's state).
    pub fn card_feed(&self) -> &CardFeedState {
        &self.card_feed
    }

    pub(crate) fn card_feed_mut(&mut self) -> &mut CardFeedState {
        &mut self.card_feed
    }

    /// The completion-gap between this session's frame reads.
    pub fn card_feed_interval(&self) -> Duration {
        DEVICE_CARD_FEED_INTERVAL
    }

    /// The engine fps the latest heartbeat on this session reported — the
    /// number the card's ▶ meta row shows next to the frame age.
    pub fn engine_fps(&self) -> Option<f32> {
        self.client
            .as_ref()
            .and_then(StudioServerClient::engine_fps)
    }

    /// The loaded-project handle the latest heartbeat reported, if one has
    /// arrived (the feed's free handle acquisition).
    pub fn heartbeat_project_handle(&self) -> Option<u32> {
        self.client
            .as_ref()
            .and_then(StudioServerClient::loaded_project_handle)
    }

    fn install_client(&mut self, client: StudioServerClient) {
        let protocol = client.protocol().to_string();
        self.client = Some(client);
        self.server_state = ServerState::Connected { protocol };
        // A fresh connection means a fresh server process/boot: its effective
        // log level is back at the init default.
        self.requested_log_level = UiLogLevel::Info;
        // …and a fresh set of project handles. The last frame survives.
        self.card_feed.invalidate_connection();
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
    /// session: the sim's tight loop, or the device gap (the serial wire is
    /// the bound, and the 150 ms floor is what keeps a board answering
    /// heartbeats while the editor pulls).
    pub fn cadence_interval(&self) -> Duration {
        match self.kind() {
            RuntimeKind::Sim => RefreshCadence::simulator().interval(),
            RuntimeKind::Device => RefreshCadence::device().interval(),
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
        // The handle and the geometry were claims about the connection that
        // just ended; the last frame is a fact about the runtime and stays.
        self.card_feed.invalidate_connection();
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
impl SimAttachment {
    /// A stubbed SIMULATOR attachment (record-level fake connector,
    /// synthetic session records) — the "connected but not hardware"
    /// fixture. The connector holds no real session, so flows that close it
    /// will error; fixtures using this only read views and speak through an
    /// injected server client.
    pub(crate) fn stub_for_test() -> Self {
        use lpa_link::providers::fake::FakeProvider;
        use lpa_link::{LinkCapabilities, LinkConnectionKind, LinkProviderKind};
        Self {
            connector: Rc::new(LinkConnector::Fake(FakeProvider::new())),
            session: LinkSession::new(
                "fake-session",
                LinkProviderKind::Fake,
                "fake-runtime",
                LinkConnectionKind::Fake,
                LinkCapabilities::esp32_serial_base(),
            ),
            connection: LinkConnection::fake("fake-runtime", "fake-session"),
        }
    }
}

#[cfg(test)]
impl DeviceLensAttachment {
    /// A stubbed DEVICE lens handle for view/derivation tests: a named
    /// device on a fixed roster id/link, no wire.
    pub(crate) fn stub_for_test(uid: &str) -> Self {
        Self {
            device: lpa_devices::DeviceId(1),
            link: lpa_devices::LinkId(1),
            uid: uid.to_string(),
            name: "XIAO ESP32-C6 · Sep 1".to_string(),
            board_id: Some("seeed/xiao-esp32-c6".to_string()),
            features: None,
        }
    }
}

/// The "Opening server protocol" connecting state every attach passes
/// through (the retired `ServerController::mark_connecting` label).
fn connecting_state() -> ServerState {
    ServerState::Connecting {
        progress: crate::ProgressState::new("Opening server protocol"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refresh_cadence_follows_the_session_kind() {
        let sim = RuntimeSession::new(
            RuntimeId::new(1),
            RuntimePayload::Sim(SimAttachment::stub_for_test()),
        );
        assert_eq!(sim.kind(), RuntimeKind::Sim);
        assert_eq!(
            sim.cadence_interval(),
            RefreshCadence::simulator().interval()
        );

        let device = RuntimeSession::new(
            RuntimeId::new(2),
            RuntimePayload::Device(DeviceLensAttachment::stub_for_test("devabc")),
        );
        assert_eq!(device.kind(), RuntimeKind::Device);
        assert_eq!(
            device.cadence_interval(),
            RefreshCadence::device().interval(),
            "a device lens pulls at the serial-safe gap"
        );
        assert!(
            device.device_features().is_none(),
            "no hello features recorded on the stub"
        );
        assert!(
            device.sim_payload().is_none() && device.device_attachment().is_some(),
            "kind accessors never cross"
        );
    }
}
