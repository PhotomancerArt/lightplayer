//! Evidence: the fold output. **Only the fold writes this.**
//!
//! Fold discipline (invariant I6) is the anti-fifth-machine mechanism in the
//! small: any new fact enters as an event or it does not enter. No `bool`
//! grown beside the fold, ever. Actions may write [`Intent`](crate::Intent)
//! and may spawn or cancel an activity; they may not touch anything here.
//!
//! Two properties fall out of writing it this way:
//!
//! - **Verdicts are non-sticky.** [`Classification`] is not stored as a
//!   transition target; it is *recomputed* from the current observation
//!   window on every fold. Opening a link, or a successful reset, clears the
//!   window — so the model reacts to reboots and replugs instead of latching
//!   a terminal state the way the shipped `DeviceState` does.
//! - **Freshness carries samples, not booleans.** Heartbeats update
//!   `last_heard`; only the went-quiet / came-back *transitions* are
//!   journaled, with a hysteresis window wide enough that a lossy wire
//!   cannot flap the timeline.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::activity::ActivityOutcome;
use crate::event::{ActivityMarker, Event};
use crate::identity::IdentityChain;
use crate::journal::JournalNote;
use crate::link::{LinkEvent, LinkId};
use crate::roster::RosterConfig;
use crate::time::Millis;
use crate::wire::{HelloFacts, LoadedProjectFacts, ServerFrameBody};

/// Boot signatures, mirroring `lpa-link`'s shipped `BootLineClassifier`.
const BLANK_HEADER_SIGNATURE: &str = "invalid header: 0xffffffff";
const ROM_DOWNLOAD_SIGNATURES: &[&str] = &["waiting for download", "(download("];
const SERVER_STARTED_SIGNATURE: &str = "fw-esp32 initialized, starting server loop";
const KNOWN_FOREIGN_BOOT_STRINGS: &[(&str, &str)] = &[(
    "hello from seeed studio xiao esp32-c6",
    "Seeed XIAO factory firmware",
)];
const RECENT_LINE_LIMIT: usize = 80;

/// How many lines the card's terminal panel keeps.
///
/// Deliberately longer than the classification window's tail: this is a LOG,
/// not an observation. A flash's narration plus the boot output that follows
/// it has to fit, because "what did this board actually say" is the question
/// the panel exists to answer.
const OUTPUT_LINE_LIMIT: usize = 200;

/// Everything the world has told us about one device.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Evidence {
    pub presence: Presence,
    /// Recomputed on every fold — never assigned as a transition.
    pub classification: Classification,
    pub freshness: Freshness,
    /// The last activity outcome, kept until a new activity supersedes it.
    /// Survives disconnect on purpose (invariant I4): "flash failed" must
    /// still be readable after the board drops off the bus.
    pub last_outcome: Option<ActivityOutcome>,
    /// A coarse effect holds this device's wire exclusively.
    ///
    /// Folded from [`Event::LinkBorrow`], and read for exactly one thing:
    /// freshness does not evaluate against a wire nobody is listening to.
    /// The pump is stopped for the length of a borrow, so silence during one
    /// says nothing about the board.
    #[serde(default)]
    pub wire_borrowed: bool,
    /// The card's terminal tail: raw serial lines and activity narration, in
    /// the order they happened.
    ///
    /// Deliberately NOT inside [`Observations`]: a window is what the
    /// classifier reasons over and it restarts on every port open, while
    /// this is a log and must survive the reopen that a flash's reconnect
    /// ladder performs — otherwise the panel wipes itself exactly when the
    /// board comes back. Bounded, fold-written, and read only for display.
    #[serde(default)]
    output: VecDeque<String>,
    observations: Observations,
}

impl Evidence {
    /// Fold one world event. The ONLY mutator of this type.
    ///
    /// `identity` is passed in because learning a binding is a fold output
    /// too: promotions and conflicts come from evidence, never from a user
    /// gesture. Returns the journal notes the transition earned — the caller
    /// writes them, so a fold that stops noticing a transition stops
    /// producing a timeline line, and replay catches it.
    pub(crate) fn fold(
        &mut self,
        now: Millis,
        event: &Event,
        identity: &mut IdentityChain,
        config: &RosterConfig,
    ) -> Vec<JournalNote> {
        let mut notes = Vec::new();
        match event {
            Event::LinkAttached { link, info } => {
                self.presence = Presence::Present {
                    link: *link,
                    since: now,
                };
                self.begin_window(now);
                // A borrow belongs to the link it was taken on. This is a
                // different link, so nothing is holding it.
                self.wire_borrowed = false;
                if let Some(learned) = identity.bind_endpoint(info.endpoint.clone()) {
                    notes.extend(identity_notes(learned));
                }
            }
            Event::LinkDetached { .. } => {
                self.presence = Presence::Detached { since: now };
                self.begin_window(now);
                // Unplugged mid-effect: the release event the effect will
                // eventually raise addresses a link this device no longer
                // has, so it would never arrive. Without this the device
                // would stop evaluating freshness for good.
                self.wire_borrowed = false;
            }
            Event::Link { link, event } => {
                notes.extend(self.fold_link_event(now, *link, event, identity, config));
            }
            Event::LinkBorrow { held, .. } => {
                self.wire_borrowed = *held;
            }
            Event::TimerFired { .. } => {
                // A borrowed wire is a wire with no reader: the pump is
                // stopped for the effect's duration, so silence proves
                // nothing about the board and must not become a verdict.
                if !self.wire_borrowed
                    && let Some(note) = self.freshness.evaluate(now, config.quiet_after_ms)
                {
                    notes.push(note);
                }
            }
            Event::ActivityMarker { marker, .. } => {
                notes.extend(self.fold_marker(marker));
            }
            // Identity learned out-of-band by a coarse effect (the flash
            // preflight's efuse MAC read). Pure identity news: it moves no
            // presence and opens no window.
            Event::IdentityObserved {
                identity: observed, ..
            } => {
                notes.extend(identity_notes(identity.learn(observed)));
            }
        }
        self.reclassify(now, config);
        notes
    }

    /// The verdict this evidence would produce if identification settled
    /// right now. The Identify activity asks this at its deadline; nothing
    /// else should need it.
    pub(crate) fn verdict_if_settled(&self, now: Millis) -> Classification {
        self.observations.classify(true, now)
    }

    /// Whether a hello has been heard in the CURRENT observation window.
    pub fn has_hello(&self) -> bool {
        self.observations.hello.is_some()
    }

    /// A proto-mismatched hello in the current window, if one arrived.
    pub fn mismatched_proto(&self) -> Option<u32> {
        self.observations.wrong_proto
    }

    /// Non-hello frames absorbed in the current window: proof of a live peer,
    /// never a verdict on its own.
    pub fn frames_seen(&self) -> usize {
        self.observations.frames_seen
    }

    /// Chip identity read from a passive boot banner, when one named it.
    pub fn detected_chip(&self) -> Option<&str> {
        self.observations.detected_chip.as_deref()
    }

    /// What the board last reported having loaded.
    ///
    /// `None` means it has not said — which is NOT "nothing loaded". The
    /// empty face turns on the difference, and over-claiming "this board is
    /// empty" would offer to overwrite a project that is right there.
    pub fn loaded_projects(&self) -> Option<&[LoadedProjectFacts]> {
        self.observations.loaded.as_deref()
    }

    /// Bounded tail of recent non-protocol serial lines in the CURRENT
    /// window, for diagnosis copy. Window-scoped on purpose: the detail line
    /// it feeds describes the machine that is on the wire now.
    pub fn recent_lines(&self) -> impl Iterator<Item = &str> {
        self.observations.lines.iter().map(String::as_str)
    }

    /// The card's terminal tail: serial lines and activity narration, oldest
    /// first, across window resets. See [`Self::output`].
    pub fn recent_output(&self) -> impl Iterator<Item = &str> {
        self.output.iter().map(String::as_str)
    }

    /// When the next quiet check is due, if one is.
    ///
    /// `None` while a coarse effect holds the wire: nothing is reading the
    /// port, so there is no silence to time. The release event re-arms the
    /// device's timer, because every fold ends by re-arming.
    pub fn quiet_deadline(&self, quiet_after_ms: u64) -> Option<Millis> {
        if self.wire_borrowed {
            return None;
        }
        self.freshness.quiet_deadline(quiet_after_ms)
    }

    /// Append one line to the terminal tail, collapsing an immediate repeat.
    ///
    /// The collapse is what keeps a percent-ticking effect from filling the
    /// panel with two hundred copies of "Writing firmware": the percent is
    /// the bar's business, and the label is the narration.
    fn push_output(&mut self, line: &str) {
        if self.output.back().is_some_and(|last| last == line) {
            return;
        }
        self.output.push_back(line.to_string());
        while self.output.len() > OUTPUT_LINE_LIMIT {
            self.output.pop_front();
        }
    }

    /// Whether identification has produced a verdict in this window.
    pub fn is_settled(&self) -> bool {
        self.observations.settled
    }

    /// The link this device is currently on, if any.
    pub fn link(&self) -> Option<LinkId> {
        self.presence.link()
    }

    fn fold_link_event(
        &mut self,
        now: Millis,
        link: LinkId,
        event: &LinkEvent,
        identity: &mut IdentityChain,
        config: &RosterConfig,
    ) -> Vec<JournalNote> {
        let mut notes = Vec::new();
        match event {
            LinkEvent::Opened { info } => {
                self.presence = Presence::Open { link, since: now };
                // A fresh port is a fresh window: whatever we concluded
                // about the previous generation is no longer evidence.
                self.begin_window(now);
                if let Some(learned) = identity.bind_endpoint(info.endpoint.clone()) {
                    notes.extend(identity_notes(learned));
                }
            }
            LinkEvent::Closed { .. } => {
                // A close moves presence only. It is OUR action — the board
                // did not change because we stopped listening — so the
                // observation window and its verdict survive (the ADR's
                // ruled list: OPEN, successful RESET and DETACH clear the
                // window; close never did). Clearing here ate every
                // ROM-conclusive verdict the moment identify settled and
                // handed the port back (bench regression, G1 2026-08-31).
                self.presence = Presence::Present { link, since: now };
            }
            LinkEvent::Frame(frame) => {
                self.observations.observe_frame(&frame.body, config);
                if let Some(observed) = frame.identity() {
                    notes.extend(identity_notes(identity.learn(observed)));
                }
                let heartbeat = matches!(frame.body, ServerFrameBody::Heartbeat { .. });
                if let Some(note) = self.freshness.heard(now, heartbeat) {
                    notes.push(note);
                }
            }
            LinkEvent::Line(line) => {
                self.observations.observe_line(line);
                self.push_output(line);
                if let Some(note) = self.freshness.heard(now, false) {
                    notes.push(note);
                }
            }
            LinkEvent::ResetOutcome { ok, .. } => {
                if *ok {
                    // The device is rebooting. Everything observed before
                    // the reset describes a machine that no longer exists.
                    self.begin_window(now);
                }
            }
            LinkEvent::Error(_) => {
                self.observations.errors += 1;
            }
        }
        notes
    }

    fn fold_marker(&mut self, marker: &ActivityMarker) -> Vec<JournalNote> {
        match marker {
            ActivityMarker::Started { kind } => {
                // A new activity supersedes the previous outcome.
                self.last_outcome = None;
                self.push_output(&format!("— {} —", kind.label()));
                Vec::new()
            }
            // The narration a coarse effect streams IS what the terminal
            // panel is for: a flash's progress used to be visible only in
            // the browser console (G1 bench, 2026-08-31). Percent stays the
            // bar's; the label is the line.
            ActivityMarker::Progress { label, .. } => {
                self.push_output(label);
                Vec::new()
            }
            ActivityMarker::Ended { outcome, .. } => {
                self.push_output(&outcome.summary());
                self.last_outcome = Some(outcome.clone());
                // An activity that reached an end has settled the window:
                // "no verdict yet" stops being an honest answer.
                self.observations.settled = true;
                Vec::new()
            }
        }
    }

    fn begin_window(&mut self, now: Millis) {
        self.observations = Observations::started_at(now);
        self.freshness = Freshness::default();
    }

    fn reclassify(&mut self, now: Millis, config: &RosterConfig) {
        self.observations.expected_proto = config.expected_proto;
        if !self.presence.is_attached() {
            // Nothing is on the other end, so there is nothing to classify.
            // Saying "blank flash" about a board that is not plugged in is
            // exactly the stale verdict this model exists to remove.
            self.classification = Classification::Unknown;
            return;
        }
        let mut classification = self.observations.classify(self.observations.settled, now);
        if self.freshness.state == Liveness::Quiet
            && matches!(classification, Classification::Unknown)
        {
            classification = Classification::Quiet {
                since: self.freshness.last_heard.unwrap_or(now),
            };
        }
        self.classification = classification;
    }
}

/// Where the device physically is.
///
/// Three variants, not the sketch's two: "plugged in" and "port open" drive
/// different labels and different escapes, and collapsing them is how the
/// shipped system ended up offering Disconnect on a device it had never
/// opened.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Presence {
    #[default]
    Unknown,
    /// Known, but not on the bus.
    Detached { since: Millis },
    /// On the bus, port not open.
    Present { link: LinkId, since: Millis },
    /// Port open; frames and lines can flow.
    Open { link: LinkId, since: Millis },
}

impl Presence {
    pub fn link(self) -> Option<LinkId> {
        match self {
            Self::Present { link, .. } | Self::Open { link, .. } => Some(link),
            Self::Unknown | Self::Detached { .. } => None,
        }
    }

    pub fn is_open(self) -> bool {
        matches!(self, Self::Open { .. })
    }

    pub fn is_attached(self) -> bool {
        self.link().is_some()
    }
}

/// What the device appears to be, recomputed from the current window.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Classification {
    /// No verdict yet. Honest during identification; never a resting state
    /// once identification has settled.
    #[default]
    Unknown,
    /// A proto-compatible LightPlayer server said hello.
    LightPlayer { hello: HelloFacts },
    /// An `M!`-speaking peer that is not a compatible LightPlayer server.
    Incompatible { reason: IncompatibleReason },
    /// Blank or erased flash (repeating invalid-header boot loop).
    Blank,
    /// Sitting in ROM download mode.
    Bootloader,
    /// Somebody else's firmware. `label` is set when the boot banner is one
    /// we recognize as safe to replace.
    Foreign { label: Option<String> },
    /// Nothing heard at all for the hysteresis window.
    Quiet { since: Millis },
}

impl Classification {
    pub fn is_light_player(&self) -> bool {
        matches!(self, Self::LightPlayer { .. })
    }

    /// Whether this counts as an answer to "what is this thing?".
    pub fn is_verdict(&self) -> bool {
        !matches!(self, Self::Unknown)
    }

    pub fn hello(&self) -> Option<&HelloFacts> {
        match self {
            Self::LightPlayer { hello } => Some(hello),
            _ => None,
        }
    }
}

/// Why a peer that speaks the framing is not usable.
///
/// Mirrors `lpa-link`'s `IncompatibleReason`, minus the sticky state
/// machine: [`Self::NoHello`] is decided at the identify deadline (frames
/// flowed, no hello ever came), never on first sight of a non-hello frame.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum IncompatibleReason {
    ProtoMismatch { proto: u32 },
    NoHello,
}

/// How recently we heard anything, and which side of the hysteresis we are
/// on.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Freshness {
    pub last_heard: Option<Millis>,
    pub last_heartbeat: Option<Millis>,
    pub state: Liveness,
    /// When [`Self::state`] last changed — the timestamp a "quiet for 12 s"
    /// label reads from.
    pub changed_at: Option<Millis>,
}

impl Freshness {
    /// Record a sample. Samples are never journaled; only the came-back
    /// transition is.
    pub(crate) fn heard(&mut self, now: Millis, heartbeat: bool) -> Option<JournalNote> {
        self.last_heard = Some(now);
        if heartbeat {
            self.last_heartbeat = Some(now);
        }
        let previous = self.state;
        self.state = Liveness::Live;
        if previous == Liveness::Quiet {
            let quiet_for_ms = self
                .changed_at
                .map(|changed| now.since(changed))
                .unwrap_or_default();
            self.changed_at = Some(now);
            return Some(JournalNote::CameBack { quiet_for_ms });
        }
        if previous == Liveness::Unknown {
            self.changed_at = Some(now);
        }
        None
    }

    /// Check the hysteresis window. Called from the timer fold, so going
    /// quiet is an event with a timestamp rather than a value that silently
    /// rots.
    pub(crate) fn evaluate(&mut self, now: Millis, quiet_after_ms: u64) -> Option<JournalNote> {
        let last_heard = self.last_heard?;
        if self.state != Liveness::Live || now.since(last_heard) < quiet_after_ms {
            return None;
        }
        self.state = Liveness::Quiet;
        self.changed_at = Some(now);
        Some(JournalNote::WentQuiet { last_heard })
    }

    /// When the next quiet check is due, if one is.
    pub(crate) fn quiet_deadline(&self, quiet_after_ms: u64) -> Option<Millis> {
        if self.state != Liveness::Live {
            return None;
        }
        Some(self.last_heard?.plus_ms(quiet_after_ms))
    }

    /// Age of the newest sample, in milliseconds.
    pub fn age_ms(&self, now: Millis) -> Option<u64> {
        self.last_heard.map(|heard| now.since(heard))
    }
}

/// Which side of the freshness hysteresis a device is on.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Liveness {
    /// Nothing heard yet in this window.
    #[default]
    Unknown,
    Live,
    Quiet,
}

/// The raw accumulator classification is computed from. Private state of the
/// fold: nothing outside this module may write it, and the accessors on
/// [`Evidence`] are the read surface.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct Observations {
    window_start: Option<Millis>,
    lines: VecDeque<String>,
    blank_header: usize,
    rom_download: usize,
    foreign_label: Option<String>,
    server_started: bool,
    detected_chip: Option<String>,
    frames_seen: usize,
    hello: Option<HelloFacts>,
    /// The board's own report of what it is running. Window-scoped like
    /// every other observation: a reopened port has to be told again.
    loaded: Option<Vec<LoadedProjectFacts>>,
    wrong_proto: Option<u32>,
    errors: usize,
    settled: bool,
    expected_proto: u32,
}

impl Observations {
    fn started_at(now: Millis) -> Self {
        Self {
            window_start: Some(now),
            ..Default::default()
        }
    }

    fn observe_frame(&mut self, body: &ServerFrameBody, config: &RosterConfig) {
        self.expected_proto = config.expected_proto;
        match body {
            ServerFrameBody::Hello(hello) if hello.proto == config.expected_proto => {
                self.hello = Some(hello.clone());
            }
            ServerFrameBody::Hello(hello) => {
                self.wrong_proto = Some(hello.proto);
            }
            // Absorbed, never condemned: a running server heartbeats, so a
            // mid-stream attach sees frames before any hello answer.
            ServerFrameBody::Heartbeat { loaded, .. } => {
                self.frames_seen += 1;
                // Only a heartbeat that CARRIES the report replaces it:
                // older firmware sends none, and treating its silence as
                // "nothing loaded" would offer to overwrite a live project.
                if let Some(loaded) = loaded {
                    self.loaded = Some(loaded.clone());
                }
            }
            ServerFrameBody::Loaded { loaded } => {
                self.frames_seen += 1;
                self.loaded = Some(loaded.clone());
            }
            ServerFrameBody::Other { .. } => {
                self.frames_seen += 1;
            }
        }
    }

    fn observe_line(&mut self, line: &str) {
        let normalized = line.to_ascii_lowercase();
        if self.detected_chip.is_none() {
            self.detected_chip = chip_from_boot_line(&normalized);
        }
        if normalized.contains(BLANK_HEADER_SIGNATURE) {
            self.blank_header += 1;
        }
        if ROM_DOWNLOAD_SIGNATURES
            .iter()
            .any(|signature| normalized.contains(signature))
        {
            self.rom_download += 1;
        }
        if self.foreign_label.is_none() {
            self.foreign_label = KNOWN_FOREIGN_BOOT_STRINGS
                .iter()
                .find(|(signature, _)| normalized.contains(signature))
                .map(|(_, label)| (*label).to_string());
        }
        if normalized.contains(SERVER_STARTED_SIGNATURE) {
            self.server_started = true;
        }
        self.lines.push_back(line.to_string());
        while self.lines.len() > RECENT_LINE_LIMIT {
            self.lines.pop_front();
        }
    }

    /// The verdict function. Pure, ordered strongest-evidence-first, and
    /// deliberately unable to remember anything that is not in this window.
    fn classify(&self, settled: bool, now: Millis) -> Classification {
        if let Some(hello) = &self.hello {
            return Classification::LightPlayer {
                hello: hello.clone(),
            };
        }
        if let Some(proto) = self.wrong_proto {
            return Classification::Incompatible {
                reason: IncompatibleReason::ProtoMismatch { proto },
            };
        }
        if self.rom_download > 0 {
            return Classification::Bootloader;
        }
        if self.blank_header > 0 {
            return Classification::Blank;
        }
        if let Some(label) = &self.foreign_label {
            return Classification::Foreign {
                label: Some(label.clone()),
            };
        }
        if !settled {
            // Boot output and heartbeats are not verdicts. Until
            // identification settles, "I don't know yet" is the honest
            // answer — which is exactly what the shipped hello gate got
            // wrong when it condemned the first non-hello frame.
            return Classification::Unknown;
        }
        if self.frames_seen > 0 || self.server_started {
            return Classification::Incompatible {
                reason: IncompatibleReason::NoHello,
            };
        }
        if !self.lines.is_empty() {
            return Classification::Foreign { label: None };
        }
        Classification::Quiet {
            since: self.window_start.unwrap_or(now),
        }
    }
}

fn identity_notes(learned: crate::identity::IdentityLearned) -> Vec<JournalNote> {
    let mut notes = Vec::new();
    for (binding, value) in learned.promotions {
        notes.push(JournalNote::IdentityPromoted { binding, value });
    }
    for (binding, value) in learned.conflicts {
        notes.push(JournalNote::IdentityConflict { binding, value });
    }
    notes
}

/// Chip identity from one normalized boot line. Mirrors `lpa-link`'s
/// `chip_from_boot_line`: modern ESP32 ROMs print `ESP-ROM:esp32c6-…` on
/// every reset (including the blank-flash boot loop, which is exactly the
/// card that needs it); the classic ESP32's ROM prints a fixed build date.
fn chip_from_boot_line(normalized: &str) -> Option<String> {
    if let Some(rest) = normalized.split("esp-rom:").nth(1) {
        let chip: String = rest
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric())
            .collect();
        if chip.starts_with("esp32") {
            return Some(chip);
        }
    }
    if normalized.contains("ets jun  8 2016") {
        return Some("esp32".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PeerIdentity;
    use crate::link::LinkInfo;
    use crate::wire::ServerFrame;

    #[test]
    fn a_heartbeat_before_the_hello_never_condemns_the_device() {
        // The shipped hello-gate defect, as a fold test: a RUNNING server
        // heartbeats, so a mid-stream attach sees frames first.
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();

        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(100),
            frame(ServerFrame::heartbeat(None)),
        );

        assert_eq!(evidence.classification, Classification::Unknown);
        assert_eq!(evidence.frames_seen(), 1);

        fold(
            &mut evidence,
            &mut identity,
            Millis(200),
            frame(ServerFrame::hello(
                4,
                HelloFacts {
                    proto: config.expected_proto,
                    ..Default::default()
                },
            )),
        );

        assert!(evidence.classification.is_light_player());
    }

    #[test]
    fn a_settled_window_with_frames_but_no_hello_is_incompatible() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();

        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(100),
            frame(ServerFrame::other(9, "UnloadProject")),
        );
        assert_eq!(evidence.classification, Classification::Unknown);

        assert_eq!(
            evidence.verdict_if_settled(Millis(5_000)),
            Classification::Incompatible {
                reason: IncompatibleReason::NoHello
            }
        );
    }

    #[test]
    fn boot_signatures_classify_immediately_but_a_later_hello_wins() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();

        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(10),
            line("ESP-ROM:esp32c6-20220919"),
        );
        fold(
            &mut evidence,
            &mut identity,
            Millis(20),
            line("invalid header: 0xffffffff"),
        );

        assert_eq!(evidence.classification, Classification::Blank);
        assert_eq!(evidence.detected_chip(), Some("esp32c6"));

        // Non-sticky: a board that boots noisily and THEN hellos is a
        // LightPlayer, not a permanently blank chip.
        fold(
            &mut evidence,
            &mut identity,
            Millis(900),
            frame(ServerFrame::hello(
                1,
                HelloFacts {
                    proto: config.expected_proto,
                    ..Default::default()
                },
            )),
        );
        assert!(evidence.classification.is_light_player());
    }

    #[test]
    fn a_successful_reset_clears_the_window() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();

        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(10),
            frame(ServerFrame::hello(
                1,
                HelloFacts {
                    proto: config.expected_proto,
                    ..Default::default()
                },
            )),
        );
        assert!(evidence.classification.is_light_player());

        fold(
            &mut evidence,
            &mut identity,
            Millis(20),
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::ResetOutcome {
                    kind: crate::link::ResetKind::Normal,
                    ok: true,
                },
            },
        );

        assert_eq!(
            evidence.classification,
            Classification::Unknown,
            "a reboot invalidates what we knew"
        );
        assert!(!evidence.has_hello());
    }

    #[test]
    fn one_dropped_heartbeat_does_not_flap_the_timeline() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(&mut evidence, &mut identity, Millis(0), opened());

        let mut notes = Vec::new();
        // Heartbeats at 0 and 5 s, one dropped at 10 s, next at 15 s.
        for at in [0_u64, 5_000] {
            notes.extend(fold(
                &mut evidence,
                &mut identity,
                Millis(at),
                frame(ServerFrame::heartbeat(None)),
            ));
        }
        notes.extend(fold(
            &mut evidence,
            &mut identity,
            Millis(config.quiet_after_ms - 1),
            timer(),
        ));
        notes.extend(fold(
            &mut evidence,
            &mut identity,
            Millis(15_000),
            frame(ServerFrame::heartbeat(None)),
        ));

        assert!(
            notes.is_empty(),
            "a single missed heartbeat is not a transition: {notes:?}"
        );
        assert_eq!(evidence.freshness.state, Liveness::Live);
    }

    #[test]
    fn a_real_silence_journals_one_transition_each_way() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(0),
            frame(ServerFrame::heartbeat(None)),
        );

        let quiet = fold(
            &mut evidence,
            &mut identity,
            Millis(config.quiet_after_ms + 1),
            timer(),
        );
        assert!(matches!(quiet.as_slice(), [JournalNote::WentQuiet { .. }]));
        assert_eq!(evidence.freshness.state, Liveness::Quiet);

        // Firing again while still quiet must not re-journal.
        let again = fold(
            &mut evidence,
            &mut identity,
            Millis(config.quiet_after_ms + 5_000),
            timer(),
        );
        assert!(again.is_empty(), "no repeat WentQuiet: {again:?}");

        let back = fold(
            &mut evidence,
            &mut identity,
            Millis(config.quiet_after_ms + 6_000),
            frame(ServerFrame::heartbeat(None)),
        );
        assert!(matches!(back.as_slice(), [JournalNote::CameBack { .. }]));
    }

    #[test]
    fn identity_is_learned_in_the_fold_and_promotions_are_journaled() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(&mut evidence, &mut identity, Millis(0), opened());

        let notes = fold(
            &mut evidence,
            &mut identity,
            Millis(10),
            frame(ServerFrame::heartbeat(Some(PeerIdentity {
                uid: Some(crate::identity::DeviceUid("dev_abc".to_string())),
                ..Default::default()
            }))),
        );

        assert!(notes.iter().any(|note| matches!(
            note,
            JournalNote::IdentityPromoted {
                binding: crate::identity::IdentityBinding::Uid,
                ..
            }
        )));
        assert_eq!(
            identity.uid,
            Some(crate::identity::DeviceUid("dev_abc".to_string()))
        );
    }

    #[test]
    fn detaching_forgets_the_verdict_but_keeps_the_last_outcome() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(10),
            frame(ServerFrame::hello(
                1,
                HelloFacts {
                    proto: config.expected_proto,
                    ..Default::default()
                },
            )),
        );
        evidence.last_outcome = Some(ActivityOutcome::Failed {
            message: "flash failed".to_string(),
        });

        fold(
            &mut evidence,
            &mut identity,
            Millis(20),
            Event::LinkDetached { link: LinkId(1) },
        );

        assert_eq!(evidence.classification, Classification::Unknown);
        assert!(matches!(
            evidence.last_outcome,
            Some(ActivityOutcome::Failed { .. })
        ));
    }

    fn fold(
        evidence: &mut Evidence,
        identity: &mut IdentityChain,
        now: Millis,
        event: Event,
    ) -> Vec<JournalNote> {
        evidence.fold(now, &event, identity, &RosterConfig::default())
    }

    fn opened() -> Event {
        Event::Link {
            link: LinkId(1),
            event: LinkEvent::Opened {
                info: LinkInfo::default(),
            },
        }
    }

    fn frame(frame: ServerFrame) -> Event {
        Event::Link {
            link: LinkId(1),
            event: LinkEvent::Frame(frame),
        }
    }

    fn line(text: &str) -> Event {
        Event::Link {
            link: LinkId(1),
            event: LinkEvent::Line(text.to_string()),
        }
    }

    fn timer() -> Event {
        Event::TimerFired {
            timer: crate::time::TimerId {
                scope: crate::journal::Scope::Device(crate::identity::DeviceId(1)),
                seq: 1,
            },
        }
    }
}
