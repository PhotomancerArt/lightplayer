//! The fixture-replay harness: the crate's primary test surface.
//!
//! A fixture is a timestamped, interleaved script of actions and events with
//! projection assertions at marked steps. The runner owns a virtual clock:
//! [`Command::StartTimer`] is scheduled, and time only advances when the
//! script says so — at which point every due timer fires, in order, before
//! the next scripted input. That makes a whole scenario deterministic and
//! makes "what did the card say at t=3.2 s" an assertion instead of a
//! guess.
//!
//! Two ways to write one, with the same vocabulary:
//!
//! ```
//! use lpa_devices::replay::{Expect, Replay, Script, Step};
//! use lpa_devices::RosterConfig;
//!
//! let script = Script::new()
//!     .at(0, Step::attach(1, "usb-1"))
//!     .at(10, Step::opened(1))
//!     // A RUNNING server heartbeats before any hello answer arrives.
//!     .at(100, Step::heartbeat(1))
//!     .expect(Expect::new().pending(1))
//!     .at(200, Step::hello(1).uid("dev_abc"))
//!     .expect(Expect::new().devices(1).device_state("Ready"));
//!
//! let mut replay = Replay::new(RosterConfig::default());
//! replay.run(&script.into_fixture("mid-stream attach")).expect("scenario");
//! ```
//!
//! …or the same thing as JSON (`fixtures/*.json`), which is what
//! [`Fixture::from_json`] parses. The step vocabulary is deliberately
//! compact so a fixture is readable by a human triaging a bug, not just by
//! serde.

use serde::{Deserialize, Serialize};

use crate::activity::ActivityKind;
use crate::device::DeviceStatus;
use crate::event::{Action, Command, Event, Input};
use crate::identity::{DeviceId, DeviceUid, EndpointKey, MacAddress, PeerIdentity};
use crate::link::{LinkEvent, LinkId, LinkInfo, ResetKind};
use crate::roster::{Roster, RosterConfig};
use crate::time::{Millis, TimerId};
use crate::view::{DeviceView, Escape, PendingLinkView, RosterView};
use crate::wire::{HelloFacts, ServerFrame};

/// A named script with assertions.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Fixture {
    pub name: String,
    pub steps: Vec<FixtureStep>,
}

impl Fixture {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("fixtures are always serializable")
    }
}

/// One scripted moment: what happened, and what the projection must say
/// afterwards.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixtureStep {
    pub at_ms: u64,
    #[serde(rename = "do")]
    pub step: Step,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<Expect>,
}

/// The compact step vocabulary. Each variant maps to exactly one
/// [`Input`] (or, for [`Step::Advance`], to nothing but the passage of
/// time).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Attach {
        link: u64,
        endpoint: String,
        #[serde(default)]
        label: Option<String>,
    },
    Detach {
        link: u64,
    },
    /// The port opened. Reuses the [`LinkInfo`] the roster already holds for
    /// this link, so a fixture cannot accidentally re-bind the endpoint (and
    /// journal a phantom identity conflict) by naming it differently twice.
    Opened {
        link: u64,
        #[serde(default)]
        endpoint: Option<String>,
    },
    Closed {
        link: u64,
        #[serde(default)]
        reason: Option<String>,
    },
    Line {
        link: u64,
        text: String,
    },
    /// An id-0 heartbeat, optionally stamped with identity (vision R4).
    Heartbeat {
        link: u64,
        #[serde(default)]
        uid: Option<String>,
        #[serde(default)]
        mac: Option<String>,
    },
    Hello {
        link: u64,
        #[serde(default)]
        uid: Option<String>,
        #[serde(default)]
        mac: Option<String>,
        /// Defaults to the roster's expected proto; set it to force a
        /// mismatch.
        #[serde(default)]
        proto: Option<u32>,
        #[serde(default)]
        board: Option<String>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Any other decoded frame: live-peer evidence, never a verdict.
    Frame {
        link: u64,
        label: String,
    },
    ResetOutcome {
        link: u64,
        ok: bool,
    },
    Error {
        link: u64,
        message: String,
    },
    AddFromUsb,
    Adopt {
        link: u64,
    },
    Dismiss {
        link: u64,
    },
    Connect {
        device: u64,
    },
    Disconnect {
        device: u64,
    },
    Forget {
        device: u64,
    },
    Cancel {
        device: u64,
    },
    Identify {
        device: u64,
    },
    SetName {
        device: u64,
        name: String,
    },
    /// Let the clock move and due timers fire, with no scripted input.
    Advance,
}

impl Step {
    pub fn attach(link: u64, endpoint: &str) -> Self {
        Self::Attach {
            link,
            endpoint: endpoint.to_string(),
            label: None,
        }
    }

    pub fn opened(link: u64) -> Self {
        Self::Opened {
            link,
            endpoint: None,
        }
    }

    pub fn closed(link: u64) -> Self {
        Self::Closed { link, reason: None }
    }

    pub fn detach(link: u64) -> Self {
        Self::Detach { link }
    }

    pub fn line(link: u64, text: &str) -> Self {
        Self::Line {
            link,
            text: text.to_string(),
        }
    }

    pub fn heartbeat(link: u64) -> Self {
        Self::Heartbeat {
            link,
            uid: None,
            mac: None,
        }
    }

    pub fn hello(link: u64) -> Self {
        Self::Hello {
            link,
            uid: None,
            mac: None,
            proto: None,
            board: None,
            name: None,
        }
    }

    pub fn frame(link: u64, label: &str) -> Self {
        Self::Frame {
            link,
            label: label.to_string(),
        }
    }

    /// Attach a uid to a `hello` or `heartbeat` step.
    pub fn uid(mut self, value: &str) -> Self {
        match &mut self {
            Self::Hello { uid, .. } | Self::Heartbeat { uid, .. } => {
                *uid = Some(value.to_string());
            }
            _ => panic!("uid() only applies to hello/heartbeat steps"),
        }
        self
    }

    /// Force a wire proto on a `hello` step.
    pub fn proto(mut self, value: u32) -> Self {
        match &mut self {
            Self::Hello { proto, .. } => *proto = Some(value),
            _ => panic!("proto() only applies to hello steps"),
        }
        self
    }

    /// Set the board label on a `hello` step.
    pub fn board(mut self, value: &str) -> Self {
        match &mut self {
            Self::Hello { board, .. } => *board = Some(value.to_string()),
            _ => panic!("board() only applies to hello steps"),
        }
        self
    }

    fn into_input(self, roster: &Roster) -> Option<Input> {
        let config = roster.config();
        Some(match self {
            Self::Attach {
                link,
                endpoint,
                label,
            } => Input::Event(Event::LinkAttached {
                link: LinkId(link),
                info: link_info(&endpoint, label.as_deref()),
            }),
            Self::Detach { link } => Input::Event(Event::LinkDetached { link: LinkId(link) }),
            Self::Opened { link, endpoint } => {
                let info = endpoint
                    .map(|endpoint| link_info(&endpoint, None))
                    .or_else(|| roster.link_info(LinkId(link)).cloned())
                    .unwrap_or_else(|| link_info(&format!("usb-{link}"), None));
                Input::link(LinkId(link), LinkEvent::Opened { info })
            }
            Self::Closed { link, reason } => Input::link(
                LinkId(link),
                LinkEvent::Closed {
                    reason: reason.unwrap_or_else(|| "closed".to_string()),
                },
            ),
            Self::Line { link, text } => Input::link(LinkId(link), LinkEvent::Line(text)),
            Self::Heartbeat { link, uid, mac } => Input::link(
                LinkId(link),
                LinkEvent::Frame(ServerFrame::heartbeat(peer_identity(uid, mac, None))),
            ),
            Self::Hello {
                link,
                uid,
                mac,
                proto,
                board,
                name,
            } => Input::link(
                LinkId(link),
                LinkEvent::Frame(ServerFrame::hello(
                    1,
                    HelloFacts {
                        proto: proto.unwrap_or(config.expected_proto),
                        identity: peer_identity(uid, mac, name).unwrap_or_default(),
                        firmware: Some("fw-esp32c6 test".to_string()),
                        board_id: board,
                    },
                )),
            ),
            Self::Frame { link, label } => {
                Input::link(LinkId(link), LinkEvent::Frame(ServerFrame::other(7, label)))
            }
            Self::ResetOutcome { link, ok } => Input::link(
                LinkId(link),
                LinkEvent::ResetOutcome {
                    kind: ResetKind::Normal,
                    ok,
                },
            ),
            Self::Error { link, message } => Input::link(LinkId(link), LinkEvent::Error(message)),
            Self::AddFromUsb => Input::Action(Action::AddFromUsb),
            Self::Adopt { link } => Input::Action(Action::AdoptLink { link: LinkId(link) }),
            Self::Dismiss { link } => Input::Action(Action::DismissLink { link: LinkId(link) }),
            Self::Connect { device } => Input::Action(Action::Connect {
                device: DeviceId(device),
            }),
            Self::Disconnect { device } => Input::Action(Action::Disconnect {
                device: DeviceId(device),
            }),
            Self::Forget { device } => Input::Action(Action::Forget {
                device: DeviceId(device),
            }),
            Self::Cancel { device } => Input::Action(Action::CancelActivity {
                device: DeviceId(device),
            }),
            Self::Identify { device } => Input::Action(Action::Identify {
                device: DeviceId(device),
            }),
            Self::SetName { device, name } => Input::Action(Action::SetName {
                device: DeviceId(device),
                name,
            }),
            Self::Advance => return None,
        })
    }
}

/// Assertions on the projection at one step. Everything is optional; only
/// what a fixture states is checked.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Expect {
    pub devices: Option<usize>,
    pub pending: Option<usize>,
    /// Which device card to assert on (default: the first).
    pub device_index: Option<usize>,
    pub device_status: Option<DeviceStatus>,
    pub device_state: Option<String>,
    pub device_title: Option<String>,
    pub device_detail_contains: Option<String>,
    pub freshness_contains: Option<String>,
    pub busy: Option<bool>,
    pub activity: Option<ActivityKind>,
    pub cancel_requested: Option<bool>,
    pub escapes: Option<Vec<Escape>>,
    pub outcome_ok: Option<bool>,
    pub outcome_contains: Option<String>,
    pub pending_index: Option<usize>,
    pub pending_state: Option<String>,
    pub pending_can_adopt: Option<bool>,
    pub pending_escapes: Option<Vec<Escape>>,
    /// Substrings that must appear in the debug form of the journal's notes.
    pub journal_notes: Option<Vec<String>>,
    /// Substrings that must NOT appear in the journal's notes.
    pub journal_notes_absent: Option<Vec<String>>,
}

impl Expect {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn devices(mut self, count: usize) -> Self {
        self.devices = Some(count);
        self
    }

    pub fn pending(mut self, count: usize) -> Self {
        self.pending = Some(count);
        self
    }

    pub fn device_state(mut self, label: &str) -> Self {
        self.device_state = Some(label.to_string());
        self
    }

    pub fn device_status(mut self, status: DeviceStatus) -> Self {
        self.device_status = Some(status);
        self
    }

    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = Some(busy);
        self
    }

    pub fn escapes(mut self, escapes: &[Escape]) -> Self {
        self.escapes = Some(escapes.to_vec());
        self
    }

    pub fn outcome_contains(mut self, needle: &str) -> Self {
        self.outcome_contains = Some(needle.to_string());
        self
    }

    pub fn outcome_ok(mut self, ok: bool) -> Self {
        self.outcome_ok = Some(ok);
        self
    }

    pub fn pending_state(mut self, label: &str) -> Self {
        self.pending_state = Some(label.to_string());
        self
    }

    pub fn journal_notes(mut self, needles: &[&str]) -> Self {
        self.journal_notes = Some(needles.iter().map(|needle| needle.to_string()).collect());
        self
    }

    pub fn journal_notes_absent(mut self, needles: &[&str]) -> Self {
        self.journal_notes_absent = Some(needles.iter().map(|needle| needle.to_string()).collect());
        self
    }

    pub fn freshness_contains(mut self, needle: &str) -> Self {
        self.freshness_contains = Some(needle.to_string());
        self
    }

    fn check(&self, view: &RosterView, notes: &[String]) -> Result<(), String> {
        if let Some(expected) = self.devices {
            require(view.devices.len() == expected, || {
                format!("expected {expected} devices, saw {}", view.devices.len())
            })?;
        }
        if let Some(expected) = self.pending {
            require(view.pending.len() == expected, || {
                format!(
                    "expected {expected} pending links, saw {}",
                    view.pending.len()
                )
            })?;
        }
        if self.wants_device() {
            let index = self.device_index.unwrap_or(0);
            let device = view
                .devices
                .get(index)
                .ok_or_else(|| format!("no device card at index {index}"))?;
            self.check_device(device)?;
        }
        if self.wants_pending() {
            let index = self.pending_index.unwrap_or(0);
            let pending = view
                .pending
                .get(index)
                .ok_or_else(|| format!("no pending link at index {index}"))?;
            self.check_pending(pending)?;
        }
        if let Some(needles) = &self.journal_notes {
            for needle in needles {
                require(notes.iter().any(|note| note.contains(needle)), || {
                    format!("journal has no note containing {needle:?}")
                })?;
            }
        }
        if let Some(needles) = &self.journal_notes_absent {
            for needle in needles {
                require(!notes.iter().any(|note| note.contains(needle)), || {
                    format!("journal unexpectedly contains a note matching {needle:?}")
                })?;
            }
        }
        Ok(())
    }

    fn check_device(&self, device: &DeviceView) -> Result<(), String> {
        if let Some(expected) = &self.device_state {
            require(&device.state_label == expected, || {
                format!("expected state {expected:?}, saw {:?}", device.state_label)
            })?;
        }
        if let Some(expected) = self.device_status {
            require(device.status == expected, || {
                format!("expected status {expected:?}, saw {:?}", device.status)
            })?;
        }
        if let Some(expected) = &self.device_title {
            require(&device.title == expected, || {
                format!("expected title {expected:?}, saw {:?}", device.title)
            })?;
        }
        if let Some(needle) = &self.device_detail_contains {
            let detail = device.detail.clone().unwrap_or_default();
            require(detail.contains(needle), || {
                format!("expected detail containing {needle:?}, saw {detail:?}")
            })?;
        }
        if let Some(needle) = &self.freshness_contains {
            let label = device.freshness_label.clone().unwrap_or_default();
            require(label.contains(needle), || {
                format!("expected freshness containing {needle:?}, saw {label:?}")
            })?;
        }
        if let Some(expected) = self.busy {
            require(device.activity.is_some() == expected, || {
                format!("expected busy={expected}, saw {:?}", device.activity)
            })?;
        }
        if let Some(expected) = self.activity {
            let kind = device.activity.as_ref().map(|activity| activity.kind);
            require(kind == Some(expected), || {
                format!("expected activity {expected:?}, saw {kind:?}")
            })?;
        }
        if let Some(expected) = self.cancel_requested {
            let requested = device
                .activity
                .as_ref()
                .map(|activity| activity.cancel_requested)
                .unwrap_or(false);
            require(requested == expected, || {
                format!("expected cancel_requested={expected}, saw {requested}")
            })?;
        }
        if let Some(expected) = &self.escapes {
            require(&device.escapes == expected, || {
                format!("expected escapes {expected:?}, saw {:?}", device.escapes)
            })?;
        }
        if let Some(expected) = self.outcome_ok {
            let ok = device.last_outcome.as_ref().map(|outcome| outcome.ok);
            require(ok == Some(expected), || {
                format!("expected outcome ok={expected}, saw {ok:?}")
            })?;
        }
        if let Some(needle) = &self.outcome_contains {
            let summary = device
                .last_outcome
                .as_ref()
                .map(|outcome| outcome.summary.clone())
                .unwrap_or_default();
            require(summary.contains(needle), || {
                format!("expected outcome containing {needle:?}, saw {summary:?}")
            })?;
        }
        Ok(())
    }

    fn check_pending(&self, pending: &PendingLinkView) -> Result<(), String> {
        if let Some(expected) = &self.pending_state {
            require(&pending.state_label == expected, || {
                format!(
                    "expected pending state {expected:?}, saw {:?}",
                    pending.state_label
                )
            })?;
        }
        if let Some(expected) = self.pending_can_adopt {
            require(pending.can_adopt == expected, || {
                format!("expected can_adopt={expected}, saw {}", pending.can_adopt)
            })?;
        }
        if let Some(expected) = &self.pending_escapes {
            require(&pending.escapes == expected, || {
                format!(
                    "expected pending escapes {expected:?}, saw {:?}",
                    pending.escapes
                )
            })?;
        }
        Ok(())
    }

    fn wants_device(&self) -> bool {
        self.device_status.is_some()
            || self.device_state.is_some()
            || self.device_title.is_some()
            || self.device_detail_contains.is_some()
            || self.freshness_contains.is_some()
            || self.busy.is_some()
            || self.activity.is_some()
            || self.cancel_requested.is_some()
            || self.escapes.is_some()
            || self.outcome_ok.is_some()
            || self.outcome_contains.is_some()
    }

    fn wants_pending(&self) -> bool {
        self.pending_state.is_some()
            || self.pending_can_adopt.is_some()
            || self.pending_escapes.is_some()
    }
}

/// Builder for a [`Fixture`].
#[derive(Clone, Debug, Default)]
pub struct Script {
    steps: Vec<FixtureStep>,
}

impl Script {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a step at an absolute millisecond stamp.
    pub fn at(mut self, at_ms: u64, step: Step) -> Self {
        self.steps.push(FixtureStep {
            at_ms,
            step,
            expect: None,
        });
        self
    }

    /// Assert on the projection right after the previous step.
    pub fn expect(mut self, expect: Expect) -> Self {
        let step = self
            .steps
            .last_mut()
            .expect("expect() needs a step to attach to");
        step.expect = Some(expect);
        self
    }

    pub fn into_fixture(self, name: &str) -> Fixture {
        Fixture {
            name: name.to_string(),
            steps: self.steps,
        }
    }
}

/// Why a fixture failed, with the step that failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayFailure {
    pub fixture: String,
    pub step: usize,
    pub at: Millis,
    pub message: String,
}

impl std::fmt::Display for ReplayFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "fixture {:?} failed at step {} (t={} ms): {}",
            self.fixture, self.step, self.at.0, self.message
        )
    }
}

impl std::error::Error for ReplayFailure {}

/// The runner: a roster plus a virtual clock and a timer queue.
pub struct Replay {
    roster: Roster,
    clock: Millis,
    scheduled: Vec<Scheduled>,
    commands: Vec<(Millis, Command)>,
}

impl Replay {
    pub fn new(config: RosterConfig) -> Self {
        Self {
            roster: Roster::new(config),
            clock: Millis(0),
            scheduled: Vec::new(),
            commands: Vec::new(),
        }
    }

    /// A runner over an already-populated roster (records loaded, say).
    pub fn with_roster(roster: Roster) -> Self {
        Self {
            roster,
            clock: Millis(0),
            scheduled: Vec::new(),
            commands: Vec::new(),
        }
    }

    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    pub fn roster_mut(&mut self) -> &mut Roster {
        &mut self.roster
    }

    pub fn now(&self) -> Millis {
        self.clock
    }

    pub fn view(&self) -> RosterView {
        crate::view::roster_view(&self.roster, self.clock)
    }

    /// Every command the run has produced, with the instant it was produced.
    pub fn commands(&self) -> &[(Millis, Command)] {
        &self.commands
    }

    /// Move the clock forward, firing every timer that comes due on the way.
    pub fn advance_to(&mut self, at: Millis) {
        loop {
            self.scheduled
                .sort_by_key(|scheduled| (scheduled.due, scheduled.timer.seq));
            let Some(index) = self
                .scheduled
                .iter()
                .position(|scheduled| scheduled.due <= at)
            else {
                break;
            };
            let scheduled = self.scheduled.remove(index);
            self.clock = self.clock.max(scheduled.due);
            let input = Input::Event(Event::TimerFired {
                timer: scheduled.timer,
            });
            self.dispatch(input);
        }
        self.clock = self.clock.max(at);
    }

    /// Feed one scripted [`Step`] at an absolute instant.
    pub fn step(&mut self, at: Millis, step: Step) -> Vec<Command> {
        match step.into_input(&self.roster) {
            Some(input) => self.feed(at, input),
            None => {
                self.advance_to(at);
                Vec::new()
            }
        }
    }

    /// Feed one input at an absolute instant, firing any timers due first.
    pub fn feed(&mut self, at: Millis, input: Input) -> Vec<Command> {
        self.advance_to(at);
        self.clock = at;
        self.dispatch(input)
    }

    /// Run a whole fixture, checking every stated expectation.
    pub fn run(&mut self, fixture: &Fixture) -> Result<(), ReplayFailure> {
        for (index, step) in fixture.steps.iter().enumerate() {
            let at = Millis(step.at_ms);
            let input = step.step.clone().into_input(&self.roster);
            match input {
                Some(input) => {
                    self.feed(at, input);
                }
                None => self.advance_to(at),
            }
            if let Some(expect) = &step.expect {
                let view = self.view();
                let notes = self.journal_notes();
                expect
                    .check(&view, &notes)
                    .map_err(|message| ReplayFailure {
                        fixture: fixture.name.clone(),
                        step: index,
                        at,
                        message,
                    })?;
            }
        }
        Ok(())
    }

    /// Debug strings for every journal note, for `journal_notes`
    /// expectations.
    pub fn journal_notes(&self) -> Vec<String> {
        self.roster
            .journal()
            .notes()
            .map(|(_, note)| format!("{note:?}"))
            .collect()
    }

    fn dispatch(&mut self, input: Input) -> Vec<Command> {
        let commands = self.roster.handle(self.clock, input);
        for command in &commands {
            if let Command::StartTimer { timer, after_ms } = command {
                self.scheduled.push(Scheduled {
                    due: self.clock.plus_ms(*after_ms),
                    timer: *timer,
                });
            }
            self.commands.push((self.clock, command.clone()));
        }
        commands
    }
}

struct Scheduled {
    due: Millis,
    timer: TimerId,
}

/// Replay a journal's recorded inputs through a fresh roster.
///
/// **No timer synthesis**: `TimerFired` events are already in the recorded
/// stream, and the timer generations regenerate identically because the same
/// inputs mint the same generations. That is what makes replay
/// bit-comparable — feed a journal back and you must get the same journal.
pub fn replay_inputs(config: RosterConfig, inputs: &[(Millis, Input)]) -> Roster {
    let mut roster = Roster::new(config);
    for (at, input) in inputs {
        roster.handle(*at, input.clone());
    }
    roster
}

fn link_info(endpoint: &str, label: Option<&str>) -> LinkInfo {
    LinkInfo {
        label: label.unwrap_or(endpoint).to_string(),
        endpoint: EndpointKey(endpoint.to_string()),
        usb: None,
        serial_number: None,
    }
}

fn peer_identity(
    uid: Option<String>,
    mac: Option<String>,
    name: Option<String>,
) -> Option<PeerIdentity> {
    let identity = PeerIdentity {
        uid: uid.map(DeviceUid),
        mac: mac.map(MacAddress),
        name,
    };
    if identity.is_empty() {
        return None;
    }
    Some(identity)
}

fn require(condition: bool, message: impl FnOnce() -> String) -> Result<(), String> {
    if condition {
        return Ok(());
    }
    Err(message())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runner_fires_due_timers_before_the_next_scripted_input() {
        let config = RosterConfig::default();
        let mut replay = Replay::new(config);
        replay.step(Millis(0), Step::attach(1, "usb-1"));
        replay.step(Millis(10), Step::opened(1));

        // Nothing answers. Advancing past the identify deadline must let the
        // activity settle by itself.
        replay.advance_to(Millis(config.identify_deadline_ms + 500));

        assert!(
            !replay.roster().pending()[0].is_identifying(),
            "the deadline settled the question without a scripted timer"
        );
    }

    #[test]
    fn a_failed_expectation_names_the_step() {
        let fixture = Script::new()
            .at(0, Step::attach(1, "usb-1"))
            .expect(Expect::new().devices(1))
            .into_fixture("bad expectation");

        let failure = Replay::new(RosterConfig::default())
            .run(&fixture)
            .expect_err("must fail");

        assert_eq!(failure.step, 0);
        assert!(failure.message.contains("expected 1 devices"));
        assert!(failure.to_string().contains("bad expectation"));
    }

    #[test]
    fn fixtures_round_trip_through_json() {
        let fixture = Script::new()
            .at(0, Step::attach(1, "usb-1"))
            .at(10, Step::opened(1))
            .at(20, Step::hello(1).uid("dev_abc"))
            .expect(Expect::new().devices(1).device_state("Ready"))
            .into_fixture("round trip");

        let json = fixture.to_json();
        let parsed = Fixture::from_json(&json).expect("parse");

        assert_eq!(parsed.name, "round trip");
        assert_eq!(parsed.steps.len(), 3);
        Replay::new(RosterConfig::default())
            .run(&parsed)
            .expect("the parsed fixture behaves the same");
    }
}
