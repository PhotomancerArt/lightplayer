//! The device card's live frame feed: one pull driver per device, in the
//! effects layer, over the shared link.
//!
//! The honest-device-preview ADR (2026-08-06) reads the frame a board has
//! ALREADY published — `ProjectProbeRequest::OutputFrame`, no render — and
//! this is that read for the round-2 device model. The pull runs as an app
//! conversation on the device's shared link (see `shared_link_client_io`),
//! so the model's pump keeps folding heartbeats while the picture streams,
//! and the pure feed state (`frame_feed::CardFeedState`: revision-only
//! change detection, the `Rc` identity contract the lamp renderer repaints
//! on, every output composed into one picture, last-known surviving the
//! link going dark) is the same state the sim card and the editor lens use.
//!
//! # Frames are not evidence
//!
//! Nothing here writes `Evidence`, changes a card's status or freshness, or
//! reaches the journal. The feed keeps ONLY frame state and its own
//! conversation bookkeeping (which link, which handle, how many pulls in a
//! row went unanswered); every device fact it needs — is the port open, has
//! the board said hello, what is it running, is the wire borrowed — is read
//! off the roster's own evidence at each tick and never cached (invariants
//! I6/I8). The view joins the two at the app layer (`DeviceCardFeedView`);
//! the model's projection stays verbatim.
//!
//! # When it pulls
//!
//! A device is fed while ALL of these hold ([`feed_target`]):
//!
//! - the port is open, the board is a LightPlayer that has said hello this
//!   window, and no activity is running on the card;
//! - the board reports a loaded project (the heartbeat carries it);
//! - nobody holds the wire — a coarse effect or the editor lens pauses the
//!   pump, and a pull then could never be answered (design pin: never pull
//!   under a borrow);
//! - the card is WANTED (mounted on the devices page) and the page is
//!   visible — a picture nobody can see is serial time the board would
//!   rather spend on the wire's other traffic;
//! - the feed is not PARKED: three consecutive pulls that timed out or
//!   failed park it, and it re-arms on the next port open, hello or
//!   loaded-project change (the dead-wire backstop, mirroring the lens's
//!   `LENS_DEAD_WIRE_FAILURES`). Card freshness stays the model's job.
//!
//! Cadence is the completion gap the sim feed uses
//! (`DEVICE_CARD_FEED_INTERVAL`, counted from each pull's completion, so a
//! big dome frame self-throttles) under `DEVICE_CARD_FEED_CLASS`: the
//! actor's passive tick runs due pulls beside the sim's and a preempting
//! gesture cancels the in-flight read at its next frame boundary.

use core::future::Future;
use core::time::Duration;
use std::collections::BTreeMap;

use lpa_client::{CancelSignal, LpClient, ProgressDeadline, PullOutcome};
use lpa_devices::identity::DeviceId;
use lpa_devices::link::LinkId;
use lpa_devices::time::Millis;
use lpa_devices::{Device, Roster};
use lpc_wire::{ClientRequest, ServerMsgBody, WireProjectHandle};

use super::device_effects::DeviceEffects;
use super::shared_link_client_io::SharedLinkClientIo;
use crate::UiControlProductPreview;
use crate::app::frame_feed::{CardFeedState, output_frame_entries};

/// Consecutive unanswered pulls before a feed parks itself.
///
/// The same figure the editor lens closes on (`LENS_DEAD_WIRE_FAILURES`):
/// Chromium can take minutes to report a lost USB port, and a feed that
/// kept asking a dead wire every 150 ms would be the loudest thing on the
/// page. Parking is silent and total; the model's own quiet detection says
/// what the card says.
pub const DEVICE_FEED_PARK_AFTER_FAILURES: u8 = 3;

/// What one device needs to be fed: the wire it is on, the dir it is
/// running from, and the identity of this observation window (the hello),
/// which is what a parked feed re-arms on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FeedTarget {
    pub link: LinkId,
    pub loaded_path: String,
    pub hello_at: Option<Millis>,
}

/// The device-side facts that decide whether a device is fed, read off the
/// roster's evidence — never stored.
///
/// `None` is the common case (no board, nothing running, an activity, a
/// borrowed wire); the caller stamps the attempt and moves on.
pub(crate) fn feed_target(device: &Device, effects: &DeviceEffects) -> Option<FeedTarget> {
    let evidence = &device.evidence;
    if !evidence.presence.is_open()
        || !evidence.classification.is_light_player()
        || !evidence.has_hello()
        || device.activity.is_some()
        || evidence.wire_borrowed
    {
        return None;
    }
    let link = evidence.link()?;
    if effects.wire_borrowed(link) {
        return None;
    }
    let loaded_path = evidence.loaded_projects()?.first()?.path.clone();
    Some(FeedTarget {
        link,
        loaded_path,
        hello_at: evidence.hello_heard_at(),
    })
}

/// One open conversation: the client on a link, minting in the app range.
struct Conversation {
    link: LinkId,
    client: LpClient<SharedLinkClientIo>,
}

/// One device's feed.
pub struct DeviceFrameFeed {
    state: CardFeedState,
    conversation: Option<Conversation>,
    /// The handle the board gave for `loaded_path`, acquired by the feed's
    /// own `ListLoadedProjects` (the mirror carries none) and dropped when
    /// the loaded dir changes, the conversation is rebuilt, or a pull fails
    /// — a device-side reload retires handles.
    handle: Option<(String, WireProjectHandle)>,
    /// Consecutive pulls that timed out or failed.
    failures: u8,
    /// The dead-wire backstop: parked against the window it gave up on, so
    /// a new link, a new hello or a new loaded dir re-arms it.
    parked: Option<FeedTarget>,
    /// The card is mounted somewhere a person can see it.
    wanted: bool,
}

impl DeviceFrameFeed {
    fn new() -> Self {
        Self {
            state: CardFeedState::default(),
            conversation: None,
            handle: None,
            failures: 0,
            parked: None,
            wanted: false,
        }
    }

    /// The newest composed picture, if any frame has ever arrived on this
    /// device (last-known survives the link going dark).
    pub fn frame(&self) -> Option<&UiControlProductPreview> {
        self.state.frame()
    }

    /// Seconds since the newest frame's revision moved.
    pub fn frame_age_secs(&self, now: f64) -> Option<f64> {
        self.state.frame_age_secs(now)
    }

    pub fn is_parked(&self) -> bool {
        self.parked.is_some()
    }

    /// When the last pull attempt completed (or was stamped as skipped), in
    /// the controller's seconds. Unchanged while the feed is not pulling —
    /// which is how a test proves a borrowed wire was never asked.
    pub fn last_pull_completed_at(&self) -> Option<f64> {
        self.state.pull_completed_at()
    }

    pub fn is_wanted(&self) -> bool {
        self.wanted
    }

    /// Whether this feed would pull on `target`: not parked, or parked
    /// against a window that has since changed (a new link, hello or
    /// loaded dir re-arms it — see [`Self::rearm_for`]).
    fn armed_for(&self, target: &FeedTarget) -> bool {
        self.parked.as_ref().is_none_or(|parked| parked != target)
    }

    /// Clear a park whose window has changed, before a pull.
    fn rearm_for(&mut self, target: &FeedTarget) {
        if self.parked.as_ref().is_some_and(|parked| parked != target) {
            self.parked = None;
            self.failures = 0;
        }
    }

    /// Whether a pull is due on this feed right now.
    fn due(&self, now: f64, gap: Duration) -> bool {
        self.state.due(now, gap)
    }

    fn due_in(&self, now: f64, gap: Duration) -> Duration {
        self.state.due_in(now, gap)
    }
}

/// Every device's feed, keyed by the model's device id.
pub struct DeviceFrameFeeds {
    by_device: BTreeMap<DeviceId, DeviceFrameFeed>,
    /// The page is visible (`document.visibilityState`); a hidden tab
    /// feeds nothing.
    page_visible: bool,
}

impl Default for DeviceFrameFeeds {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceFrameFeeds {
    pub fn new() -> Self {
        Self {
            by_device: BTreeMap::new(),
            // Honest default for a page that has not said otherwise: the
            // studio's own visibility signal flips it on first paint.
            page_visible: true,
        }
    }

    pub fn get(&self, device: DeviceId) -> Option<&DeviceFrameFeed> {
        self.by_device.get(&device)
    }

    pub fn page_visible(&self) -> bool {
        self.page_visible
    }

    /// The card's mount lease: `true` when a `DeviceRosterCard` for this
    /// device is on screen, `false` when it unmounts.
    pub fn set_wanted(&mut self, device: DeviceId, wanted: bool) {
        match self.by_device.get_mut(&device) {
            Some(feed) => feed.wanted = wanted,
            None if wanted => {
                let mut feed = DeviceFrameFeed::new();
                feed.wanted = true;
                self.by_device.insert(device, feed);
            }
            None => {}
        }
    }

    pub fn set_page_visible(&mut self, visible: bool) {
        self.page_visible = visible;
    }

    /// Drop the feeds of devices the model no longer has (Forget).
    pub fn retain_devices(&mut self, roster: &Roster) {
        self.by_device
            .retain(|device, _| roster.device(*device).is_some());
    }

    /// Devices whose feed would pull right now, with what they need.
    fn active(&self, roster: &Roster, effects: &DeviceEffects) -> Vec<(DeviceId, FeedTarget)> {
        if !self.page_visible {
            return Vec::new();
        }
        let mut active = Vec::new();
        for (id, feed) in &self.by_device {
            if !feed.wanted {
                continue;
            }
            let Some(device) = roster.device(*id) else {
                continue;
            };
            let Some(target) = feed_target(device, effects) else {
                continue;
            };
            if feed.armed_for(&target) {
                active.push((*id, target));
            }
        }
        active
    }

    /// Time until the earliest due pull, for the actor's min-over-lanes
    /// delay. `None` when nothing is feeding — the common case.
    pub fn due_in(
        &self,
        now: f64,
        gap: Duration,
        roster: &Roster,
        effects: &DeviceEffects,
    ) -> Option<Duration> {
        self.active(roster, effects)
            .into_iter()
            .filter_map(|(id, _)| self.by_device.get(&id))
            .map(|feed| feed.due_in(now, gap))
            .min()
    }

    /// Pull one published frame per feeding device whose completion gap
    /// elapsed.
    ///
    /// Returns `(preempted, new_frame)`: whether a due pull was skipped or
    /// cut short by cancellation (the actor's starvation floor), and
    /// whether any card has a new picture to repaint.
    pub async fn run_due<MakeTimer, Timer, Cancel>(
        &mut self,
        now_secs: &dyn Fn() -> f64,
        gap: Duration,
        deadline_budget: Duration,
        roster: &Roster,
        effects: &DeviceEffects,
        make_timer: MakeTimer,
        cancel: &Cancel,
    ) -> (bool, bool)
    where
        MakeTimer: FnMut(Duration) -> Timer + Clone,
        Timer: Future<Output = ()>,
        Cancel: CancelSignal + ?Sized,
    {
        let now = now_secs();
        let due: Vec<(DeviceId, FeedTarget)> = self
            .active(roster, effects)
            .into_iter()
            .filter(|(id, _)| {
                self.by_device
                    .get(id)
                    .is_some_and(|feed| feed.due(now, gap))
            })
            .collect();
        let mut preempted = false;
        let mut new_frame = false;
        for (id, target) in due {
            if cancel.is_cancelled() {
                preempted = true;
                break;
            }
            let Some(feed) = self.by_device.get_mut(&id) else {
                continue;
            };
            feed.rearm_for(&target);
            let pulled = feed
                .pull(
                    now_secs,
                    &target,
                    deadline_budget,
                    effects,
                    make_timer.clone(),
                    cancel,
                )
                .await;
            new_frame |= pulled;
            preempted = cancel.is_cancelled();
        }
        (preempted, new_frame)
    }
}

impl DeviceFrameFeed {
    /// One pull: make sure the conversation is on the right link and the
    /// handle is for the right dir, read the published frame, fold it.
    /// Returns whether a NEW frame arrived.
    async fn pull<MakeTimer, Timer, Cancel>(
        &mut self,
        now_secs: &dyn Fn() -> f64,
        target: &FeedTarget,
        deadline_budget: Duration,
        effects: &DeviceEffects,
        make_timer: MakeTimer,
        cancel: &Cancel,
    ) -> bool
    where
        MakeTimer: FnMut(Duration) -> Timer,
        Timer: Future<Output = ()>,
        Cancel: CancelSignal + ?Sized,
    {
        if !self.ensure_conversation(target, effects) {
            // The link lends no conversation (gone, or the seams are not
            // installed): stamp the attempt so the ask paces itself.
            self.state.mark_pull_complete(now_secs());
            self.conversation = None;
            self.handle = None;
            return false;
        }
        let handle = match self.ensure_handle(target).await {
            Ok(handle) => handle,
            Err(()) => {
                // Unanswered or refused: counts as a failed pull — a board
                // that will not even list its projects is not one to keep
                // asking for pictures.
                self.note_failure(now_secs(), target);
                return false;
            }
        };
        let request = lpc_wire::ProjectReadRequest {
            since: None,
            queries: Vec::new(),
            // One probe, no mirror queries: a picture, not a ProjectSync.
            probes: vec![lpc_wire::ProjectProbeRequest::OutputFrame(
                lpc_wire::OutputFrameProbeRequest {
                    display_layout: self.state.display_layout_read(),
                },
            )],
        };
        let deadline = ProgressDeadline::new(deadline_budget, make_timer);
        let Some(conversation) = self.conversation.as_mut() else {
            return false;
        };
        let outcome = conversation
            .client
            .project_read_gated(handle, request, deadline, cancel)
            .await;
        let now = now_secs();
        match outcome {
            PullOutcome::Completed { events, .. } => {
                self.state.mark_pull_complete(now);
                self.failures = 0;
                let outputs = output_frame_entries(&events);
                // Every entry folds in: the card composes ALL published
                // outputs into one picture.
                self.state.apply(&outputs, now).new_frame
            }
            // Preempted: keep the old completion stamp so the redo is
            // prompt, exactly like the sim feed.
            PullOutcome::Cancelled => false,
            PullOutcome::TimedOut => {
                self.note_failure(now, target);
                false
            }
            PullOutcome::Failed(error) => {
                log::debug!("device card frame read failed: {error}");
                self.note_failure(now, target);
                false
            }
        }
    }

    /// The conversation on `target.link`, rebuilt when the device moved to
    /// another link (a replug is a new port).
    fn ensure_conversation(&mut self, target: &FeedTarget, effects: &DeviceEffects) -> bool {
        let stale = self
            .conversation
            .as_ref()
            .is_none_or(|conversation| conversation.link != target.link);
        if !stale {
            return true;
        }
        let Some(io) = effects.conversation_io(target.link) else {
            return false;
        };
        self.conversation = Some(Conversation {
            link: target.link,
            client: io.into_client(),
        });
        // A new wire is a new connection: the handle and the per-output
        // geometry claims were about the old one. The last frame stays.
        self.handle = None;
        self.state.invalidate_connection();
        true
    }

    /// The board's handle for `target.loaded_path`, asked for once per
    /// (connection, dir).
    async fn ensure_handle(&mut self, target: &FeedTarget) -> Result<WireProjectHandle, ()> {
        if let Some((path, handle)) = &self.handle
            && *path == target.loaded_path
        {
            return Ok(*handle);
        }
        let Some(conversation) = self.conversation.as_mut() else {
            return Err(());
        };
        let outcome = conversation
            .client
            .send_request(ClientRequest::ListLoadedProjects)
            .await
            .map_err(|_| ())?;
        let ServerMsgBody::ListLoadedProjects { projects } = outcome.value.msg else {
            return Err(());
        };
        let handle = projects
            .iter()
            .find(|project| project.path.as_str() == target.loaded_path)
            .or_else(|| projects.first())
            .map(|project| project.handle)
            .ok_or(())?;
        self.handle = Some((target.loaded_path.clone(), handle));
        Ok(handle)
    }

    fn note_failure(&mut self, now: f64, target: &FeedTarget) {
        self.state.mark_pull_complete(now);
        // A read that timed out or errored says nothing about the handle
        // staying valid — a device-side reload retires it — so drop the
        // connection-scoped half and re-acquire next tick. The last frame
        // stays on screen, aging honestly.
        self.handle = None;
        self.state.invalidate_connection();
        self.failures = self.failures.saturating_add(1);
        if self.failures >= DEVICE_FEED_PARK_AFTER_FAILURES {
            log::debug!(
                "device card feed parked after {} unanswered pulls on {:?}",
                self.failures,
                target.link
            );
            self.parked = Some(target.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parked_feed_re_arms_when_its_window_changes() {
        let mut feed = DeviceFrameFeed::new();
        let target = FeedTarget {
            link: LinkId(1),
            loaded_path: "/projects/studio".to_string(),
            hello_at: Some(Millis(100)),
        };
        feed.parked = Some(target.clone());
        feed.failures = DEVICE_FEED_PARK_AFTER_FAILURES;

        assert!(!feed.armed_for(&target), "the same window stays parked");
        feed.rearm_for(&target);
        assert!(feed.is_parked());

        let rehello = FeedTarget {
            hello_at: Some(Millis(900)),
            ..target.clone()
        };
        assert!(feed.armed_for(&rehello), "a new hello re-arms");
        feed.rearm_for(&rehello);
        assert_eq!(feed.failures, 0);
        assert!(feed.parked.is_none());
    }

    #[test]
    fn three_failures_park_the_feed_and_drop_the_handle() {
        let mut feed = DeviceFrameFeed::new();
        let target = FeedTarget {
            link: LinkId(1),
            loaded_path: "/projects/studio".to_string(),
            hello_at: None,
        };
        feed.handle = Some((target.loaded_path.clone(), WireProjectHandle(7)));

        for _ in 0..(DEVICE_FEED_PARK_AFTER_FAILURES - 1) {
            feed.note_failure(1.0, &target);
            assert!(!feed.is_parked());
        }
        assert!(feed.handle.is_none(), "a failed pull retires the handle");
        feed.note_failure(2.0, &target);

        assert!(feed.is_parked());
        assert_eq!(feed.state.pull_completed_at(), Some(2.0));
    }

    #[test]
    fn wanting_a_card_creates_its_feed_and_unwanting_keeps_the_last_frame() {
        let mut feeds = DeviceFrameFeeds::new();
        let device = DeviceId(3);
        assert!(feeds.get(device).is_none());

        feeds.set_wanted(device, true);
        assert!(feeds.get(device).is_some_and(DeviceFrameFeed::is_wanted));

        feeds.set_wanted(device, false);
        assert!(
            feeds.get(device).is_some_and(|feed| !feed.is_wanted()),
            "the feed (and its last frame) outlives the card's mount"
        );
        // A device never wanted never gets a feed.
        feeds.set_wanted(DeviceId(4), false);
        assert!(feeds.get(DeviceId(4)).is_none());
    }
}
