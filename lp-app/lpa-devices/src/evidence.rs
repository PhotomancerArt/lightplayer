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
use crate::wire::{
    HelloFacts, LoadedProjectFacts, ProjectFaultFacts, RecoveryFacts, RecoveryLevelFacts,
    ServerFrameBody,
};

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
const TERMINAL_CAP: usize = 200;

/// A boot-line signature ROM output matches, mirroring `lpa-link`'s
/// `chip_from_boot_line` classifier: modern ESP32 ROMs are chatty on every
/// reset, and that chatter is what the terminal panel colours apart from
/// what the running server itself prints.
const ROM_LINE_SIGNATURES: &[&str] = &[
    "esp-rom:",
    "build:",
    "rst:",
    "boot:",
    "invalid header",
    "waiting for download",
    "entry 0x",
    "load:",
];

/// The marker a recovery-ledger line on the wire starts with.
const RECOVERY_LINE_PREFIX: &str = "[RECOVERY]";

/// One line of the card's terminal panel.
///
/// Typed so the renderer can colour ROM banter, board chatter, decoded wire
/// frames, Studio's own narration and outcomes apart — see
/// [`TerminalKind`]. `repeats` collapses a consecutive identical `(kind,
/// text)` pair rather than dropping it silently, so a percent-ticking
/// effect or a heartbeat that says nothing new reads as "×12", not as
/// twelve copies or one copy with the rest thrown away.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalLine {
    pub kind: TerminalKind,
    pub text: String,
    pub repeats: u32,
}

/// What produced one terminal line, for the renderer's colour and grouping.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TerminalKind {
    /// The ROM bootloader's own chatter (`ESP-ROM:`, `rst:`, `boot:`, …).
    Rom,
    /// A line the running board's own server printed.
    Board,
    /// A decoded wire frame (hello, heartbeat, loaded, other) — see
    /// [`wire_summary`]. This is what makes heartbeats visible at all; the
    /// wire never reached the panel before this existed.
    Wire,
    /// Studio's own narration of an activity: started, progress, or a step
    /// label. The kind carries what used to be "— … —" dressing.
    Studio,
    /// An activity ended successfully.
    Outcome,
    /// An activity ended unsuccessfully.
    Failure,
    /// A `[RECOVERY]` line from the device's crash-recovery ledger.
    Recovery,
}

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
    /// The card's terminal tail: raw serial lines, decoded wire frames and
    /// activity narration, in the order they happened.
    ///
    /// Deliberately NOT inside [`Observations`]: a window is what the
    /// classifier reasons over and it restarts on every port open, while
    /// this is a log and must survive the reopen that a flash's reconnect
    /// ladder performs — otherwise the panel wipes itself exactly when the
    /// board comes back. Bounded, fold-written, and read only for display.
    #[serde(default)]
    output: VecDeque<TerminalLine>,
    /// How many lines have fallen off the front of [`Self::output`] since
    /// this evidence began. Deliberately NOT reset on a window boundary —
    /// `output` itself survives a reopen for the same reason (see its doc),
    /// so a counter that reset with the window would undercount right after
    /// the reconnect ladder that made it matter.
    #[serde(default)]
    terminal_dropped: u32,
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

    /// Whether a hello has been heard in the CURRENT observation window —
    /// any hello, whatever wire proto it claims (see [`Self::wire_version`]).
    pub fn has_hello(&self) -> bool {
        self.observations.hello.is_some()
    }

    /// How the board's wire proto compares to this build's, once a hello has
    /// said what it speaks. `None` until one has.
    ///
    /// A FACT, not a verdict (ruled 2026-09-04): a board on another wire
    /// version is still a LightPlayer we talk to — the user at 2am wants the
    /// small change, not a forced flash that might go wrong — and this is
    /// what lets every face and verb stay while the firmware line says
    /// "older than Studio".
    pub fn wire_version(&self) -> Option<WireVersion> {
        self.observations
            .hello
            .as_ref()
            .map(|hello| WireVersion::compare(hello.proto, self.observations.expected_proto))
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

    /// The board's last reported crash-recovery state.
    ///
    /// `None` means it never said — an embedder with no recovery region
    /// (browser sim, host server) or firmware too old to report. It is NOT
    /// "green", and no caller may render it as healthy.
    pub fn recovery(&self) -> Option<&RecoveryFacts> {
        self.observations.recovery.as_ref()
    }

    /// The fault verdict of the project the card speaks for — the first
    /// reported one, matching the running face's own choice (firmware runs
    /// one project; a host server with several has no card to draw).
    pub fn project_fault(&self) -> Option<&ProjectFaultFacts> {
        self.observations
            .loaded
            .as_ref()
            .and_then(|loaded| loaded.first())
            .and_then(|project| project.fault.as_ref())
    }

    /// Whether the board is running but not running WELL: a faulted
    /// project, or a recovery state it reported as anything but green.
    pub fn is_degraded(&self) -> bool {
        self.project_fault().is_some()
            || self
                .recovery()
                .is_some_and(|recovery| recovery.is_degraded())
    }

    /// Bounded tail of recent non-protocol serial lines in the CURRENT
    /// window, for diagnosis copy. Window-scoped on purpose: the detail line
    /// it feeds describes the machine that is on the wire now.
    pub fn recent_lines(&self) -> impl Iterator<Item = &str> {
        self.observations.lines.iter().map(String::as_str)
    }

    /// The card's terminal tail: serial lines, wire frames and activity
    /// narration, oldest first, across window resets. See [`Self::output`].
    pub fn recent_output(&self) -> impl Iterator<Item = &TerminalLine> {
        self.output.iter()
    }

    /// How many terminal lines have been dropped to keep the panel at
    /// [`TERMINAL_CAP`]. Never reset — see [`Self::terminal_dropped`]'s doc.
    pub fn terminal_dropped(&self) -> u32 {
        self.terminal_dropped
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

    /// Append one line to the terminal tail, collapsing an immediate repeat
    /// of the same `(kind, text)` pair into a `repeats` count instead of
    /// dropping it silently.
    ///
    /// The collapse is what keeps a percent-ticking effect from filling the
    /// panel with two hundred copies of "Writing firmware", and what keeps
    /// an unchanging heartbeat from filling it with two hundred copies of
    /// the same wire summary: the percent (or the uptime) is not part of the
    /// text, so identical facts collapse and only a real change starts a new
    /// line.
    fn push_output(&mut self, kind: TerminalKind, text: impl AsRef<str>) {
        let text = text.as_ref();
        if let Some(last) = self.output.back_mut()
            && last.kind == kind
            && last.text == text
        {
            last.repeats += 1;
            return;
        }
        self.output.push_back(TerminalLine {
            kind,
            text: text.to_string(),
            repeats: 1,
        });
        while self.output.len() > TERMINAL_CAP {
            self.output.pop_front();
            self.terminal_dropped += 1;
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
                // Decoded to one readable line: this is what makes
                // heartbeats visible in the panel at all (they never reached
                // it before). The summary carries no uptime and no counter,
                // so an unchanging heartbeat collapses via `push_output`.
                self.push_output(TerminalKind::Wire, wire_summary(&frame.body));
                // A hello on another wire version is news once per window:
                // one journal line and one terminal line saying we noticed
                // and are carrying on. Every hello after it in the same
                // window says the same thing, so it is not repeated.
                if let ServerFrameBody::Hello(hello) = &frame.body
                    && let Some(version) = self.wire_version()
                    && version.is_mismatch()
                    && !self.observations.wire_mismatch_noted
                {
                    self.observations.wire_mismatch_noted = true;
                    self.push_output(TerminalKind::Studio, version.notice());
                    notes.push(JournalNote::WireVersionMismatch {
                        board: hello.proto,
                        studio: config.expected_proto,
                    });
                }
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
                self.push_output(line_kind(line), line);
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
                // No more "— … —" dressing: the Studio kind carries that
                // the line is Studio's own narration.
                self.push_output(TerminalKind::Studio, kind.label());
                Vec::new()
            }
            // The narration a coarse effect streams IS what the terminal
            // panel is for: a flash's progress used to be visible only in
            // the browser console (G1 bench, 2026-08-31). Percent stays the
            // bar's; the label is the line.
            ActivityMarker::Progress { label, .. } => {
                self.push_output(TerminalKind::Studio, label);
                Vec::new()
            }
            ActivityMarker::Ended { outcome, .. } => {
                let kind = if outcome.is_success() {
                    TerminalKind::Outcome
                } else {
                    TerminalKind::Failure
                };
                self.push_output(kind, outcome.summary());
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
///
/// There is deliberately no proto-mismatch variant any more. A hello on
/// another wire version used to land here and the card then described a
/// running board as a blank chip (bench 2026-09-04, proto-19 V3 on a
/// proto-20 Studio). The version difference is a fact on the LightPlayer
/// verdict — [`Evidence::wire_version`] — not a reason to refuse it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum IncompatibleReason {
    NoHello,
}

/// The board's wire proto against this build's.
///
/// Studio has no wire-format versioning yet (ruled 2026-09-04: hope it
/// works, so long as we are aware it is old or new). This is the
/// awareness: it rides the firmware face and the journal, changes no
/// status and withholds no verb.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WireVersion {
    Match,
    BoardOlder { board: u32, studio: u32 },
    BoardNewer { board: u32, studio: u32 },
}

impl WireVersion {
    pub fn compare(board: u32, studio: u32) -> Self {
        match board.cmp(&studio) {
            std::cmp::Ordering::Equal => Self::Match,
            std::cmp::Ordering::Less => Self::BoardOlder { board, studio },
            std::cmp::Ordering::Greater => Self::BoardNewer { board, studio },
        }
    }

    pub fn is_mismatch(self) -> bool {
        !matches!(self, Self::Match)
    }

    /// The one-line notice the terminal and the journal carry. Internal
    /// vocabulary ("wire proto") is fine here — this is the log, and the
    /// numbers are what a bug report needs; the card's firmware line says
    /// it in user words.
    pub fn notice(self) -> String {
        match self {
            Self::Match => "firmware speaks this build's wire proto".to_string(),
            Self::BoardOlder { board, studio } => format!(
                "firmware speaks wire proto {board}, Studio speaks {studio} — older firmware, \
                 proceeding anyway"
            ),
            Self::BoardNewer { board, studio } => format!(
                "firmware speaks wire proto {board}, Studio speaks {studio} — newer firmware, \
                 proceeding anyway"
            ),
        }
    }
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
    /// The board's own report of its crash-recovery state. Window-scoped
    /// for the same reason, and — like `loaded` — only REPLACED by a frame
    /// that carries one.
    recovery: Option<RecoveryFacts>,
    /// The wire-version notice has been journaled for this window.
    #[serde(default)]
    wire_mismatch_noted: bool,
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
            // EVERY hello is kept, whatever proto it claims. Dropping the
            // mismatched ones here is how the card came to say "no firmware"
            // over a board whose hello had just named its firmware (bench
            // 2026-09-04). The proto comparison is a fact read off the
            // stored hello — `Evidence::wire_version` — never a filter.
            ServerFrameBody::Hello(hello) => {
                self.hello = Some(hello.clone());
            }
            // Absorbed, never condemned: a running server heartbeats, so a
            // mid-stream attach sees frames before any hello answer.
            ServerFrameBody::Heartbeat {
                loaded, recovery, ..
            } => {
                self.frames_seen += 1;
                // Only a heartbeat that CARRIES the report replaces it:
                // older firmware sends none, and treating its silence as
                // "nothing loaded" would offer to overwrite a live project.
                if let Some(loaded) = loaded {
                    self.loaded = Some(loaded.clone());
                }
                // Same rule, and it matters MORE here: a frame with no
                // recovery block is a device that did not say, so the last
                // thing it did say stands. Clearing on silence would make
                // an embedder without a recovery region (and old firmware)
                // look green, which is the lie this whole fact exists to
                // stop.
                if let Some(recovery) = recovery {
                    self.recovery = Some(recovery.clone());
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
            // Any hello: a LightPlayer, on whatever wire version it speaks.
            return Classification::LightPlayer {
                hello: hello.clone(),
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

/// The [`TerminalKind`] a raw serial line earns, for the terminal panel.
///
/// ROM chatter is checked first — a boot loop's `invalid header` lines are
/// unmistakably the bootloader, never the recovery ledger — then a
/// `[RECOVERY]`-prefixed line, then the fallback: whatever the running
/// board's own server printed.
fn line_kind(line: &str) -> TerminalKind {
    let normalized = line.to_ascii_lowercase();
    if ROM_LINE_SIGNATURES
        .iter()
        .any(|signature| normalized.contains(signature))
    {
        TerminalKind::Rom
    } else if line.starts_with(RECOVERY_LINE_PREFIX) {
        TerminalKind::Recovery
    } else {
        TerminalKind::Board
    }
}

/// Decode one wire frame to the single readable line the terminal panel
/// shows. Deliberately excludes anything that changes every time a healthy
/// board is asked nothing new (uptime, a frame counter): those would turn
/// every heartbeat into a new line instead of one line collapsing with a
/// repeat count, which is the whole point of putting the wire on the panel.
fn wire_summary(body: &ServerFrameBody) -> String {
    match body {
        ServerFrameBody::Hello(hello) => format!(
            "hello · proto {} · {} · {}",
            hello.proto,
            hello.board_id.as_deref().unwrap_or("?"),
            hello.firmware.as_deref().unwrap_or("?"),
        ),
        ServerFrameBody::Heartbeat {
            loaded, recovery, ..
        } => heartbeat_summary(loaded, recovery),
        ServerFrameBody::Loaded { loaded } => loaded_summary(loaded),
        ServerFrameBody::Other { label } => label.clone(),
    }
}

/// `heartbeat · <project|idle>[ · FAULT <label>]`. No fps or heap: this
/// mirror crate carries no such facts on [`LoadedProjectFacts`] or
/// [`RecoveryFacts`] today (see `wire.rs`'s module doc on why the mirror
/// stays small), so the summary states only what the model actually knows.
fn heartbeat_summary(
    loaded: &Option<Vec<LoadedProjectFacts>>,
    recovery: &Option<RecoveryFacts>,
) -> String {
    let project = loaded
        .as_ref()
        .and_then(|projects| projects.first())
        .map(|project| project.label().to_string())
        .unwrap_or_else(|| "idle".to_string());
    let mut summary = format!("heartbeat · {project}");
    let fault = loaded
        .as_ref()
        .and_then(|projects| projects.first())
        .and_then(|project| project.fault.as_ref())
        .map(|_| "fault".to_string())
        .or_else(|| {
            recovery
                .as_ref()
                .filter(|recovery| recovery.is_degraded())
                .map(|recovery| {
                    if recovery.level == RecoveryLevelFacts::Green {
                        // Green with safe mode set is the one degraded state
                        // the level word itself would misdescribe.
                        "safe-mode".to_string()
                    } else {
                        recovery.level.label().to_string()
                    }
                })
        });
    if let Some(fault) = fault {
        summary.push_str(&format!(" · FAULT {fault}"));
    }
    summary
}

/// `loaded · <n> project(s)`, names joined by `, `.
fn loaded_summary(loaded: &[LoadedProjectFacts]) -> String {
    let count = loaded.len();
    let unit = if count == 1 { "project" } else { "projects" };
    if loaded.is_empty() {
        return format!("loaded · 0 {unit}");
    }
    let names: Vec<&str> = loaded.iter().map(LoadedProjectFacts::label).collect();
    format!("loaded · {count} {unit} ({})", names.join(", "))
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

    #[test]
    fn ten_identical_board_lines_collapse_to_one_line_with_a_repeat_count() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(&mut evidence, &mut identity, Millis(0), opened());

        for at in 0..10 {
            fold(
                &mut evidence,
                &mut identity,
                Millis(at),
                line("project sync: 4 slots bound"),
            );
        }

        let lines: Vec<&TerminalLine> = evidence.recent_output().collect();
        assert_eq!(
            lines.len(),
            1,
            "ten identical lines are one line, not ten: {lines:?}"
        );
        assert_eq!(lines[0].kind, TerminalKind::Board);
        assert_eq!(lines[0].text, "project sync: 4 slots bound");
        assert_eq!(lines[0].repeats, 10);
    }

    #[test]
    fn two_hundred_fifty_distinct_lines_keep_the_newest_two_hundred() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(&mut evidence, &mut identity, Millis(0), opened());

        for at in 0..250 {
            fold(
                &mut evidence,
                &mut identity,
                Millis(at),
                line(&format!("line-{at}")),
            );
        }

        let lines: Vec<&TerminalLine> = evidence.recent_output().collect();
        assert_eq!(lines.len(), 200, "the panel caps at 200: {}", lines.len());
        assert_eq!(evidence.terminal_dropped(), 50);
        assert_eq!(
            lines.first().map(|line| line.text.as_str()),
            Some("line-50"),
            "the oldest fifty are gone"
        );
        assert_eq!(
            lines.last().map(|line| line.text.as_str()),
            Some("line-249")
        );
    }

    #[test]
    fn three_identical_heartbeats_collapse_but_a_different_project_starts_a_new_line() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(&mut evidence, &mut identity, Millis(0), opened());

        let running = vec![LoadedProjectFacts::new("/projects/demo")];
        for at in [0_u64, 5_000, 10_000] {
            fold(
                &mut evidence,
                &mut identity,
                Millis(at),
                frame(ServerFrame::heartbeat_report(
                    None,
                    Some(running.clone()),
                    None,
                )),
            );
        }

        let lines: Vec<&TerminalLine> = evidence.recent_output().collect();
        assert_eq!(
            lines.len(),
            1,
            "three unchanging heartbeats are one Wire line: {lines:?}"
        );
        assert_eq!(lines[0].kind, TerminalKind::Wire);
        assert_eq!(lines[0].text, "heartbeat · demo");
        assert_eq!(lines[0].repeats, 3);

        // A heartbeat that says something DIFFERENT starts a new line rather
        // than collapsing into the last one.
        fold(
            &mut evidence,
            &mut identity,
            Millis(15_000),
            frame(ServerFrame::heartbeat_report(None, Some(Vec::new()), None)),
        );
        let lines: Vec<&TerminalLine> = evidence.recent_output().collect();
        assert_eq!(lines.len(), 2, "a changed heartbeat is a new line");
        assert_eq!(lines[1].text, "heartbeat · idle");
        assert_eq!(lines[1].repeats, 1);
    }

    #[test]
    fn a_hello_frame_produces_a_wire_line_naming_proto_and_board() {
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
                    proto: 9,
                    board_id: Some("dig-uno".to_string()),
                    firmware: Some("fw-esp32c6 abc1234".to_string()),
                    ..Default::default()
                },
            )),
        );

        let lines: Vec<&TerminalLine> = evidence.recent_output().collect();
        let hello_line = lines
            .iter()
            .find(|line| line.kind == TerminalKind::Wire)
            .expect("a hello produces a Wire line");
        assert_eq!(
            hello_line.text,
            "hello · proto 9 · dig-uno · fw-esp32c6 abc1234"
        );
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
