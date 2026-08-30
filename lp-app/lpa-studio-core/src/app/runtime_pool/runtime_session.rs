//! One attached runtime: its payload, wire client, and per-session state.
//!
//! ⚠️ The DEVICE payload arm is gone (M2 of the device-model rebuild):
//! `DeviceHandle`, the per-device reconcile bundle (`device_sync`,
//! `hardware_id`, versions, drift times, storage id), the in-flight
//! `operation` flag, the observed-device-state heartbeat baseline and the
//! device console sink all died with the flows that wrote them. The pool
//! was reduced to the SIM's needs (vision R1: the sim keeps its existing
//! path untouched through round 1); the rebuilt device model owns its own
//! session shape.
//!
//! A [`RuntimeSession`] therefore bundles: the simulator attachment, the
//! wire client it owns, the server protocol state, the per-session console
//! tail, the card frame feed, and this session's refresh/heartbeat pacing.
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

/// One runtime session in the pool: the attached simulator, its wire
/// client (each session owns its OWN [`StudioServerClient`]), and the
/// server protocol state.
pub struct RuntimeSession {
    id: RuntimeId,
    payload: SimAttachment,
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
    pub(crate) fn new(id: RuntimeId, payload: SimAttachment) -> Self {
        Self {
            id,
            payload,
            client: None,
            server_state: ServerState::Disconnected,
            requested_log_level: UiLogLevel::Info,
            backoff: BackoffPolicy::new(PASSIVE_REFRESH_BACKOFF_BASE, PASSIVE_REFRESH_BACKOFF_MAX),
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

    pub fn payload(&self) -> &SimAttachment {
        &self.payload
    }

    /// Tear the session apart into its attachment (teardown: the wire
    /// client and per-session state drop here).
    pub fn into_payload(self) -> SimAttachment {
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
    pub fn attach_server(&mut self, updates: UxUpdateSink) -> Result<(), UiError> {
        // Direct field write (not a &mut self helper) so the state
        // transition can happen while the payload is borrowed.
        self.server_state = connecting_state();
        let client = StudioServerClient::from_sim_connection(
            Rc::clone(&self.payload.connector),
            &self.payload.connection,
            updates,
        )?;
        self.install_client(client);
        Ok(())
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
    /// session.
    pub fn cadence_interval(&self) -> Duration {
        RefreshCadence::simulator().interval()
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
    }

    pub(crate) fn record_refresh_failure(&mut self) {
        self.backoff.record_failure();
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

/// The "Opening server protocol" connecting state every attach passes
/// through (the retired `ServerController::mark_connecting` label).
fn connecting_state() -> ServerState {
    ServerState::Connecting {
        progress: crate::ProgressState::new("Opening server protocol"),
    }
}
