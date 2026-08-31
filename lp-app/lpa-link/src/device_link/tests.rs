//! End-to-end on the host: a real [`Roster`] driven through a real
//! [`Link`] over the scripted fake device.
//!
//! Nothing here fakes at the model's own vocabulary. Every assertion is
//! reached through `M!` bytes on a wire, the real line splitter, the real
//! frame mapping and a real host `LpServer` — because every device bug so far
//! lived below the record level, and a test that hands the fold ready-made
//! `LinkEvent`s (`lpa-devices`' own fixtures do that, correctly, at that
//! layer) cannot see framing or timing at all.
//!
//! # The bench is a miniature effects layer
//!
//! [`Bench`] is the smallest honest version of what M3's studio-core slice
//! does for real: it
//! executes `Command::Link` on the adapter, arms `Command::StartTimer` on a
//! real clock, records the record/grant commands, and pumps `poll_event` back
//! in as `Event::Link`. It exists so THIS slice can prove the adapter drives
//! the model, without pre-empting the effects layer's own home. It follows
//! invariant I7 by construction: the loop never awaits device IO — the
//! adapter never blocks.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use lpa_devices::event::{Action, Command, Event, Input};
use lpa_devices::evidence::{Classification, IncompatibleReason};
use lpa_devices::link::{Link, LinkId, LinkInfo};
use lpa_devices::record::DeviceRecord;
use lpa_devices::roster::{Roster, RosterConfig};
use lpa_devices::time::{Millis, TimerId};
use lpa_devices::view::{Escape, RosterView, roster_view};

use crate::device_link::fake::{FakeDeviceLink, fake_device_link, fake_link_info};
use crate::device_link::wire::roster_config;
use crate::providers::fake_device::{
    FakeBootState, FakeDeviceIdentity, FakeDeviceScript, FakeEsp32Device, FakeLightPlayerState,
};
use crate::stream::DeviceByteStream;

/// Wall-clock ceiling for any single `run_until`. Generous: the fake boots a
/// real server on a real thread. A test that hits this has found a hang,
/// which is the failure this milestone exists to make impossible.
const RUN_TIMEOUT: Duration = Duration::from_secs(10);

/// One link, one roster, one clock.
struct Bench {
    roster: Roster,
    link: FakeDeviceLink,
    link_id: LinkId,
    started: Instant,
    /// Armed timers as (due, id). The model keeps one per scope and drops
    /// superseded fires itself, so the bench does no bookkeeping beyond
    /// delivering them in order.
    timers: Vec<(Millis, TimerId)>,
    persisted: Vec<DeviceRecord>,
    deleted: Vec<lpa_devices::identity::DeviceId>,
    revoked: Vec<LinkInfo>,
}

impl Bench {
    fn new(device: &FakeEsp32Device, endpoint: &str) -> Self {
        Self::with_config(device, endpoint, roster_config())
    }

    fn with_config(device: &FakeEsp32Device, endpoint: &str, config: RosterConfig) -> Self {
        let info = fake_link_info(endpoint);
        Self {
            roster: Roster::new(config),
            link: fake_device_link(info, device),
            link_id: LinkId(1),
            started: Instant::now(),
            timers: Vec::new(),
            persisted: Vec::new(),
            deleted: Vec::new(),
            revoked: Vec::new(),
        }
    }

    fn now(&self) -> Millis {
        Millis(
            u64::try_from(self.started.elapsed().as_millis())
                .expect("a test does not run for 500 million years"),
        )
    }

    /// Attach the link, exactly as a granted-port discovery would.
    fn attach(&mut self) {
        let info = self.link.info().clone();
        let link = self.link_id;
        self.feed(Input::Event(Event::LinkAttached { link, info }));
    }

    fn feed(&mut self, input: Input) {
        let now = self.now();
        let commands = self.roster.handle(now, input);
        self.apply(commands);
    }

    fn apply(&mut self, commands: Vec<Command>) {
        for command in commands {
            match command {
                Command::Link { link, command } if link == self.link_id => {
                    self.link.submit(command);
                }
                Command::Link { .. } => panic!("a command for a link the bench does not own"),
                Command::StartTimer { timer, after_ms } => {
                    let due = self.now().plus_ms(after_ms);
                    self.timers.push((due, timer));
                }
                Command::RequestUsbGrant => {}
                Command::PersistRecord(record) => self.persisted.push(record),
                Command::DeleteRecord(device) => self.deleted.push(device),
                Command::RevokeGrant(info) => self.revoked.push(info),
            }
        }
    }

    /// One turn of the loop: deliver everything the wire said, then every
    /// timer that came due.
    fn step(&mut self) {
        let mut events = VecDeque::new();
        while let Some(event) = self.link.poll_event() {
            events.push_back(event);
        }
        for event in events {
            let link = self.link_id;
            self.feed(Input::Event(Event::Link { link, event }));
        }

        let now = self.now();
        let due: Vec<TimerId> = self
            .timers
            .iter()
            .filter(|(at, _)| *at <= now)
            .map(|(_, timer)| *timer)
            .collect();
        self.timers.retain(|(at, _)| *at > now);
        for timer in due {
            self.feed(Input::Event(Event::TimerFired { timer }));
        }
    }

    /// Step until `ready` holds, or fail. The only sleep in the bench: the
    /// fake's server runs on its own thread, so there is genuinely nothing to
    /// do between polls.
    fn run_until(&mut self, what: &str, ready: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + RUN_TIMEOUT;
        loop {
            self.step();
            if ready(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; journal:\n{}",
                self.journal_dump()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn view(&self) -> RosterView {
        roster_view(&self.roster, self.now())
    }

    fn journal_dump(&self) -> String {
        self.roster
            .journal()
            .notes()
            .map(|(at, note)| format!("  {at:?} {note:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A LightPlayer board with a stamped identity and a MAC — the everyday case.
fn light_player(uid: &str) -> FakeEsp32Device {
    FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new(uid, "Bench board"))
            .with_base_mac("60:55:f9:0a:0b:0c"),
    )))
}

/// Drain whatever the device has already put on the wire, so the next link to
/// open it lands MID-STREAM: the boot banner and the unsolicited id-0 hello
/// are already gone, exactly like connecting to a board that has been running
/// for an hour.
fn run_past_the_boot_hello(device: &FakeEsp32Device) {
    let mut stream = crate::providers::fake_device::FakeDeviceByteStream::new(device.clone());
    let mut buf = [0u8; 4096];
    let mut seen = String::new();
    let deadline = Instant::now() + RUN_TIMEOUT;
    while !seen.contains("\"hello\"") {
        let read = stream.read_available(&mut buf).expect("the fake is alive");
        seen.push_str(&String::from_utf8_lossy(&buf[..read]));
        assert!(
            Instant::now() < deadline,
            "the fake never said hello; saw: {seen}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The everyday connect: attach → the roster opens the port and identifies →
/// a real hello over real framing settles it as a LightPlayer, with a record
/// to persist and a card to draw.
#[test]
fn a_fresh_connect_identifies_a_light_player_through_the_wire() {
    let device = light_player("dev_bench01");
    let mut bench = Bench::new(&device, "usb-bench-1");

    bench.attach();

    // Before any bytes: the roster's own "new device found, identifying…".
    assert_eq!(bench.roster.pending().len(), 1);
    assert!(bench.roster.pending()[0].is_identifying());
    assert!(bench.roster.devices().is_empty());

    bench.run_until("the hello to settle the pending link", |bench| {
        !bench.roster.devices().is_empty()
    });

    assert!(
        bench.roster.pending().is_empty(),
        "an identified link becomes a device"
    );
    let device_entry = &bench.roster.devices()[0];
    assert!(
        device_entry.evidence.classification.is_light_player(),
        "{:?}",
        device_entry.evidence.classification
    );
    assert!(device_entry.evidence.has_hello());

    // Identity came off the wire: the stamped uid and the efuse MAC, the
    // latter normalized by the adapter.
    assert_eq!(
        device_entry.identity.uid,
        Some(lpa_devices::identity::DeviceUid("dev_bench01".to_string()))
    );
    assert_eq!(
        device_entry.identity.mac,
        Some(lpa_devices::identity::MacAddress(
            "60:55:f9:0a:0b:0c".to_string()
        ))
    );

    // The boot banner reached the fold as diagnosis, not as a verdict.
    assert_eq!(device_entry.evidence.detected_chip(), Some("esp32c6"));

    // The effects layer was told to remember it, and the card is drawable
    // with a way out.
    assert!(!bench.persisted.is_empty(), "a new device earns a record");
    let view = bench.view();
    assert_eq!(view.devices.len(), 1);
    assert!(view.devices[0].escapes.contains(&Escape::Forget));
}

/// A device that is already running has ALREADY sent its unsolicited hello,
/// so the connect must ask — and the answer, arriving after heartbeat-shaped
/// traffic, must still classify it correctly. This is the hello-gate defect
/// (`docs/defects/2026-08-21-hello-gate-assumes-fresh-boot.md`) driven
/// through the real wire.
#[test]
fn a_mid_stream_connect_is_classified_by_the_answer_to_our_own_hello() {
    let device = light_player("dev_midstream");
    run_past_the_boot_hello(&device);
    let mut bench = Bench::new(&device, "usb-bench-2");

    bench.attach();
    bench.run_until("the hello ANSWER to arrive", |bench| {
        !bench.roster.devices().is_empty()
    });

    let device_entry = &bench.roster.devices()[0];
    assert!(
        device_entry.evidence.classification.is_light_player(),
        "a running board that answers is a LightPlayer: {:?}",
        device_entry.evidence.classification
    );
}

/// The response-starvation wire (the 2026-08-24 request-idle defect): id-0
/// heartbeats keep flowing while every correlated response dies at the wire.
/// The old system hung here. The model must instead reach the identify
/// deadline and say something honest — frames flowed, no hello ever came —
/// with the card still escapable.
///
/// Since R4a those heartbeats also carry identity, so the honest outcome is
/// now BETTER than it was: the board is named (uid off the unsolicited
/// channel) while its verdict stays the truthful "never said hello". Naming
/// it is what moves it out of `pending` and onto a real card.
#[test]
fn dropped_responses_settle_as_honest_evidence_instead_of_hanging() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new("dev_starved", "Starved board"))
            .with_dropped_responses()
            // A live wire that never answers: heartbeats defeat any
            // frame-gap timeout, so only the settle deadline can end this.
            .with_heartbeat_interval(Duration::from_millis(50)),
    )));
    // Mid-stream: the unsolicited hello (id 0, which the drop does not touch)
    // is already gone, so the ONLY road to a verdict is the answer that never
    // comes.
    run_past_the_boot_hello(&device);

    let config = RosterConfig {
        // Short budgets keep the test quick; the shape is the deadline, not
        // its size.
        identify_deadline_ms: 600,
        hello_request_interval_ms: 100,
        ..roster_config()
    };
    let mut bench = Bench::with_config(&device, "usb-bench-3", config);

    bench.attach();
    bench.run_until("identification to settle without a hello", |bench| {
        bench
            .roster
            .devices()
            .first()
            .is_some_and(|device| device.evidence.classification.is_verdict())
    });

    let device_entry = &bench.roster.devices()[0];
    assert_eq!(
        device_entry.evidence.classification,
        Classification::Incompatible {
            reason: IncompatibleReason::NoHello
        },
        "frames flowed and no hello came: that is the honest verdict"
    );
    assert!(
        device_entry.evidence.frames_seen() > 0,
        "the heartbeats were absorbed as live-peer evidence"
    );
    assert!(!device_entry.evidence.has_hello());
    assert!(
        device_entry.activity.is_none(),
        "the deadline ended it; nothing is still waiting"
    );

    // R4a: the heartbeats named it, so the board is no longer the anonymous
    // stuck card the shipped system could never escape — and the projection
    // still offers a way out.
    assert_eq!(
        device_entry.identity.uid,
        Some(lpa_devices::identity::DeviceUid("dev_starved".to_string())),
        "a starved wire still says who it is, every heartbeat"
    );
    let view = bench.view();
    assert!(
        view.pending.is_empty(),
        "a named board is not a pending link"
    );
    assert_eq!(view.devices.len(), 1);
    assert!(view.devices[0].escapes.contains(&Escape::Forget));
}

/// A starved wire that HEALS: the same link, the same roster, and a re-asked
/// Identify reaches the truth. Proof the verdict is not sticky and that a
/// timed-out identification did not poison the transport.
#[test]
fn a_healed_wire_re_identifies_without_a_new_link() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new("dev_healed", "Healed board"))
            .with_dropped_responses()
            .with_heartbeat_interval(Duration::from_millis(50)),
    )));
    run_past_the_boot_hello(&device);
    let config = RosterConfig {
        identify_deadline_ms: 600,
        hello_request_interval_ms: 100,
        ..roster_config()
    };
    let mut bench = Bench::with_config(&device, "usb-bench-4", config);

    bench.attach();
    // R4a names it off the heartbeats, so the settled entry is a device
    // card carrying the honest "never said hello" verdict.
    bench.run_until("the starved identification to settle", |bench| {
        bench.roster.devices().first().is_some_and(|device| {
            matches!(
                device.evidence.classification,
                Classification::Incompatible { .. }
            )
        })
    });

    device.set_drop_responses(false);
    // The user's escape from a stale verdict: ask again.
    let starved_device = bench.roster.devices()[0].id;
    bench.feed(Input::Action(Action::Identify {
        device: starved_device,
    }));
    bench.run_until("the healed wire to answer", |bench| {
        bench
            .roster
            .devices()
            .first()
            .is_some_and(|device| device.evidence.classification.is_light_player())
    });

    let device_entry = &bench.roster.devices()[0];
    assert!(
        device_entry.evidence.classification.is_light_player(),
        "a verdict is a function of the CURRENT window: {:?}",
        device_entry.evidence.classification
    );
    assert_eq!(
        device_entry.identity.uid,
        Some(lpa_devices::identity::DeviceUid("dev_healed".to_string()))
    );
}

/// Blank flash: the ROM's `invalid header` loop is a verdict the fold reaches
/// from LINES alone, with no protocol frame anywhere — and because a blank
/// chip may never identify itself, the entry stays a pending link the user
/// can adopt rather than vanishing.
#[test]
fn a_blank_board_is_diagnosed_from_boot_lines_and_stays_adoptable() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::BlankFlash));
    let config = RosterConfig {
        identify_deadline_ms: 600,
        hello_request_interval_ms: 100,
        ..roster_config()
    };
    let mut bench = Bench::with_config(&device, "usb-bench-5", config);

    bench.attach();
    bench.run_until("the blank-flash verdict", |bench| {
        bench
            .roster
            .pending()
            .first()
            .is_some_and(|pending| pending.evidence().classification == Classification::Blank)
    });

    let pending = &bench.roster.pending()[0];
    assert_eq!(pending.evidence().classification, Classification::Blank);
    assert_eq!(pending.evidence().detected_chip(), Some("esp32c6"));
    assert_eq!(pending.evidence().frames_seen(), 0, "no frames, only lines");

    bench.feed(Input::Action(Action::AdoptLink {
        link: bench.link_id,
    }));
    assert_eq!(bench.roster.devices().len(), 1);
    assert!(bench.roster.devices()[0].intent.setup_requested);
    assert!(!bench.persisted.is_empty());
}

/// Forget from a live, identified device: the entry, its record and its grant
/// all go, and the effects layer is told about each — including the
/// `RevokeGrant` without which the next page load re-created the row the user
/// just deleted.
#[test]
fn forgetting_an_identified_device_gives_the_grant_back() {
    let device = light_player("dev_forget");
    let mut bench = Bench::new(&device, "usb-bench-6");
    bench.attach();
    bench.run_until("identification", |bench| !bench.roster.devices().is_empty());
    let device_id = bench.roster.devices()[0].id;

    bench.feed(Input::Action(Action::Forget { device: device_id }));

    assert!(bench.roster.devices().is_empty());
    assert!(bench.deleted.contains(&device_id));
    assert_eq!(bench.revoked.len(), 1, "the port stops being ours");
    assert!(bench.view().devices.is_empty());
}
