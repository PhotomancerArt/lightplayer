//! The scenario suite: every failure the shipped device layer produced,
//! replayed as a script against the model.
//!
//! These run through the crate's PUBLIC surface on purpose — if a scenario
//! needs internals to express, the model is not usable by the app either.
//! JSON fixtures live in `fixtures/`; the rest are builder scripts in the
//! same vocabulary.

use lpa_devices::journal::EvictionReason;
use lpa_devices::replay::{Expect, Fixture, Replay, Script, Step, replay_inputs};
use lpa_devices::view::roster_view;
use lpa_devices::{
    Action, ActivityKind, Classification, DeviceId, DeviceRecord, DeviceStatus, DeviceUid, Escape,
    IdentityChain, Input, LinkId, Liveness, Millis, Roster, RosterConfig,
};

#[test]
fn mid_stream_attach_fixture() {
    // The shipped hello-gate defect
    // (docs/defects/2026-08-21-hello-gate-assumes-fresh-boot.md) as a replay
    // test: heartbeats arrive before the hello answer, and nothing condemns
    // the device for it.
    run_fixture(include_str!("../fixtures/mid-stream-attach.json"));
}

#[test]
fn boot_banner_attach_fixture() {
    run_fixture(include_str!("../fixtures/boot-banner-attach.json"));
}

#[test]
fn a_silent_link_is_identified_by_its_silence() {
    let config = RosterConfig::default();
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));

    // Nothing at all — no lines, no frames.
    replay.advance_to(Millis(config.identify_deadline_ms + 200));

    let pending = &replay.roster().pending()[0];
    assert!(!pending.is_identifying(), "the deadline settled it");
    assert!(
        matches!(pending.verdict(), Some(Classification::Quiet { .. })),
        "verdict: {:?}",
        pending.verdict()
    );
    let view = replay.view();
    assert_eq!(view.pending[0].escapes, vec![Escape::Forget]);
    assert!(view.pending[0].can_adopt, "a blank chip is still adoptable");
}

#[test]
fn unplugging_mid_activity_evicts_and_refolds() {
    let mut replay = ready_device();
    let device = first_device(&replay);
    replay.feed(Millis(2_000), Input::Action(Action::Identify { device }));
    assert!(replay.roster().device(device).expect("device").is_busy());

    replay.step(Millis(2_100), Step::detach(1));

    let entry = replay.roster().device(device).expect("device");
    assert!(!entry.is_busy(), "the activity went with the link");
    assert_eq!(entry.status(), DeviceStatus::Offline);
    assert_eq!(
        entry.evidence.classification,
        Classification::Unknown,
        "the verdict is re-derived, not latched"
    );
    assert!(
        notes(&replay).iter().any(|note| note.contains("LinkLost")),
        "eviction reason is journaled: {:?}",
        notes(&replay)
    );
}

#[test]
fn a_cancel_the_activity_honors_inside_the_grace_is_not_an_eviction() {
    let config = RosterConfig::default();
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    let device = replay.roster().pending()[0].device_id();

    replay.feed(
        Millis(500),
        Input::Action(Action::CancelActivity { device }),
    );
    // The transport gives the port back promptly.
    replay.step(Millis(700), Step::closed(1));

    assert!(!replay.roster().pending()[0].is_identifying());
    assert!(
        !notes(&replay)
            .iter()
            .any(|note| note.contains("ActivityEvicted")),
        "a polite wind-down is not an eviction: {:?}",
        notes(&replay)
    );
    assert!(notes(&replay).iter().any(|note| note.contains("Cancelled")));
}

#[test]
fn a_cancel_that_hangs_is_evicted_after_the_grace_with_recovery_commands() {
    let config = RosterConfig::default();
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    let device = replay.roster().pending()[0].device_id();

    replay.feed(
        Millis(500),
        Input::Action(Action::CancelActivity { device }),
    );
    let before = replay.commands().len();

    // The port never reports the close: the wedged-port case that used to
    // need a page refresh.
    replay.advance_to(Millis(500 + config.cancel_grace_ms + 10));

    assert!(!replay.roster().pending()[0].is_identifying());
    assert!(
        notes(&replay).iter().any(|note| {
            note.contains("ActivityEvicted") && note.contains("CancelGraceExpired")
        }),
        "eviction is journaled with its reason: {:?}",
        notes(&replay)
    );
    let recovery: Vec<String> = replay.commands()[before..]
        .iter()
        .map(|(_, command)| format!("{command:?}"))
        .collect();
    assert!(
        recovery.iter().any(|command| command.contains("Close"))
            && recovery.iter().any(|command| command.contains("Open")),
        "recovery rebuilds the link: {recovery:?}"
    );
    // And the eviction reason is a real enum, not a string.
    assert_eq!(
        format!("{:?}", EvictionReason::CancelGraceExpired),
        "CancelGraceExpired"
    );
}

#[test]
fn forget_works_from_every_representative_state() {
    // 1. Pending (dismiss).
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    let commands = replay.feed(
        Millis(100),
        Input::Action(Action::DismissLink { link: LinkId(1) }),
    );
    assert!(replay.roster().pending().is_empty());
    assert!(revokes_a_grant(&commands), "{commands:?}");

    // 2. Attached and idle.
    let mut replay = ready_device();
    let device = first_device(&replay);
    let commands = replay.feed(Millis(3_000), Input::Action(Action::Forget { device }));
    assert!(replay.roster().devices().is_empty());
    assert!(
        deletes_a_record(&commands) && revokes_a_grant(&commands),
        "{commands:?}"
    );

    // 3. Mid-activity.
    let mut replay = ready_device();
    let device = first_device(&replay);
    replay.feed(Millis(3_000), Input::Action(Action::Identify { device }));
    assert!(replay.roster().device(device).expect("device").is_busy());
    let commands = replay.feed(Millis(3_100), Input::Action(Action::Forget { device }));
    assert!(replay.roster().devices().is_empty());
    assert!(
        deletes_a_record(&commands) && revokes_a_grant(&commands),
        "{commands:?}"
    );
    assert!(
        notes(&replay)
            .iter()
            .any(|note| note.contains("DeviceForgotten"))
    );

    // 4. Detached — and anonymous, which the shipped system could never
    //    forget (forget required a uid).
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(40), Step::line(1, "invalid header: 0xffffffff"));
    replay.advance_to(Millis(6_000));
    replay.feed(
        Millis(6_100),
        Input::Action(Action::AdoptLink { link: LinkId(1) }),
    );
    let device = first_device(&replay);
    assert!(
        replay
            .roster()
            .device(device)
            .expect("device")
            .identity
            .is_anonymous(),
        "the adopted board has no uid"
    );
    replay.step(Millis(6_200), Step::detach(1));
    let commands = replay.feed(Millis(6_300), Input::Action(Action::Forget { device }));
    assert!(replay.roster().devices().is_empty());
    assert!(deletes_a_record(&commands), "{commands:?}");
}

#[test]
fn identity_promotes_from_endpoint_to_uid() {
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(Millis(0), Step::attach(1, "usb-1"));

    let pending = &replay.roster().pending()[0];
    assert!(
        pending.identity().endpoint.is_some(),
        "endpoint bound on attach"
    );
    assert!(pending.identity().uid.is_none());
    assert!(pending.identity().is_anonymous());

    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(200), Step::hello(1).uid("dev_2f8a"));

    let device = replay
        .roster()
        .device(first_device(&replay))
        .expect("device");
    assert_eq!(device.identity.uid, Some(DeviceUid("dev_2f8a".to_string())));
    assert!(!device.identity.is_anonymous());
    assert!(
        notes(&replay)
            .iter()
            .any(|note| { note.contains("IdentityPromoted") && note.contains("Uid") }),
        "promotion is a journaled operation: {:?}",
        notes(&replay)
    );
}

#[test]
fn an_anonymous_entry_merges_into_the_record_matched_device() {
    // The user adopted a blank board last week and named it; today it is
    // provisioned and hellos with a uid that belongs to another entry.
    let config = RosterConfig::default();
    let mut roster = Roster::new(config);
    roster.load_records(vec![DeviceRecord {
        name: Some("Kitchen".to_string()),
        ..DeviceRecord::new(
            DeviceId(1),
            IdentityChain {
                uid: Some(DeviceUid("dev_2f8a".to_string())),
                ..Default::default()
            },
        )
    }]);
    let mut replay = Replay::with_roster(roster);

    replay.step(Millis(0), Step::attach(7, "usb-7"));
    replay.step(Millis(20), Step::opened(7));
    replay.step(Millis(200), Step::hello(7).uid("dev_2f8a"));

    assert_eq!(replay.roster().devices().len(), 1, "one board, one card");
    let device = &replay.roster().devices()[0];
    assert_eq!(device.id, DeviceId(1), "the record-holding entry survives");
    assert_eq!(device.intent.name.as_deref(), Some("Kitchen"));
    assert!(device.evidence.classification.is_light_player());
    assert!(
        notes(&replay)
            .iter()
            .any(|note| note.contains("DevicesMerged"))
    );
}

#[test]
fn a_lossy_wire_never_flaps_the_timeline() {
    let config = RosterConfig::default();
    let mut replay = ready_device_with(config);
    let device = first_device(&replay);

    // Heartbeats every 5 s with one dropped. quiet_after is 12 s, so a
    // single missed beat is invisible.
    for at in [5_000_u64, 10_000, 20_000] {
        replay.step(Millis(at), Step::heartbeat(1));
    }
    assert_eq!(
        count_notes(&replay, "WentQuiet"),
        0,
        "one dropped heartbeat is not a transition: {:?}",
        notes(&replay)
    );
    assert_eq!(
        replay
            .roster()
            .device(device)
            .expect("device")
            .evidence
            .freshness
            .state,
        Liveness::Live
    );

    // Now the device really goes away.
    replay.advance_to(Millis(40_000));
    assert_eq!(count_notes(&replay, "WentQuiet"), 1);
    let view = replay.view();
    assert!(
        view.devices[0]
            .freshness_label
            .as_deref()
            .unwrap_or_default()
            .contains("quiet"),
        "the card reads honestly: {:?}",
        view.devices[0].freshness_label
    );

    // And comes back.
    replay.step(Millis(41_000), Step::heartbeat(1));
    assert_eq!(count_notes(&replay, "CameBack"), 1);
    assert_eq!(count_notes(&replay, "WentQuiet"), 1, "no repeat");
}

#[test]
fn an_outcome_survives_disconnect_and_clears_when_superseded() {
    let mut replay = ready_device();
    let device = first_device(&replay);

    let view = replay.view();
    assert!(view.devices[0].last_outcome.as_ref().expect("outcome").ok);

    replay.step(Millis(3_000), Step::detach(1));
    let view = replay.view();
    assert!(
        view.devices[0].last_outcome.is_some(),
        "outcomes survive disconnect (I4)"
    );

    // A new activity supersedes it.
    replay.step(Millis(4_000), Step::attach(1, "usb-1"));
    assert!(replay.roster().device(device).expect("device").is_busy());
    assert!(
        replay
            .roster()
            .device(device)
            .expect("device")
            .evidence
            .last_outcome
            .is_none(),
        "a new activity clears the old outcome"
    );
}

#[test]
fn a_busy_device_refuses_a_second_activity() {
    let mut replay = ready_device();
    let device = first_device(&replay);
    replay.feed(Millis(3_000), Input::Action(Action::Identify { device }));
    assert_eq!(
        replay
            .roster()
            .device(device)
            .expect("device")
            .activity_kind(),
        Some(ActivityKind::Identify)
    );

    let commands = replay.feed(Millis(3_010), Input::Action(Action::Identify { device }));

    assert!(
        commands.is_empty(),
        "one activity per device (I5): {commands:?}"
    );
    assert_eq!(count_notes(&replay, "ActivityStarted"), 2, "not three");
}

#[test]
fn replaying_a_journal_reproduces_the_journal_and_the_projection() {
    let fixture = full_walk_fixture();
    let mut first = Replay::new(RosterConfig::default());
    first.run(&fixture).expect("first run");

    let inputs = first.roster().journal().replay_inputs();
    let replayed = replay_inputs(RosterConfig::default(), &inputs);

    let original_entries: Vec<_> = first.roster().journal().entries().cloned().collect();
    let replayed_entries: Vec<_> = replayed.journal().entries().cloned().collect();
    assert_eq!(
        original_entries, replayed_entries,
        "the journal is reproducible from its own inputs"
    );
    assert_eq!(
        roster_view(first.roster(), first.now()),
        roster_view(&replayed, first.now()),
        "and so is the projection"
    );

    // Running the same fixture twice from scratch must agree too.
    let mut second = Replay::new(RosterConfig::default());
    second.run(&fixture).expect("second run");
    assert_eq!(
        original_entries,
        second
            .roster()
            .journal()
            .entries()
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(first.view(), second.view());
}

#[test]
fn a_proto_mismatch_reads_honestly_instead_of_hanging() {
    let config = RosterConfig::default();
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(
        Millis(200),
        Step::hello(1)
            .proto(config.expected_proto + 7)
            .uid("dev_old"),
    );

    let view = replay.view();
    assert_eq!(view.devices.len(), 1);
    assert!(
        view.devices[0].state_label.contains("Incompatible"),
        "state: {:?}",
        view.devices[0].state_label
    );
    assert!(view.devices[0].escapes.contains(&Escape::Forget));
}

#[test]
fn a_reset_reopens_the_question_instead_of_latching_a_verdict() {
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(40), Step::line(1, "invalid header: 0xffffffff"));
    replay.advance_to(Millis(6_000));
    assert_eq!(
        replay.roster().pending()[0].verdict(),
        Some(&Classification::Blank)
    );

    // Somebody flashed it and reset it. Non-sticky: the window reopens.
    replay.feed(
        Millis(6_100),
        Input::link(
            LinkId(1),
            lpa_devices::LinkEvent::ResetOutcome {
                kind: lpa_devices::ResetKind::Normal,
                ok: true,
            },
        ),
    );

    assert_eq!(
        replay.roster().pending()[0].evidence().classification,
        Classification::Unknown,
        "a reboot invalidates the verdict"
    );
}

/// The M2 walk: a blank board is flashed FROM ITS PENDING CARD. The gesture
/// adopts the link, the preflight MAC identity-joins the anonymous entry
/// (and earns a registry write), progress reaches the projection, the
/// ladder's reopen finds the boot hello, the board manifest is stamped, and
/// the activity ends Ready.
#[test]
fn flashing_a_blank_pending_link_adopts_joins_identity_and_lands_ready() {
    let config = RosterConfig::default();
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(40), Step::line(1, "ESP-ROM:esp32c6-20220919"));
    replay.step(Millis(60), Step::line(1, "invalid header: 0xffffffff"));
    replay.advance_to(Millis(6_000));
    let view = replay.view();
    assert!(view.pending[0].needs_firmware, "{:?}", view.pending[0]);
    assert_eq!(view.pending[0].detected_chip.as_deref(), Some("esp32c6"));
    let device = view.pending[0].device;

    // The flash gesture: adopt + spawn + the coarse effect command.
    let commands = replay.step(
        Millis(6_100),
        Step::Flash {
            device: device.0,
            board: "seeed-xiao-esp32c6".to_string(),
            build: "esp32c6-4mb".to_string(),
        },
    );
    assert!(replay.roster().pending().is_empty(), "flash adopts");
    assert_eq!(replay.roster().devices().len(), 1);
    assert!(
        commands.iter().any(|command| matches!(
            command,
            lpa_devices::Command::RunEffect {
                effect: lpa_devices::EffectRequest::Flash { .. },
                ..
            }
        )),
        "{commands:?}"
    );
    let view = replay.view();
    assert_eq!(
        view.devices[0]
            .activity
            .as_ref()
            .map(|activity| activity.kind),
        Some(ActivityKind::Flash)
    );

    // The preflight reads the efuse MAC: the anonymous entry becomes a
    // rememberable identity, and the record write follows.
    let commands = replay.step(
        Millis(7_000),
        Step::MacObserved {
            device: device.0,
            mac: "60:55:f9:0a:0b:0c".to_string(),
        },
    );
    assert!(
        commands.iter().any(
            |command| matches!(command, lpa_devices::Command::PersistRecord(record)
                if record.identity.mac.as_ref().is_some_and(|mac| mac.0 == "60:55:f9:0a:0b:0c"))
        ),
        "the MAC join persists: {commands:?}"
    );

    // Progress reaches the projection.
    replay.step(
        Millis(20_000),
        Step::EffectProgress {
            device: device.0,
            label: "Writing firmware".to_string(),
            percent: Some(62),
        },
    );
    let view = replay.view();
    assert_eq!(
        view.devices[0]
            .activity
            .as_ref()
            .and_then(|activity| activity.percent),
        Some(62)
    );

    // The effect ends; the ladder reopens; the new firmware hellos.
    let commands = replay.step(
        Millis(60_000),
        Step::EffectEnded {
            device: device.0,
            ok: true,
            message: None,
        },
    );
    assert!(
        commands.iter().any(|command| matches!(
            command,
            lpa_devices::Command::Link {
                command: lpa_devices::LinkCommand::Open { .. },
                ..
            }
        )),
        "the ladder starts by reopening: {commands:?}"
    );
    replay.step(Millis(61_000), Step::opened(1));
    let commands = replay.step(
        Millis(62_000),
        Step::hello(1).uid("dev_2f8a").board("seeed-xiao-esp32c6"),
    );
    assert!(
        commands.iter().any(|command| matches!(
            command,
            lpa_devices::Command::RunEffect {
                effect: lpa_devices::EffectRequest::WriteBoardManifest { board_id },
                ..
            } if board_id == "seeed-xiao-esp32c6"
        )),
        "the hello proves the app protocol; the D4 stamp follows: {commands:?}"
    );

    replay.step(
        Millis(62_500),
        Step::EffectEnded {
            device: device.0,
            ok: true,
            message: Some("manifest written".to_string()),
        },
    );

    let view = replay.view();
    assert_eq!(
        view.devices[0].state_label, "Ready",
        "{:?}",
        view.devices[0]
    );
    assert!(!view.devices[0].needs_firmware);
    let outcome = view.devices[0].last_outcome.as_ref().expect("an outcome");
    assert!(outcome.ok, "{outcome:?}");
    assert!(
        outcome.summary.contains("firmware installed"),
        "{outcome:?}"
    );
    assert_eq!(count_notes(&replay, "ActivityEnded"), 2, "identify + flash");
}

/// Scripted post-flash silence: the ladder escalates reopen → Normal →
/// BothThenDrop and then fails with the honest replug/Reconnect guidance
/// (V3/CH340: a replug kills the grant).
#[test]
fn a_silent_board_after_a_flash_climbs_the_ladder_then_fails_honestly() {
    let config = RosterConfig::default();
    let mut replay = ready_device_with(config);
    let device = first_device(&replay);
    replay.step(
        Millis(2_000),
        Step::Flash {
            device: device.0,
            board: "dig-uno".to_string(),
            build: "esp32-4mb".to_string(),
        },
    );
    replay.step(
        Millis(30_000),
        Step::EffectEnded {
            device: device.0,
            ok: true,
            message: None,
        },
    );
    replay.step(Millis(30_100), Step::opened(1));

    // Silence through every rung. The runner fires the scheduled pokes and
    // escalations on the way.
    let exhausted = 30_000 + 3 * config.flash_rung_ms + 1_000;
    replay.advance_to(Millis(exhausted));

    let commands: Vec<&lpa_devices::Command> = replay
        .commands()
        .iter()
        .map(|(_, command)| command)
        .collect();
    let resets: Vec<&lpa_devices::ResetKind> = commands
        .iter()
        .filter_map(|command| match command {
            lpa_devices::Command::Link {
                command: lpa_devices::LinkCommand::RunReset(kind),
                ..
            } => Some(kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        resets,
        vec![
            &lpa_devices::ResetKind::Normal,
            &lpa_devices::ResetKind::BothThenDrop
        ],
        "the escalation order is the bench-proven one"
    );

    let view = replay.view();
    assert!(!view.devices[0].status.eq(&DeviceStatus::Busy));
    let outcome = view.devices[0].last_outcome.as_ref().expect("an outcome");
    assert!(!outcome.ok);
    assert!(
        outcome.summary.contains("Reconnect"),
        "honest guidance: {outcome:?}"
    );
}

/// Forget works mid-flash (I3): the entry, record and grant go, and the
/// journal shows an eviction — cancellation bounded by removal.
#[test]
fn forget_mid_flash_evicts_and_cleans_up() {
    let mut replay = ready_device();
    let device = first_device(&replay);
    replay.step(
        Millis(2_000),
        Step::Flash {
            device: device.0,
            board: "dig-uno".to_string(),
            build: "esp32-4mb".to_string(),
        },
    );
    assert!(replay.roster().device(device).expect("device").is_busy());

    let commands = replay.step(Millis(10_000), Step::Forget { device: device.0 });

    assert!(replay.roster().devices().is_empty());
    assert!(deletes_a_record(&commands));
    assert!(revokes_a_grant(&commands));
    assert_eq!(count_notes(&replay, "ActivityEvicted"), 1);
}

// ---------------------------------------------------------------- helpers

fn run_fixture(json: &str) {
    let fixture = Fixture::from_json(json).expect("fixture parses");
    let mut replay = Replay::new(RosterConfig::default());
    if let Err(failure) = replay.run(&fixture) {
        panic!("{failure}\nview: {:#?}", replay.view());
    }
}

/// A device that has attached, opened, hello'd and finished identifying.
fn ready_device() -> Replay {
    ready_device_with(RosterConfig::default())
}

fn ready_device_with(config: RosterConfig) -> Replay {
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(200), Step::hello(1).uid("dev_2f8a").board("dig-uno"));
    assert_eq!(replay.roster().devices().len(), 1, "setup: one device");
    replay
}

fn first_device(replay: &Replay) -> DeviceId {
    replay.roster().devices()[0].id
}

fn notes(replay: &Replay) -> Vec<String> {
    replay.journal_notes()
}

fn count_notes(replay: &Replay, needle: &str) -> usize {
    notes(replay)
        .iter()
        .filter(|note| note.contains(needle))
        .count()
}

fn revokes_a_grant(commands: &[lpa_devices::Command]) -> bool {
    commands
        .iter()
        .any(|command| matches!(command, lpa_devices::Command::RevokeGrant(_)))
}

fn deletes_a_record(commands: &[lpa_devices::Command]) -> bool {
    commands
        .iter()
        .any(|command| matches!(command, lpa_devices::Command::DeleteRecord(_)))
}

/// A script that exercises every stream: attach, boot noise, heartbeats, a
/// hello, a rename, a cancel, an unplug and a forget.
fn full_walk_fixture() -> Fixture {
    Script::new()
        .at(0, Step::attach(1, "usb-1"))
        .expect(Expect::new().pending(1).devices(0))
        .at(20, Step::opened(1))
        .at(60, Step::line(1, "ESP-ROM:esp32c6-20220919"))
        .at(120, Step::heartbeat(1))
        .at(400, Step::frame(1, "UnloadProject"))
        .at(900, Step::hello(1).uid("dev_2f8a").board("dig-uno"))
        .expect(Expect::new().devices(1).pending(0).device_state("Ready"))
        .at(
            1_200,
            Step::SetName {
                device: 1,
                name: "Kitchen".to_string(),
            },
        )
        .at(2_000, Step::Identify { device: 1 })
        .expect(Expect::new().busy(true))
        .at(2_400, Step::Cancel { device: 1 })
        .at(9_000, Step::Advance)
        .at(9_500, Step::heartbeat(1))
        .at(30_000, Step::Advance)
        .at(30_500, Step::detach(1))
        .at(31_000, Step::Forget { device: 1 })
        .expect(Expect::new().devices(0).pending(0))
        .into_fixture("full walk")
}
