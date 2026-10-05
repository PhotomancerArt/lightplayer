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

use crate::activity::{ActivityKind, ActivityOutcome};
use crate::app_version::AppVersion;
use crate::bootloader::bootloader_code_ranges;
use crate::event::{ActivityMarker, Event};
use crate::firmware_age::FirmwareAge;
use crate::identity::IdentityChain;
use crate::journal::JournalNote;
use crate::link::{LinkEvent, LinkId};
use crate::roster::RosterConfig;
use crate::time::Millis;
use crate::update_facts::{UpdateBoardState, UpdateFacts};
use crate::wire::{
    HelloFacts, LoadedProjectFacts, ProjectFaultFacts, RecoveryFacts, RecoveryLevelFacts,
    ServerFrameBody,
};

/// Boot signatures, mirroring `lpa-link`'s shipped `BootLineClassifier`.
const BLANK_HEADER_SIGNATURE: &str = "invalid header: 0xffffffff";
const ROM_DOWNLOAD_SIGNATURES: &[&str] = &["waiting for download", "(download("];
const SERVER_STARTED_SIGNATURE: &str = "fw-esp32 initialized, starting server loop";
/// How firmware from before the USB link moved onto lp-link (wire proto 30)
/// spoke on USB: every protocol message as one `M!{json}` text line. Since
/// then a message is a binary link frame, so such a line can only come from
/// older LightPlayer firmware — which the new link reads as console text,
/// never as a frame, so it never says a hello Studio can hear.
const LEGACY_FRAME_LINE_PREFIX: &str = "M!{";
/// The field of the boot marker (`[INIT] fw-esp32 initialized, starting
/// server loop... proto=29 commit=… dirty=…`) that names its wire proto.
const BOOT_MARKER_PROTO_FIELD: &str = "proto=";
/// Foreign firmware recognized by a substring of a lowercased line.
const KNOWN_FOREIGN_BOOT_STRINGS: &[(&str, &str)] = &[
    (
        "hello from seeed studio xiao esp32-c6",
        "Seeed XIAO factory firmware",
    ),
    // WLED built with `WLED_DEBUG`: `setup()` prints
    // `---WLED <version> <build> INIT---` (wled00/wled.cpp, WLED 35948831c).
    ("---wled ", "WLED"),
];
/// Foreign firmware recognized by a whole line, exactly (after trimming).
///
/// A release WLED build prints nothing of its own at boot except the
/// Adalight handshake `Ada` (wled00/wled.cpp `setup()`, WLED 35948831c),
/// once, when the serial pins are free. An Adalight sketch says the same
/// word, and is just as much LED firmware a flash replaces.
const KNOWN_FOREIGN_BOOT_LINES: &[(&str, &str)] = &[("Ada", "WLED")];
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
    /// How the last update ended, typed — the driver's word, or the model's
    /// own when the board never came back. Kept, like [`Self::last_outcome`],
    /// until a new activity supersedes it: the card's words for "needs USB
    /// once" or "the board did not come back" are made of it after the
    /// activity is gone.
    #[serde(default)]
    pub last_update_outcome: Option<crate::activity::UpdateOutcomeFacts>,
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
    /// The current link carries lp-link's update channel (channel 3), as its
    /// [`LinkInfo`](crate::LinkInfo) said at attach and open. A fact about
    /// the LINK, not the board, so it lives outside [`Observations`]: a
    /// board's reset restarts the window but not the transport. Cleared on
    /// detach.
    #[serde(default)]
    link_carries_update_channel: bool,
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
                self.link_carries_update_channel = info.carries_update_channel;
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
                self.link_carries_update_channel = false;
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
            Event::ActivityMarker { marker, effect, .. } => {
                notes.extend(self.fold_marker(marker, effect.is_some()));
            }
            // Identity learned out-of-band by a coarse effect (the flash
            // preflight's efuse MAC read). Pure identity news: it moves no
            // presence and opens no window.
            Event::IdentityObserved {
                identity: observed, ..
            } => {
                notes.extend(identity_notes(identity.learn(observed)));
            }
            // Roster news; a device never hears it.
            Event::GrantAnswered { .. } => {}
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
    /// any hello, whatever wire proto it claims (see [`Self::wire_version`]),
    /// including one from another wire that only told us its version.
    pub fn has_hello(&self) -> bool {
        self.observations.hello.is_some() || self.observations.other_wire_hello.is_some()
    }

    /// The board this window's hello named: a hello this build reads, or —
    /// the one other fact read off it — a hello from another wire. The card
    /// reads it before the record's memory, so an older LightPlayer that
    /// said which board it is gets Update firmware for that board, not a
    /// pick of every board (G1 walk, 2026-10-03).
    pub fn hello_board_id(&self) -> Option<&str> {
        self.classification
            .hello()
            .and_then(|hello| hello.board_id.as_deref())
            .or(self.observations.other_wire_board.as_deref())
    }

    /// When the current window's hello was heard, if one has been. This is
    /// how an activity tells a hello that answered ITS reopen from one the
    /// board sent before the activity began: the window survives a close,
    /// so [`Self::has_hello`] alone cannot.
    pub fn hello_heard_at(&self) -> Option<Millis> {
        self.observations.hello_at
    }

    /// When the current window's channel-3 board manifest (`M`) was heard,
    /// if one has been — the hello's own copy is [`Self::hello_heard_at`]'s.
    /// How an update tells a core-only board that came back (it sends no
    /// hello, only `M`) from the manifest it sent before its reset.
    pub fn update_facts_heard_at(&self) -> Option<Millis> {
        self.observations.update_facts_at
    }

    /// When the current observation window began (an attach, an open, a
    /// successful reset or a detach). A window newer than an instant is a
    /// link session newer than it.
    pub fn window_started_at(&self) -> Option<Millis> {
        self.observations.window_start
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
            .map(|hello| hello.proto)
            .or(self.observations.other_wire_hello)
            .map(|proto| WireVersion::compare(proto, self.observations.expected_proto))
    }

    /// How the board's app version compares to this build's, once a hello
    /// has said it. `None` until one has; [`FirmwareAge::Unknown`] when
    /// either side's version is not one this build can read.
    ///
    /// This is what "out of date" means (plan
    /// `lp2025/2026-10-03-1330-ota-firmware-updates`, M1): an older VERSION,
    /// not an older wire proto. Like [`Self::wire_version`] it is a fact,
    /// never a verdict.
    pub fn firmware_age(&self) -> Option<FirmwareAge> {
        self.observations.hello.as_ref().map(|hello| {
            let board = hello
                .version
                .as_deref()
                .map_or(AppVersion::Unknown, AppVersion::parse);
            FirmwareAge::compare(board, self.observations.expected_version)
        })
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

    /// How many `Saved PC:` boot lines in the CURRENT window landed inside
    /// the detected chip's bootloader code ranges (D4,
    /// [`crate::bootloader::bootloader_code_ranges`]) — the hung-bootloader
    /// signature: the second-stage bootloader was running, not the app, when
    /// the reset hit. Window-scoped like every other observation — a reopen
    /// or a successful reset restarts the count, which is what the Flash
    /// ladder wants (a fresh rung gets a fresh chance).
    pub fn bootloader_hung_resets(&self) -> usize {
        self.observations.bootloader_hung_resets
    }

    /// The last `Saved PC` that landed inside the bootloader's ranges, if
    /// any this window. Kept for the failure copy — a bug report needs the
    /// address, not just "it hung".
    pub fn last_bootloader_hung_pc(&self) -> Option<u32> {
        self.observations.last_bootloader_hung_pc
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
    /// The engine's reported frame rate, off the latest heartbeat this
    /// window that carried one.
    pub fn engine_fps(&self) -> Option<u16> {
        self.observations.engine_fps
    }

    /// The board's link counters, off the latest heartbeat this window that
    /// carried them.
    pub fn link_counters(&self) -> Option<crate::LinkCounterFacts> {
        self.observations.link_counters
    }

    /// The board's latest update facts this window: channel 3's own `M`
    /// when one was heard (authoritative), else the hello's manifest.
    /// `None` = the board has said nothing about its firmware's update
    /// state in this window. What the effects layer reads to decide an
    /// update, and what the core-only face is made of.
    pub fn update_facts(&self) -> Option<&UpdateFacts> {
        self.observations.update_facts.as_ref().or_else(|| {
            self.observations
                .hello
                .as_ref()
                .and_then(|hello| hello.update.as_ref())
        })
    }

    /// Whether the board announced channel 3 in this window: a hello that
    /// carried its manifest, or an `M` it sent. The effects layer asks this
    /// before it sends anything on channel 3 — a board without the channel
    /// would never acknowledge a reliable frame there and the link would
    /// stall.
    pub fn announced_update_channel(&self) -> bool {
        self.update_facts().is_some()
    }

    /// Whether the current link carries lp-link's update channel at all —
    /// the transport's fact, from its [`LinkInfo`](crate::LinkInfo). An
    /// over-the-air update needs both this and
    /// [`Self::announced_update_channel`].
    pub fn carries_update_channel(&self) -> bool {
        self.link_carries_update_channel && self.presence.is_attached()
    }

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
                self.link_carries_update_channel = info.carries_update_channel;
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
                if matches!(
                    frame.body,
                    ServerFrameBody::Hello(_) | ServerFrameBody::HelloOnOtherWire { .. }
                ) {
                    self.observations.hello_at = Some(now);
                }
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
            // Frames are not evidence. An app conversation's reply (the
            // card's frame feed) is routed to its asker by the effects
            // layer and never reaches here by design; one that strays in
            // proves nothing the heartbeat does not, so it moves nothing —
            // not `frames_seen`, not freshness, not the terminal.
            LinkEvent::Passthrough { .. } => {}
            // The transport narrating the link's encoding: journaled (that
            // is the point of it), and not evidence of anything.
            LinkEvent::WireNote(_) => {}
            // Update traffic is routed, not folded: the effects layer hands
            // it to the device's update driver. What the model needs of it
            // arrives decoded, as `UpdateFacts` below.
            LinkEvent::Update(_) => {}
            // The board's own word about its firmware (channel 3's `M`):
            // the core-only verdict is made of it, and it is the board
            // speaking, so it counts as heard.
            LinkEvent::UpdateFacts(facts) => {
                self.observations.update_facts = Some(facts.clone());
                self.observations.update_facts_at = Some(now);
                self.push_output(TerminalKind::Wire, update_summary(facts));
                if let Some(note) = self.freshness.heard(now, false) {
                    notes.push(note);
                }
            }
        }
        notes
    }

    /// `stamped`: the marker came from a coarse effect (it carries an
    /// effect stamp), not from the device's own brackets.
    fn fold_marker(&mut self, marker: &ActivityMarker, stamped: bool) -> Vec<JournalNote> {
        match marker {
            ActivityMarker::Started { kind } => {
                // A new activity supersedes the previous outcome.
                self.last_outcome = None;
                self.last_update_outcome = None;
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
            // The layout inspection's answer is narration too: the card's
            // terminal says what was found before anything is written.
            ActivityMarker::LayoutVerdict { verdict } => {
                let line = match verdict {
                    crate::activity::LayoutVerdict::Plain => {
                        "layout: the board's files stay where they are".to_string()
                    }
                    crate::activity::LayoutVerdict::Migrate { files, .. } => {
                        format!("layout: {files} files to move to the new layout")
                    }
                    crate::activity::LayoutVerdict::Restore { files, .. } => {
                        format!("layout: {files} files to restore from the stored backup")
                    }
                    crate::activity::LayoutVerdict::Refused { files, .. } => {
                        format!("layout: {files} files do not fit the new layout")
                    }
                };
                self.push_output(TerminalKind::Studio, &line);
                Vec::new()
            }
            // An update LEG's end is not the activity's: the activity's own
            // (unstamped) bracket carries the outcome. A leg that ended with
            // its link is the board resetting — the card keeps "Updating…",
            // and the terminal says it is reconnecting, never a failure.
            ActivityMarker::Ended {
                kind: ActivityKind::Update,
                outcome,
            } if stamped => {
                if let ActivityOutcome::Interrupted { reason } = outcome {
                    self.push_output(TerminalKind::Studio, format!("reconnecting — {reason}"));
                }
                Vec::new()
            }
            // Display only: the stage and percent live on the running cell,
            // and the terminal lines are the effects layer's own narration.
            ActivityMarker::UpdateStage { .. } => Vec::new(),
            ActivityMarker::UpdateOutcome(outcome) => {
                self.last_update_outcome = Some(*outcome);
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
        self.observations.expected_version = config.expected_version;
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
    /// A LightPlayer running only its core: it spoke channel 3 (its board
    /// manifest) and no hello — waiting for its engine, engine crashing, a
    /// new core on trial. A LightPlayer, not a blank chip: its way forward
    /// is its engine back over the air, never a Flash. `version` is the
    /// manifest's, the one name such a board gives.
    CoreOnly {
        version: Option<String>,
        state: UpdateBoardState,
    },
    /// LightPlayer firmware too old to speak this Studio's link: it prints
    /// its messages as `M!{json}` text lines, or its boot marker names a wire
    /// proto older than Studio's, and no hello ever arrives. `proto` is the
    /// boot marker's, when one was heard. A flash is the way forward, and the
    /// project on the board survives it (the image stops short of `lpfs`).
    OlderLightPlayer { proto: Option<u32> },
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
    /// Lines in the pre-lp-link `M!{json}` form this window. See
    /// [`LEGACY_FRAME_LINE_PREFIX`].
    #[serde(default)]
    legacy_frame_lines: usize,
    /// The wire proto the boot marker named, when one was heard.
    #[serde(default)]
    boot_marker_proto: Option<u32>,
    detected_chip: Option<String>,
    frames_seen: usize,
    /// Count of `Saved PC:` boot lines this window that landed inside the
    /// detected chip's bootloader code ranges — D4's hung-bootloader
    /// signature. See [`Evidence::bootloader_hung_resets`].
    #[serde(default)]
    bootloader_hung_resets: usize,
    /// The last such PC, kept for the failure copy.
    #[serde(default)]
    last_bootloader_hung_pc: Option<u32>,
    hello: Option<HelloFacts>,
    /// The wire version of a hello this build could not decode, when one
    /// was heard this window ([`ServerFrameBody::HelloOnOtherWire`]).
    #[serde(default)]
    other_wire_hello: Option<u32>,
    /// The board that hello named, when it named one.
    #[serde(default)]
    other_wire_board: Option<String>,
    /// When this window's hello was heard. The Flash ladder asks whether a
    /// hello is NEWER than its write effect's end: a close does not clear
    /// the window, so a board that ran LightPlayer before a flash still
    /// carries its pre-flash hello here while the flasher's closed port is
    /// being reopened (bench, 2026-09-04: that hello started the manifest
    /// stamp over the closed port on the ladder's first poke).
    #[serde(default)]
    hello_at: Option<Millis>,
    /// When this window's channel-3 manifest was heard — the core-only
    /// board's only word, so an update reads its return off this.
    #[serde(default)]
    update_facts_at: Option<Millis>,
    /// The board's own report of what it is running. Window-scoped like
    /// every other observation: a reopened port has to be told again.
    loaded: Option<Vec<LoadedProjectFacts>>,
    /// The board's own report of its crash-recovery state. Window-scoped
    /// for the same reason, and — like `loaded` — only REPLACED by a frame
    /// that carries one.
    recovery: Option<RecoveryFacts>,
    /// The engine's reported frame rate off the latest heartbeat that
    /// carried one. Window-scoped; read by the card's live-feed pill.
    #[serde(default)]
    engine_fps: Option<u16>,
    /// The board's link counters off the latest heartbeat that carried
    /// them. Window-scoped; read by the card's link section.
    #[serde(default)]
    link_counters: Option<crate::LinkCounterFacts>,
    /// The board's update facts off the latest channel-3 `M` this window —
    /// authoritative over the hello's copy (see [`Evidence::update_facts`]).
    #[serde(default)]
    update_facts: Option<UpdateFacts>,
    /// The wire-version notice has been journaled for this window.
    #[serde(default)]
    wire_mismatch_noted: bool,
    errors: usize,
    settled: bool,
    expected_proto: u32,
    #[serde(default)]
    expected_version: AppVersion,
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
        self.expected_version = config.expected_version;
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
                loaded,
                recovery,
                engine_fps,
                link,
                ..
            } => {
                self.frames_seen += 1;
                if engine_fps.is_some() {
                    self.engine_fps = *engine_fps;
                }
                if link.is_some() {
                    self.link_counters = *link;
                }
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
            // A hello all the same, on a wire this build cannot read: kept
            // as its version, the one fact read off it. See `classify`.
            ServerFrameBody::HelloOnOtherWire { proto, board_id } => {
                self.frames_seen += 1;
                self.other_wire_hello = Some(*proto);
                self.other_wire_board = board_id.clone();
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
                .or_else(|| {
                    KNOWN_FOREIGN_BOOT_LINES
                        .iter()
                        .find(|(whole_line, _)| line.trim() == *whole_line)
                })
                .map(|(_, label)| (*label).to_string());
        }
        if normalized.contains(SERVER_STARTED_SIGNATURE) {
            self.server_started = true;
            if let Some(proto) = boot_marker_proto(&normalized) {
                self.boot_marker_proto = Some(proto);
            }
        }
        if line.trim_start().starts_with(LEGACY_FRAME_LINE_PREFIX) {
            self.legacy_frame_lines += 1;
        }
        if let Some(pc) = saved_pc(&normalized) {
            let in_bootloader = self
                .detected_chip
                .as_deref()
                .map(bootloader_code_ranges)
                .unwrap_or(&[])
                .iter()
                .any(|range| range.contains(&pc));
            if in_bootloader {
                self.bootloader_hung_resets += 1;
                self.last_bootloader_hung_pc = Some(pc);
            }
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
        // A hello this build could not read is still a hello (G1-F1). From
        // an OLDER wire it is the older-LightPlayer verdict — an update is
        // the way forward and the project survives it; from a newer one it
        // is a LightPlayer we warn about (this Studio is the old side), on
        // the facts the version alone gives.
        if let Some(proto) = self.other_wire_hello {
            return if proto < self.expected_proto {
                Classification::OlderLightPlayer { proto: Some(proto) }
            } else {
                Classification::LightPlayer {
                    hello: HelloFacts::version_only(proto, self.other_wire_board.clone()),
                }
            };
        }
        // A board that spoke channel 3 and no hello is a LightPlayer core
        // (waiting for its engine, on trial, engine crashing) — never a
        // blank chip, never "no hello". Its own word outranks boot banter,
        // as a hello does. A RUNNING board's facts are not the verdict
        // while its hello may still come; once identification has settled
        // without one, the board that spoke the update protocol is still
        // nothing a Flash should be offered over.
        if let Some(facts) = &self.update_facts
            && (facts.is_core_only() || settled)
        {
            return Classification::CoreOnly {
                version: facts.version.clone(),
                state: facts.state,
            };
        }
        if self.rom_download > 0 {
            return Classification::Bootloader;
        }
        if self.blank_header > 0 {
            return Classification::Blank;
        }
        // Older LightPlayer outranks the foreign banners: its `M!` lines and
        // its boot marker are LightPlayer's own words, and a hello (above)
        // still wins the moment the board speaks this Studio's link.
        let older_marker = self
            .boot_marker_proto
            .is_some_and(|proto| proto < self.expected_proto);
        if self.legacy_frame_lines > 0 || older_marker {
            return Classification::OlderLightPlayer {
                proto: self.boot_marker_proto,
            };
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

/// The wire proto a (normalized) boot marker names: `… proto=29 commit=…`.
fn boot_marker_proto(normalized: &str) -> Option<u32> {
    let rest = normalized.split(BOOT_MARKER_PROTO_FIELD).nth(1)?;
    let digits: String = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Parse a `Saved PC:0x<hex>` boot line (already lowercased), the ROM's own
/// report of where execution was when a reset landed. `None` if the line
/// carries no such field, or the hex fails to parse.
fn saved_pc(normalized: &str) -> Option<u32> {
    let rest = normalized.split("saved pc:0x").nth(1)?;
    let hex: String = rest
        .chars()
        .take_while(|character| character.is_ascii_hexdigit())
        .collect();
    u32::from_str_radix(&hex, 16).ok()
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
        ServerFrameBody::HelloOnOtherWire { proto, board_id } => format!(
            "hello · proto {proto} · {} · another wire: only its version and board were read",
            board_id.as_deref().unwrap_or("?"),
        ),
    }
}

/// `heartbeat · <project|idle>[ · FAULT <label>]`. No fps or heap: this
/// mirror crate carries no such facts on [`LoadedProjectFacts`] or
/// [`RecoveryFacts`] today (see `wire.rs`'s module doc on why the mirror
/// stays small), so the summary states only what the model actually knows.
/// One terminal line for a board manifest heard on channel 3. Carries no
/// byte counts, so a board re-reporting the same state collapses.
fn update_summary(facts: &UpdateFacts) -> String {
    let mut line = format!(
        "update · {:?} · {}",
        facts.state,
        facts.version.as_deref().unwrap_or("?")
    );
    if let Some(transfer) = &facts.transfer {
        line.push_str(&format!(" · {:?} transfer", transfer.kind));
        if transfer.busy {
            line.push_str(" (another link)");
        }
    }
    line
}

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

    /// Frames are not evidence: an app conversation's reply that strays
    /// into the fold moves nothing — not the frame count, not freshness,
    /// not the terminal.
    #[test]
    fn a_passthrough_leaves_evidence_untouched() {
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
        let before = evidence.clone();

        let notes = fold(
            &mut evidence,
            &mut identity,
            Millis(20),
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Passthrough {
                    request_id: crate::link::APP_CONVERSATION_ID_BASE + 1,
                    line: "M!{\"id\":1073741825,\"msg\":\"projectRead\"}".to_string(),
                },
            },
        );

        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(evidence, before);
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

    /// D4: a `Saved PC` inside the detected chip's bootloader ranges counts
    /// as a hang and is kept for the failure copy.
    #[test]
    fn a_saved_pc_inside_the_bootloader_range_counts_as_a_hang() {
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
            line("Saved PC:0x4086ed7a"),
        );

        assert_eq!(evidence.bootloader_hung_resets(), 1);
        assert_eq!(evidence.last_bootloader_hung_pc(), Some(0x4086ed7a));
    }

    /// The stub's own PC (`0x40800832`) is not inside the bootloader's
    /// ranges: a live board answering through the stub is not a hang.
    #[test]
    fn a_saved_pc_outside_the_bootloader_range_is_not_a_hang() {
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
            line("Saved PC:0x40800832"),
        );

        assert_eq!(evidence.bootloader_hung_resets(), 0);
        assert_eq!(evidence.last_bootloader_hung_pc(), None);
    }

    /// No chip named yet (no `ESP-ROM:` line this window): the range table
    /// has nothing to check against, so a `Saved PC` line never counts.
    #[test]
    fn a_saved_pc_with_no_detected_chip_never_counts() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(20),
            line("Saved PC:0x4086ed7a"),
        );

        assert_eq!(evidence.detected_chip(), None);
        assert_eq!(evidence.bootloader_hung_resets(), 0);
    }

    /// A board still running firmware from before the USB link moved onto
    /// lp-link (proto 29 and older) prints every message as an `M!{json}`
    /// line, which the new link reads as console text. The card must call it
    /// older LightPlayer firmware — it used to say "Unrecognized firmware" —
    /// and a hello, once the board speaks this Studio's link, still wins.
    #[test]
    fn m_bang_text_lines_are_older_light_player_firmware_until_a_hello() {
        let config = studio_config();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();

        evidence.fold(Millis(0), &opened(), &mut identity, &config);
        evidence.fold(
            Millis(10),
            &line(r#"M!{"id":0,"msg":{"heartbeat":{"frame_count":4839,"loaded_projects":[]}}}"#),
            &mut identity,
            &config,
        );
        assert_eq!(
            evidence.classification,
            Classification::OlderLightPlayer { proto: None }
        );
        assert_eq!(
            evidence.verdict_if_settled(Millis(5_000)),
            Classification::OlderLightPlayer { proto: None },
            "not Foreign, and not the pre-hello verdict"
        );

        let hello = frame(ServerFrame::hello(
            1,
            HelloFacts {
                proto: config.expected_proto,
                ..Default::default()
            },
        ));
        evidence.fold(Millis(900), &hello, &mut identity, &config);
        assert!(evidence.classification.is_light_player());
    }

    /// G1-F1: a hello from another wire that this build could not decode —
    /// only its version read — is still a hello. From an older wire it is
    /// the older-LightPlayer verdict at once (no settle wait, no "pre-hello
    /// firmware"); from a newer one, a LightPlayer this Studio is behind.
    #[test]
    fn a_hello_from_another_wire_is_a_light_player_on_that_wire() {
        let config = studio_config();

        let mut older = Evidence::default();
        let mut identity = IdentityChain::default();
        older.fold(Millis(0), &opened(), &mut identity, &config);
        older.fold(
            Millis(10),
            &frame(ServerFrame::heartbeat(None)),
            &mut identity,
            &config,
        );
        older.fold(
            Millis(20),
            &frame(ServerFrame::hello_on_other_wire(0, 32, None)),
            &mut identity,
            &config,
        );
        assert_eq!(
            older.classification,
            Classification::OlderLightPlayer { proto: Some(32) }
        );
        assert_eq!(
            older.verdict_if_settled(Millis(5_000)),
            Classification::OlderLightPlayer { proto: Some(32) },
            "never the pre-hello verdict"
        );
        assert!(older.has_hello(), "it said hello; identify settles on it");
        assert_eq!(older.hello_heard_at(), Some(Millis(20)));

        let mut newer = Evidence::default();
        let mut identity = IdentityChain::default();
        newer.fold(Millis(0), &opened(), &mut identity, &config);
        newer.fold(
            Millis(10),
            &frame(ServerFrame::hello_on_other_wire(0, 35, None)),
            &mut identity,
            &config,
        );
        assert!(
            newer.classification.is_light_player(),
            "{:?}",
            newer.classification
        );
        assert_eq!(
            newer.wire_version(),
            Some(WireVersion::BoardNewer {
                board: 35,
                studio: 34
            })
        );

        // Main's wire 33 (PR #929) is older too: a board flashed from it
        // reads as older LightPlayer firmware, like a fielded wire-32 one.
        let mut older_33 = Evidence::default();
        let mut identity = IdentityChain::default();
        older_33.fold(Millis(0), &opened(), &mut identity, &config);
        older_33.fold(
            Millis(10),
            &frame(ServerFrame::hello_on_other_wire(0, 33, None)),
            &mut identity,
            &config,
        );
        assert_eq!(
            older_33.classification,
            Classification::OlderLightPlayer { proto: Some(33) }
        );
    }

    /// G1 walk (2026-10-03): an older LightPlayer's hello names its board,
    /// and the fold keeps it — on the older verdict and the newer one alike
    /// — so the card can offer Update firmware for that board.
    #[test]
    fn a_hello_from_another_wire_names_its_board() {
        let config = studio_config();
        let board = || Some("seeed/xiao-esp32-c6".to_string());
        for proto in [32, 33, 35] {
            let mut evidence = Evidence::default();
            let mut identity = IdentityChain::default();
            evidence.fold(Millis(0), &opened(), &mut identity, &config);
            assert_eq!(evidence.hello_board_id(), None, "nothing said yet");
            evidence.fold(
                Millis(10),
                &frame(ServerFrame::hello_on_other_wire(0, proto, board())),
                &mut identity,
                &config,
            );
            assert_eq!(
                evidence.hello_board_id(),
                Some("seeed/xiao-esp32-c6"),
                "wire {proto}"
            );
        }
        let mut unstamped = Evidence::default();
        let mut identity = IdentityChain::default();
        unstamped.fold(Millis(0), &opened(), &mut identity, &config);
        unstamped.fold(
            Millis(10),
            &frame(ServerFrame::hello_on_other_wire(0, 32, None)),
            &mut identity,
            &config,
        );
        assert_eq!(unstamped.hello_board_id(), None, "no stamp, no board");
    }

    /// The hello's version against the app's own: what "older than Studio"
    /// reads. A board on this Studio's wire proto is still older when its
    /// VERSION is, and nothing is claimed before a hello or without a
    /// version.
    #[test]
    fn a_hello_names_its_firmware_age_against_studios_version() {
        let config = RosterConfig {
            expected_version: AppVersion::parse("2026.10.03-1"),
            ..studio_config()
        };
        let age_after = |version: Option<&str>| {
            let mut evidence = Evidence::default();
            let mut identity = IdentityChain::default();
            evidence.fold(Millis(0), &opened(), &mut identity, &config);
            assert_eq!(evidence.firmware_age(), None, "nothing before a hello");
            let hello = frame(ServerFrame::hello(
                1,
                HelloFacts {
                    proto: config.expected_proto,
                    version: version.map(str::to_string),
                    ..Default::default()
                },
            ));
            evidence.fold(Millis(10), &hello, &mut identity, &config);
            assert_eq!(evidence.wire_version(), Some(WireVersion::Match));
            evidence.firmware_age().expect("a hello was heard")
        };
        assert_eq!(age_after(Some("2026.10.02-3")), FirmwareAge::Older);
        assert_eq!(age_after(Some("2026.10.03-1")), FirmwareAge::Current);
        assert_eq!(age_after(Some("2026.10.04-1")), FirmwareAge::Newer);
        assert_eq!(age_after(Some("unknown")), FirmwareAge::Unknown);
        assert_eq!(age_after(None), FirmwareAge::Unknown);
    }

    /// The boot marker names its proto; an older one is older LightPlayer
    /// firmware, this build's own is not (its hello follows as a frame).
    #[test]
    fn a_boot_marker_older_than_studio_is_older_light_player_firmware() {
        const MARKER_30: &str = "[INIT] fw-esp32 initialized, starting server loop... \
                                 proto=30 commit=4caa5b658157 dirty=false";
        const MARKER_34: &str = "[INIT] fw-esp32 initialized, starting server loop... \
                                 proto=34 commit=4caa5b658157 dirty=false";
        let config = studio_config();

        let mut older = Evidence::default();
        let mut identity = IdentityChain::default();
        older.fold(Millis(0), &opened(), &mut identity, &config);
        older.fold(Millis(10), &line(MARKER_30), &mut identity, &config);
        assert_eq!(
            older.classification,
            Classification::OlderLightPlayer { proto: Some(30) }
        );

        let mut current = Evidence::default();
        let mut identity = IdentityChain::default();
        current.fold(Millis(0), &opened(), &mut identity, &config);
        current.fold(Millis(10), &line(MARKER_34), &mut identity, &config);
        assert_eq!(current.classification, Classification::Unknown);
    }

    /// A release WLED build on a classic ESP32: the ROM's reset chatter, then
    /// the one line WLED itself prints, the Adalight handshake `Ada`. The
    /// card names it, so it can offer to replace WLED.
    #[test]
    fn a_release_wled_boot_is_foreign_firmware_labelled_wled() {
        const WLED_BOOT: &[&str] = &[
            "ets Jul 29 2019 12:21:46",
            "",
            "rst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)",
            "configsip: 0, SPIWP:0xee",
            "clk_drv:0x00,q_drv:0x00,d_drv:0x00,cs0_drv:0x00,hd_drv:0x00,wp_drv:0x00",
            "mode:DIO, clock div:1",
            "load:0x3fff0030,len:1184",
            "load:0x40078000,len:13232",
            "load:0x40080400,len:3028",
            "entry 0x400805e4",
            "Ada\r",
        ];
        let config = studio_config();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        evidence.fold(Millis(0), &opened(), &mut identity, &config);
        for (at, text) in WLED_BOOT.iter().enumerate() {
            evidence.fold(Millis(10 * at as u64), &line(text), &mut identity, &config);
        }

        let wled = Classification::Foreign {
            label: Some("WLED".to_string()),
        };
        assert_eq!(evidence.classification, wled);
        assert_eq!(evidence.verdict_if_settled(Millis(5_000)), wled);
    }

    /// A `WLED_DEBUG` build says its name outright.
    #[test]
    fn a_debug_wled_banner_is_foreign_firmware_labelled_wled() {
        let config = studio_config();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        evidence.fold(Millis(0), &opened(), &mut identity, &config);
        evidence.fold(
            Millis(10),
            &line("---WLED 0.15.0 2412100 INIT---"),
            &mut identity,
            &config,
        );

        assert_eq!(
            evidence.classification,
            Classification::Foreign {
                label: Some("WLED".to_string())
            }
        );
    }

    /// `Ada` names WLED only as the whole line: a line that merely contains
    /// the word is someone else's firmware, unnamed.
    #[test]
    fn ada_inside_a_longer_line_does_not_name_wled() {
        let config = studio_config();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        evidence.fold(Millis(0), &opened(), &mut identity, &config);
        evidence.fold(
            Millis(10),
            &line("Ada fruit NeoPixel strand test"),
            &mut identity,
            &config,
        );

        assert_eq!(
            evidence.verdict_if_settled(Millis(5_000)),
            Classification::Foreign { label: None }
        );
    }

    /// A board running only its core sends no hello — only its manifest on
    /// channel 3. That is a LightPlayer waiting for its engine, at once and
    /// still after identification settles: never "no hello", never silent.
    #[test]
    fn a_core_only_manifest_with_no_hello_is_core_only_even_when_settled() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();

        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(10),
            update(update_facts(UpdateBoardState::NeedsEngine, "2026.10.05-1")),
        );

        let core_only = Classification::CoreOnly {
            version: Some("2026.10.05-1".to_string()),
            state: UpdateBoardState::NeedsEngine,
        };
        assert_eq!(evidence.classification, core_only);
        assert_eq!(evidence.verdict_if_settled(Millis(5_000)), core_only);
        assert!(evidence.announced_update_channel());
        assert!(!evidence.has_hello());
        assert_eq!(
            evidence.freshness.state,
            Liveness::Live,
            "the board speaking channel 3 is the board speaking"
        );
    }

    /// A running board's hello keeps the LightPlayer verdict, whatever its
    /// manifest says, and the manifest is still its update facts.
    #[test]
    fn a_hello_carrying_a_manifest_keeps_the_light_player_verdict() {
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
                    update: Some(update_facts(UpdateBoardState::Running, "2026.10.05-1")),
                    ..Default::default()
                },
            )),
        );

        assert!(evidence.classification.is_light_player());
        assert!(evidence.announced_update_channel());
        assert_eq!(
            evidence.update_facts().map(|facts| facts.state),
            Some(UpdateBoardState::Running)
        );
    }

    /// DM9: channel 3 is authoritative. When both the hello's copy and an
    /// `M` were heard, the `M` is the board's update facts — whichever came
    /// first.
    #[test]
    fn channel_three_wins_over_the_hellos_manifest() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();

        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(10),
            update(update_facts(UpdateBoardState::Updating, "2026.10.05-2")),
        );
        fold(
            &mut evidence,
            &mut identity,
            Millis(20),
            frame(ServerFrame::hello(
                1,
                HelloFacts {
                    proto: config.expected_proto,
                    update: Some(update_facts(UpdateBoardState::Running, "2026.10.05-1")),
                    ..Default::default()
                },
            )),
        );

        let facts = evidence.update_facts().expect("facts heard");
        assert_eq!(facts.state, UpdateBoardState::Updating);
        assert_eq!(facts.version.as_deref(), Some("2026.10.05-2"));
        assert!(
            evidence.classification.is_light_player(),
            "the hello still decides the verdict"
        );
    }

    /// The update facts are this window's, like every other observation: a
    /// board that reset is a machine we have not heard from yet.
    #[test]
    fn a_window_reset_clears_the_update_facts() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();

        fold(&mut evidence, &mut identity, Millis(0), opened());
        fold(
            &mut evidence,
            &mut identity,
            Millis(10),
            update(update_facts(UpdateBoardState::NeedsEngine, "2026.10.05-1")),
        );
        assert!(evidence.announced_update_channel());

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

        assert!(evidence.update_facts().is_none());
        assert!(!evidence.announced_update_channel());
        assert_eq!(evidence.classification, Classification::Unknown);
    }

    /// Update bytes are the update driver's: the fold moves nothing for
    /// them — no verdict, no freshness, no terminal line.
    #[test]
    fn update_bytes_leave_evidence_untouched() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(&mut evidence, &mut identity, Millis(0), opened());
        let before = evidence.clone();

        fold(
            &mut evidence,
            &mut identity,
            Millis(10),
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Update(b"Q".to_vec()),
            },
        );

        assert_eq!(evidence, before);
    }

    /// Whether the link carries channel 3 is the transport's word, kept
    /// across a board reset and forgotten on detach.
    #[test]
    fn the_update_channel_is_the_links_fact() {
        let mut evidence = Evidence::default();
        let mut identity = IdentityChain::default();
        fold(
            &mut evidence,
            &mut identity,
            Millis(0),
            Event::LinkAttached {
                link: LinkId(1),
                info: LinkInfo {
                    carries_update_channel: true,
                    ..LinkInfo::default()
                },
            },
        );
        assert!(evidence.carries_update_channel());

        fold(
            &mut evidence,
            &mut identity,
            Millis(10),
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::ResetOutcome {
                    kind: crate::link::ResetKind::Normal,
                    ok: true,
                },
            },
        );
        assert!(evidence.carries_update_channel(), "a reset keeps the link");

        fold(
            &mut evidence,
            &mut identity,
            Millis(20),
            Event::LinkDetached { link: LinkId(1) },
        );
        assert!(!evidence.carries_update_channel());
    }

    /// Studio's own roster config: this build's wire proto (34, the hello's
    /// `fs` boot state), not the model's placeholder default.
    fn studio_config() -> RosterConfig {
        RosterConfig {
            expected_proto: 34,
            ..RosterConfig::default()
        }
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

    fn update(facts: UpdateFacts) -> Event {
        Event::Link {
            link: LinkId(1),
            event: LinkEvent::UpdateFacts(facts),
        }
    }

    fn update_facts(state: UpdateBoardState, version: &str) -> UpdateFacts {
        UpdateFacts {
            state,
            version: Some(version.to_string()),
            manifest_json: "{}".to_string(),
            ..UpdateFacts::default()
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
