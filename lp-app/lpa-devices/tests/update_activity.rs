//! The Update activity, as event scripts against the public surface.
//!
//! An over-the-air update is one flow on the card, but the link under it
//! goes down and comes back up to three times (the board resets into its
//! new core, its trial, and its engine). These scripts are those resets:
//! the activity must survive them, wait for the board's own word that it is
//! back — a hello, or a core-only board's manifest alone — and end on the
//! driver's outcome, or honestly when the board never comes back.

use lpa_devices::replay::{Replay, Step};
use lpa_devices::view::DeviceView;
use lpa_devices::{
    Action, ActivityKind, Command, DeviceStatus, EffectRequest, Escape, Input, LinkCommand, LinkId,
    Millis, RosterConfig, UpdateBoardState, UpdateIntentFacts, UpdateOutcomeFacts,
    UpdateStageFacts,
};

/// The whole flow: a core leg, the board resetting into its new core
/// (core-only — it says only `M`), the engine leg, the board resetting into
/// its engine (a hello), the confirming leg, and the driver's "up to date".
#[test]
fn a_three_leg_update_survives_two_resets_and_ends_up_to_date() {
    let mut replay = ready_board();
    let commands = replay.step(Millis(1_000), update(install()));
    assert_eq!(
        update_legs(&commands),
        1,
        "the first leg runs: {commands:?}"
    );
    let card = card_of(&replay);
    let activity = card.activity.as_ref().expect("an activity");
    assert_eq!(activity.kind, ActivityKind::Update);
    assert_eq!(
        activity.update.as_ref().map(|update| update.intent.clone()),
        Some(install())
    );

    replay.step(Millis(1_100), stage(UpdateStageFacts::Updating, 400, 1_000));
    let card = card_of(&replay);
    assert_eq!(card.activity.as_ref().and_then(|a| a.percent), Some(40));
    assert_eq!(card.state_label, "Updating firmware…");

    // Reset 1: the board boots its new core and the port re-enumerates.
    replay.step(Millis(2_000), Step::closed(1));
    let commands = replay.step(Millis(2_001), leg_interrupted());
    assert!(
        opens(&commands),
        "the gap starts by reopening the port: {commands:?}"
    );
    let card = card_of(&replay);
    assert_eq!(card.status, DeviceStatus::Busy, "a reset is not a failure");
    assert_eq!(card.state_label, "Updating firmware…");
    assert_eq!(card.activity.as_ref().and_then(|a| a.percent), Some(40));
    assert!(
        card.activity
            .as_ref()
            .and_then(|a| a.update.as_ref())
            .is_some_and(|update| update.between_legs)
    );
    assert!(card.last_outcome.is_none(), "a leg's end is not an outcome");
    assert!(
        card.terminal
            .iter()
            .any(|line| line.text.starts_with("reconnecting")),
        "the terminal says it is reconnecting: {:?}",
        card.terminal
    );

    // The core-only board's only word is its manifest: that is it back.
    replay.step(Millis(3_000), Step::opened(1));
    let commands = replay.step(
        Millis(3_100),
        Step::update_facts(1, UpdateBoardState::OnTrial),
    );
    assert_eq!(update_legs(&commands), 1, "leg two: {commands:?}");
    replay.step(
        Millis(3_200),
        stage(UpdateStageFacts::Finishing, 900, 1_000),
    );

    // Reset 2: the board boots its engine and says hello.
    replay.step(Millis(5_000), Step::closed(1));
    replay.step(Millis(5_001), leg_interrupted());
    replay.step(Millis(6_000), Step::opened(1));
    let commands = replay.step(
        Millis(6_100),
        Step::hello(1)
            .uid("dev_2f8a")
            .board("seeed-xiao-esp32c6")
            .with_update(UpdateBoardState::Running),
    );
    assert_eq!(update_legs(&commands), 1, "leg three: {commands:?}");

    replay.step(Millis(6_200), outcome(UpdateOutcomeFacts::UpToDate));
    replay.step(Millis(6_201), leg_ended_ok());

    let card = card_of(&replay);
    assert!(card.activity.is_none(), "{card:?}");
    assert_eq!(card.status, DeviceStatus::Ready);
    let ended = card.last_outcome.as_ref().expect("an outcome");
    assert!(ended.ok, "{ended:?}");
    assert_eq!(card.last_update_outcome, Some(UpdateOutcomeFacts::UpToDate));
    assert_eq!(update_legs_so_far(&replay), 3);
}

/// A gap that runs out ends the activity honestly: the board keeps its
/// place, and the card says so on the evidence's own face — never on
/// "Offline" alone.
#[test]
fn a_gap_that_times_out_ends_honestly_and_never_reads_offline() {
    let mut replay = ready_board();
    replay.step(Millis(1_000), update(UpdateIntentFacts::Auto));
    replay.step(Millis(1_100), stage(UpdateStageFacts::Updating, 400, 1_000));
    replay.step(Millis(2_000), Step::closed(1));
    replay.step(Millis(2_001), leg_interrupted());

    // Still waiting just short of the deadline.
    replay.advance_to(Millis(2_001 + lpa_devices::activity::UPDATE_GAP_MS - 10));
    assert_eq!(card_of(&replay).status, DeviceStatus::Busy);

    replay.advance_to(Millis(2_001 + lpa_devices::activity::UPDATE_GAP_MS + 10));
    let card = card_of(&replay);
    assert!(card.activity.is_none(), "the gap is bounded: {card:?}");
    assert_eq!(
        card.last_update_outcome,
        Some(UpdateOutcomeFacts::BoardDidNotComeBack)
    );
    let ended = card.last_outcome.as_ref().expect("an outcome");
    assert!(!ended.ok);
    assert!(ended.summary.contains("did not come back"), "{ended:?}");
    assert!(ended.summary.contains("reconnect"), "{ended:?}");
    assert_ne!(card.state_label, "Offline");
    assert!(card.escapes.contains(&Escape::Forget));
}

/// Cancel is offered while nothing on the board has changed — before the
/// first stage and while backing up — and ends the activity at once,
/// abandoning the running leg (the effects layer drops the driver).
#[test]
fn cancel_while_backing_up_ends_the_update_and_abandons_the_leg() {
    let mut replay = ready_board();
    replay.step(Millis(1_000), update(install()));
    assert!(cancellable(&replay), "before the first stage");
    replay.step(
        Millis(1_100),
        stage(UpdateStageFacts::BackingUp, 100, 1_000),
    );
    assert!(cancellable(&replay));
    assert!(card_of(&replay).escapes.contains(&Escape::Cancel));

    let commands = replay.step(Millis(1_200), Step::Cancel { device: 1 });
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::AbandonEffect { .. })),
        "the leg is abandoned: {commands:?}"
    );
    let card = card_of(&replay);
    assert!(card.activity.is_none());
    assert_eq!(
        card.last_outcome.as_ref().map(|o| o.summary.as_str()),
        Some("cancelled")
    );
}

/// Once writing starts there is no Cancel: the card offers none, and a
/// cancel that arrives anyway is refused — never held into an eviction.
#[test]
fn cancel_is_refused_once_the_update_writes() {
    let config = RosterConfig::default();
    let mut replay = ready_board();
    replay.step(Millis(1_000), update(install()));
    replay.step(
        Millis(1_100),
        stage(UpdateStageFacts::BackingUp, 1_000, 1_000),
    );
    replay.step(Millis(1_200), stage(UpdateStageFacts::Updating, 0, 1_000));
    assert!(!cancellable(&replay));
    assert!(!card_of(&replay).escapes.contains(&Escape::Cancel));

    replay.step(Millis(1_300), Step::Cancel { device: 1 });
    replay.advance_to(Millis(1_300 + config.cancel_grace_ms + 1_000));
    let card = card_of(&replay);
    assert_eq!(
        card.activity.as_ref().map(|activity| activity.kind),
        Some(ActivityKind::Update),
        "the update keeps going"
    );
    assert!(!card.activity.as_ref().is_some_and(|a| a.cancel_requested));
    assert!(
        !replay.journal_notes().iter().any(
            |note| note.contains("ActivityCancelRequested") || note.contains("ActivityEvicted")
        ),
        "refused, not held: {:?}",
        replay.journal_notes()
    );
}

/// A native-USB C6 re-enumerates on every software reset: the port
/// disappears (the link detaches) and reappears as a new link on the same
/// endpoint. The update survives both, the card never reads offline, and
/// the next leg runs on the new link.
#[test]
fn a_native_usb_re_enumeration_mid_update_keeps_the_update() {
    let mut replay = ready_board();
    replay.step(Millis(1_000), update(install()));
    replay.step(Millis(1_100), stage(UpdateStageFacts::Updating, 400, 1_000));

    replay.step(Millis(2_000), Step::detach(1));
    let card = card_of(&replay);
    assert_eq!(card.status, DeviceStatus::Busy, "{card:?}");
    assert_eq!(card.state_label, "Updating firmware…");
    assert_eq!(card.activity.as_ref().and_then(|a| a.percent), Some(40));
    assert!(
        !replay
            .journal_notes()
            .iter()
            .any(|note| note.contains("ActivityEvicted")),
        "a vanished link is a gap, not lost ground"
    );

    replay.step(Millis(2_500), Step::attach_with_update_channel(2, "usb-1"));
    assert_eq!(replay.roster().devices().len(), 1, "one board, one card");
    assert!(replay.roster().pending().is_empty());
    let before = replay.commands().len();
    replay.advance_to(Millis(4_000));
    assert!(
        replay.commands()[before..]
            .iter()
            .any(|(_, command)| matches!(
                command,
                Command::Link {
                    link: LinkId(2),
                    command: LinkCommand::Open { .. },
                }
            )),
        "the gap knocks on the new link"
    );
    replay.step(Millis(4_100), Step::opened(2));
    let commands = replay.step(
        Millis(4_200),
        Step::update_facts(2, UpdateBoardState::OnTrial),
    );
    assert!(
        commands.iter().any(|command| matches!(
            command,
            Command::RunEffect {
                link: LinkId(2),
                effect: EffectRequest::Update { .. },
                ..
            }
        )),
        "the next leg runs on the new link: {commands:?}"
    );

    replay.step(Millis(4_300), outcome(UpdateOutcomeFacts::UpToDate));
    replay.step(Millis(4_301), leg_ended_ok());
    assert_eq!(
        card_of(&replay).last_update_outcome,
        Some(UpdateOutcomeFacts::UpToDate)
    );
}

/// The same re-enumeration arriving the other way: the new link generation
/// attaches before the old one says goodbye (the roster supersedes it in
/// place). The update survives that too.
#[test]
fn a_new_link_generation_superseding_the_old_keeps_the_update() {
    let mut replay = ready_board();
    replay.step(Millis(1_000), update(UpdateIntentFacts::Auto));
    replay.step(Millis(1_100), stage(UpdateStageFacts::Updating, 400, 1_000));
    replay.step(Millis(2_000), Step::attach_with_update_channel(2, "usb-1"));
    assert_eq!(
        card_of(&replay).activity.as_ref().map(|a| a.kind),
        Some(ActivityKind::Update)
    );
    let commands = replay.step(Millis(2_010), leg_interrupted());
    assert!(
        commands.iter().any(|command| matches!(
            command,
            Command::Link {
                link: LinkId(2),
                command: LinkCommand::Open { .. },
            }
        )),
        "{commands:?}"
    );
    replay.step(Millis(2_500), Step::opened(2));
    let commands = replay.step(
        Millis(2_600),
        Step::update_facts(2, UpdateBoardState::NeedsEngine),
    );
    assert_eq!(update_legs(&commands), 1, "{commands:?}");
}

/// A core-only board sends no hello — only its manifest on channel 3. That
/// alone brings the next leg, but only when it is NEWER than the leg that
/// ended: the manifest it sent during the leg is not the board coming back.
#[test]
fn a_core_only_board_coming_back_with_only_its_manifest_resumes_the_next_leg() {
    let mut replay = ready_board();
    replay.step(Millis(1_000), update(UpdateIntentFacts::Auto));
    replay.step(
        Millis(1_500),
        Step::update_facts(1, UpdateBoardState::Updating),
    );
    // The leg ends with the link still open: the board has not spoken since.
    let commands = replay.step(Millis(2_000), leg_interrupted());
    assert_eq!(update_legs(&commands), 0, "an older manifest: {commands:?}");
    replay.advance_to(Millis(5_000));
    assert_eq!(update_legs_so_far(&replay), 1, "still waiting");
    assert!(
        replay.commands().iter().any(|(_, command)| matches!(
            command,
            Command::Link {
                command: LinkCommand::SendFrame(_),
                ..
            }
        )),
        "an open, quiet link is asked for a hello"
    );

    let commands = replay.step(
        Millis(5_100),
        Step::update_facts(1, UpdateBoardState::NeedsEngine),
    );
    assert_eq!(update_legs(&commands), 1, "{commands:?}");
    assert!(
        replay
            .roster()
            .devices()
            .first()
            .and_then(|device| device.evidence.hello_heard_at())
            .is_some_and(|heard_at| heard_at < Millis(2_000)),
        "no hello since the leg ended was needed"
    );
}

/// The controller's no-click `Auto` spawn on a board that is already
/// updating is a no-op: no second activity, no error line, and the running
/// update keeps its intent.
#[test]
fn an_auto_spawn_while_an_update_runs_is_ignored() {
    let mut replay = ready_board();
    replay.step(Millis(1_000), update(install()));
    replay.step(Millis(1_100), stage(UpdateStageFacts::Updating, 400, 1_000));
    let starts = count(&replay, "ActivityStarted");

    let commands = replay.step(Millis(1_200), update(UpdateIntentFacts::Auto));
    assert!(commands.is_empty(), "{commands:?}");
    let card = card_of(&replay);
    assert_eq!(count(&replay, "ActivityStarted"), starts);
    assert_eq!(
        card.activity
            .as_ref()
            .and_then(|a| a.update.as_ref())
            .map(|update| update.intent.clone()),
        Some(install())
    );
    assert!(card.last_outcome.is_none(), "no error line");
}

/// One activity at a time (I5): an update refuses to start under another
/// activity, and nothing else starts under an update.
#[test]
fn an_update_and_any_other_activity_refuse_each_other() {
    let mut replay = ready_board();
    replay.step(
        Millis(1_000),
        Step::Flash {
            device: 1,
            board: "seeed-xiao-esp32c6".to_string(),
            build: "esp32c6-4mb".to_string(),
            name: None,
        },
    );
    let commands = replay.step(Millis(1_100), update(install()));
    assert_eq!(update_legs(&commands), 0);
    assert_eq!(
        card_of(&replay).activity.as_ref().map(|a| a.kind),
        Some(ActivityKind::Flash)
    );

    let mut replay = ready_board();
    replay.step(Millis(1_000), update(install()));
    for step in [
        Step::Push { device: 1 },
        Step::Erase { device: 1 },
        Step::Identify { device: 1 },
        Step::RemoveProject { device: 1 },
    ] {
        replay.step(Millis(1_100), step);
        assert_eq!(
            card_of(&replay).activity.as_ref().map(|a| a.kind),
            Some(ActivityKind::Update)
        );
    }
    let commands = replay.feed(
        Millis(1_200),
        Input::Action(Action::ResetBoard {
            device: lpa_devices::DeviceId(1),
        }),
    );
    assert!(
        commands.is_empty(),
        "no reset under an update: {commands:?}"
    );
}

// ---------------------------------------------------------------- helpers

/// A running board on a link that carries channel 3, its hello announcing
/// it, identified and idle.
fn ready_board() -> Replay {
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(Millis(0), Step::attach_with_update_channel(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(
        Millis(200),
        Step::hello(1)
            .uid("dev_2f8a")
            .board("seeed-xiao-esp32c6")
            .with_update(UpdateBoardState::Running),
    );
    assert_eq!(replay.roster().devices().len(), 1, "setup: one device");
    assert!(card_of(&replay).activity.is_none(), "setup: idle");
    replay
}

fn install() -> UpdateIntentFacts {
    UpdateIntentFacts::Install {
        version: "2026.10.05-2".to_string(),
        allow_downgrade: false,
    }
}

fn update(intent: UpdateIntentFacts) -> Step {
    Step::UpdateFirmware { device: 1, intent }
}

fn stage(stage: UpdateStageFacts, done: u32, total: u32) -> Step {
    Step::UpdateStage {
        device: 1,
        stage,
        done,
        total,
        effect: None,
    }
}

fn outcome(outcome: UpdateOutcomeFacts) -> Step {
    Step::UpdateOutcome {
        device: 1,
        outcome,
        effect: None,
    }
}

fn leg_interrupted() -> Step {
    Step::LegInterrupted {
        device: 1,
        reason: Some("the board reset".to_string()),
        effect: None,
    }
}

fn leg_ended_ok() -> Step {
    Step::EffectEnded {
        device: 1,
        ok: true,
        message: Some("leg done".to_string()),
        effect: None,
        kind: Some(ActivityKind::Update),
    }
}

fn card_of(replay: &Replay) -> DeviceView {
    replay.view().devices.into_iter().next().expect("a card")
}

fn cancellable(replay: &Replay) -> bool {
    card_of(replay)
        .activity
        .as_ref()
        .is_some_and(|activity| activity.cancellable)
}

fn update_legs(commands: &[Command]) -> usize {
    commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                Command::RunEffect {
                    effect: EffectRequest::Update { .. },
                    ..
                }
            )
        })
        .count()
}

fn update_legs_so_far(replay: &Replay) -> usize {
    let commands: Vec<Command> = replay
        .commands()
        .iter()
        .map(|(_, command)| command.clone())
        .collect();
    update_legs(&commands)
}

fn opens(commands: &[Command]) -> bool {
    commands.iter().any(|command| {
        matches!(
            command,
            Command::Link {
                command: LinkCommand::Open { .. },
                ..
            }
        )
    })
}

fn count(replay: &Replay, needle: &str) -> usize {
    replay
        .journal_notes()
        .iter()
        .filter(|note| note.contains(needle))
        .count()
}
