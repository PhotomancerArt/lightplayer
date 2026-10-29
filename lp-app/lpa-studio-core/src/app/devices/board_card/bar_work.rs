//! Work in progress, in the row doing it (D34): which row an activity
//! belongs to, what its state says, and how the row reads for a few seconds
//! after work that went well (Done) or until the next try after work that
//! failed (Failed, with Retry, the reason in its details).
//!
//! An activity narrates in the row whose subject it changes, so the row
//! that moves says what kind of work it is: a push or a removal changes
//! what the board plays (Project); a flash, an erase or an update changes
//! what runs it (Firmware); identifying finds out what the board is
//! (Hardware); a connect is the board being reached, on the row of the link
//! it takes ([`super::link_session`]).
//!
//! The words are the row's state, in label voice, with one number when it
//! has one (`docs/ux/anatomy/board-card.md`): "sending · 40%", "updating ·
//! 42%" (one number for the whole update, never "1 of 2"), "backing up ·
//! 12%", "installing · 30%", "identifying".

use lpa_devices::view::ActivityView;
use lpa_devices::{ActivityKind, UpdateStageFacts};

use super::board_card_input::BoardCardInput;
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_stack_bar::BarLayer;
use crate::app::devices::device_flash_offer::SET_BOARD;
use crate::app::devices::device_identity::device_chip;
use crate::app::devices::device_update_standing::UpdateStanding;
use crate::app::devices::device_update_words::UpdateRowKind;

/// The row an activity of `kind` narrates in.
pub fn activity_bar(kind: ActivityKind) -> BarLayer {
    match kind {
        ActivityKind::Push | ActivityKind::RemoveProject => BarLayer::Project,
        ActivityKind::Flash | ActivityKind::Erase | ActivityKind::Update => BarLayer::Firmware,
        ActivityKind::Identify | ActivityKind::SetBoard => BarLayer::Hardware,
    }
}

/// An activity as its row's state: its verb, and its percentage when it
/// has one ("sending · 40%", "installing"); "cancelling" once a cancel was
/// asked for (a flash's can hold for the rest of the write window — esptool
/// cannot stop mid-image).
pub fn activity_words(activity: &ActivityView) -> String {
    if activity.cancel_requested {
        return "cancelling".to_string();
    }
    state_words(activity_verb(activity.kind), activity.percent)
}

/// `verb`, and `percent` when known: "sending · 40%".
pub(crate) fn state_words(verb: &str, percent: Option<u8>) -> String {
    match percent {
        Some(percent) => format!("{verb} · {}", crate::percent(u32::from(percent))),
        None => verb.to_string(),
    }
}

/// The verb a row says while `kind` runs.
fn activity_verb(kind: ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Push => "sending",
        ActivityKind::RemoveProject => "removing",
        ActivityKind::Flash => "installing",
        ActivityKind::Erase => "erasing",
        ActivityKind::Update => "updating",
        ActivityKind::Identify => "identifying",
        ActivityKind::SetBoard => "saving the board",
    }
}

/// How a row says `kind` ended well, for its few green seconds.
fn done_word(kind: ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Push => "sent",
        ActivityKind::RemoveProject => "removed",
        ActivityKind::Flash => "installed",
        ActivityKind::Erase => "erased",
        ActivityKind::Update => "updated",
        ActivityKind::Identify => "identified",
        ActivityKind::SetBoard => "saved",
    }
}

/// How a row says `kind` failed; the reason is the details'.
fn failed_words(kind: ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Push => "send failed",
        ActivityKind::RemoveProject => "remove failed",
        ActivityKind::Flash => "install failed",
        ActivityKind::Erase => "erase failed",
        ActivityKind::Update => "update failed",
        ActivityKind::Identify => "not identified",
        ActivityKind::SetBoard => "not saved",
    }
}

/// An update under way: its row's verb, and one number for the whole
/// update ([`UpdateStanding::whole_percent`]), so "updating · 42%" never
/// restarts at "2 of 2". `None` for a standing that is not an update
/// running.
pub(crate) fn update_work(standing: &UpdateStanding) -> Option<(&'static str, Option<u8>)> {
    let verb = match standing {
        UpdateStanding::BackingUp { .. } => "backing up",
        UpdateStanding::Finishing { resumed: true, .. } => "resuming",
        UpdateStanding::Restoring { .. } => "restoring",
        UpdateStanding::Updating { .. }
        | UpdateStanding::Finishing { .. }
        | UpdateStanding::AnotherDevice { .. } => "updating",
        UpdateStanding::UpgradingProjects { .. } => "upgrading projects",
        _ => return None,
    };
    Some((verb, standing.whole_percent()))
}

/// The one number an update says everywhere: `percent` of the current
/// `stage`, as a percent of the whole update. The new firmware's core is
/// 0–49 % and its engine (finishing) 50–100 %; a backup, a restore and
/// another device's update are one step each. The card's words
/// ([`UpdateStanding::whole_percent`]) and the terminal's "at N%" and
/// "· N%" ([`crate::app::devices::update_narration`]) say this number, so
/// one moment of one update never reads as two (a test pins them equal).
pub(crate) fn whole_update_percent(stage: UpdateStageFacts, percent: u8) -> u8 {
    let percent = percent.min(100);
    match stage {
        UpdateStageFacts::Updating => percent / 2,
        UpdateStageFacts::Finishing => percent / 2 + 50,
        UpdateStageFacts::BackingUp
        | UpdateStageFacts::Restoring
        | UpdateStageFacts::Waiting
        | UpdateStageFacts::UpgradingProjects => percent,
    }
}

/// Work under way on a row: `verb` and `percent`, the one number its words
/// say, its foot draws and the card's surface reads — so they can never
/// disagree.
pub(crate) fn running_work(verb: &str, percent: Option<u8>) -> UiBarWork {
    UiBarWork::running(state_words(verb, percent), percent)
}

/// The work `layer` shows on this board: the running activity when it is
/// this row's, else how this row's last work ended. A link row's work is
/// the session's ([`super::link_session::session_work`]).
pub(crate) fn bar_work(input: &BoardCardInput<'_>, layer: BarLayer) -> Option<UiBarWork> {
    if let Some(activity) = &input.view.activity
        && activity_bar(activity.kind) == layer
    {
        // An update says its story's words, one number for the whole of it.
        let story = input
            .update
            .filter(|update| {
                activity.kind == ActivityKind::Update && update.kind == UpdateRowKind::Progress
            })
            .and_then(|update| update_work(&update.standing));
        let verb = story.map_or(activity_verb(activity.kind), |(verb, _)| verb);
        let percent = story.and_then(|(_, percent)| percent).or(activity.percent);
        let work = match activity.cancel_requested {
            true => UiBarWork::running(activity_words(activity), percent),
            false => running_work(verb, percent),
        };
        return Some(UiBarWork {
            cancel: input
                .offer("cancel")
                .filter(|_| activity.cancellable)
                .map(|offer| offer.path.clone()),
            ..work
        });
    }
    ended_work(input, layer)
}

/// How this row's last work ended: Done for a few seconds after it went
/// well, Failed while the model keeps its failed outcome (until the next
/// activity clears it), the outcome's words in its details.
///
/// A failed identify is not striped: how it ended is the board's own state
/// now (not responding, needs firmware), and the link row's state for that
/// says it, with Retry.
fn ended_work(input: &BoardCardInput<'_>, layer: BarLayer) -> Option<UiBarWork> {
    let end = input.ended.filter(|end| activity_bar(end.kind) == layer)?;
    let outcome = input.view.last_outcome.as_ref();
    if end.shows_done(input.now) {
        return Some(UiBarWork {
            state: BarWorkState::Done,
            detail: outcome
                .filter(|outcome| outcome.ok)
                .map(|outcome| outcome.summary.clone()),
            ..UiBarWork::running(done_word(end.kind), None)
        });
    }
    let kind = failed_kind(input).filter(|kind| *kind == end.kind)?;
    Some(UiBarWork::failed(
        failed_words(kind),
        outcome.map(|outcome| outcome.summary.clone()),
        retry(input, kind),
    ))
}

/// The kind of work that failed and still shows so: the model keeps its
/// failed outcome until the next activity clears it, past the few green
/// seconds a success gets. Never an identify (how it ended is the board's
/// own state now), and only on an idle board.
pub(crate) fn failed_kind(input: &BoardCardInput<'_>) -> Option<ActivityKind> {
    let end = input.ended?;
    if end.shows_done(input.now) || end.ok || end.kind == ActivityKind::Identify {
        return None;
    }
    let failed = input
        .view
        .last_outcome
        .as_ref()
        .is_some_and(|outcome| !outcome.ok);
    (failed && input.idle()).then_some(end.kind)
}

/// Retry for a failed `kind`: the row's own verb, when it is offered again —
/// the project picker for a push, the update for an update, the board pick
/// for a flash, Factory reset for an erase (it still arms: it is Lasting),
/// and the removal for a removal. An identify does not retry from the row:
/// the board's own state says what comes next.
pub(crate) fn retry(input: &BoardCardInput<'_>, kind: ActivityKind) -> Option<UiCardAction> {
    let retry = |offer, draw| {
        Some(
            UiCardAction::press(offer, "Retry")
                .with_icon("retry")
                .drawn(draw),
        )
    };
    match kind {
        ActivityKind::Push => retry(
            input.offer("push")?,
            UiActionDraw::ProjectPick {
                board_id: input.view.board_id.clone(),
            },
        ),
        ActivityKind::Flash => retry(input.offer("flash")?, UiActionDraw::Press),
        ActivityKind::SetBoard => retry(input.offer(SET_BOARD)?, board_pick(input)),
        ActivityKind::Update => {
            let update = input.offer("update-firmware")?;
            retry(update, UiActionDraw::Press)
        }
        ActivityKind::Erase => retry(input.offer("erase")?, UiActionDraw::Press),
        ActivityKind::RemoveProject => retry(input.offer("remove-project")?, UiActionDraw::Press),
        ActivityKind::Identify => None,
    }
}

/// The board pick's chip filter for this board: the boot banner's chip,
/// else the catalog family of the hello's board id (once the web's `joined_chip`).
pub(crate) fn board_pick(input: &BoardCardInput<'_>) -> UiActionDraw {
    let from_banner = input.view.detected_chip.is_some();
    UiActionDraw::BoardPick {
        chip: device_chip(input.view),
        chip_from_banner: from_banner,
    }
}

#[cfg(test)]
mod tests {
    use super::super::card_fixtures::{CardFixture, activity};
    use super::*;
    use crate::app::devices::activity_ends::ActivityEnd;
    use crate::app::devices::device_update_version::UpdateVersion;
    use lpa_devices::view::OutcomeView;

    /// An activity narrates in ONE row — the one whose subject it changes.
    #[test]
    fn each_activity_belongs_to_the_row_whose_subject_it_changes() {
        assert_eq!(activity_bar(ActivityKind::Push), BarLayer::Project);
        assert_eq!(activity_bar(ActivityKind::RemoveProject), BarLayer::Project);
        assert_eq!(activity_bar(ActivityKind::Flash), BarLayer::Firmware);
        assert_eq!(activity_bar(ActivityKind::Erase), BarLayer::Firmware);
        assert_eq!(activity_bar(ActivityKind::Update), BarLayer::Firmware);
        assert_eq!(activity_bar(ActivityKind::Identify), BarLayer::Hardware);
    }

    /// The board pick's chip filter names its own source: the boot banner
    /// when the ROM printed one, the firmware's board id otherwise, nothing
    /// when neither knows (moved from the web's `joined_chip`).
    #[test]
    fn the_board_picks_chip_says_which_fact_answered() {
        let mut fixture = CardFixture::ready();
        fixture.view.board_id = None;
        fixture.view.detected_chip = None;
        assert_eq!(
            board_pick(&fixture.input()),
            UiActionDraw::BoardPick {
                chip: None,
                chip_from_banner: false
            }
        );

        fixture.view.board_id = Some("seeed/xiao-esp32-c6".to_string());
        assert_eq!(
            board_pick(&fixture.input()),
            UiActionDraw::BoardPick {
                chip: Some("esp32c6".to_string()),
                chip_from_banner: false
            }
        );

        fixture.view.detected_chip = Some("esp32c6".to_string());
        assert_eq!(
            board_pick(&fixture.input()),
            UiActionDraw::BoardPick {
                chip: Some("esp32c6".to_string()),
                chip_from_banner: true
            }
        );
    }

    /// An activity reads as its row's state: its verb and its number, or
    /// "cancelling" once a cancel was asked for.
    #[test]
    fn an_activity_reads_as_its_rows_state() {
        let flashing = activity(ActivityKind::Flash, "Flashing firmware", Some(30));
        assert_eq!(activity_words(&flashing), "installing · 30%");
        let pushing = activity(ActivityKind::Push, "Sending the project", Some(40));
        assert_eq!(activity_words(&pushing), "sending · 40%");
        let identifying = activity(ActivityKind::Identify, "Identifying…", None);
        assert_eq!(activity_words(&identifying), "identifying");
        let cancelling = ActivityView {
            cancel_requested: true,
            percent: None,
            ..flashing
        };
        assert_eq!(activity_words(&cancelling), "cancelling");
    }

    /// One number for the whole update: the first half is the new firmware,
    /// the second its last part — never "1 of 2".
    #[test]
    fn an_update_is_one_number() {
        let x = UpdateVersion::new("2026.10.08-9");
        let y = UpdateVersion::new("2026.10.16-1");
        let updating = UpdateStanding::Updating {
            board: x.clone(),
            to: y.clone(),
            link: crate::app::devices::device_update_route::UpdateLink::Usb,
            percent: Some(84),
        };
        assert_eq!(update_work(&updating), Some(("updating", Some(42))));
        assert_eq!(running_work("updating", Some(42)).words, "updating · 42%");
        let finishing = UpdateStanding::Finishing {
            board: x.clone(),
            to: y.clone(),
            percent: Some(40),
            running: true,
            resumed: false,
        };
        assert_eq!(update_work(&finishing), Some(("updating", Some(70))));
        let backing_up = UpdateStanding::BackingUp {
            board: x.clone(),
            to: y,
            percent: Some(12),
        };
        assert_eq!(update_work(&backing_up), Some(("backing up", Some(12))));
        assert_eq!(update_work(&UpdateStanding::UpToDate { version: x }), None);
    }

    /// The terminal's number and the card's are one: for every stage and
    /// every part-percent, [`whole_update_percent`] says what the standing
    /// the card is built from says.
    #[test]
    fn the_terminals_number_is_the_cards() {
        let x = UpdateVersion::new("2026.10.08-9");
        let y = UpdateVersion::new("2026.10.16-1");
        for percent in 0..=100u8 {
            let some = Some(percent);
            let standings = [
                (
                    UpdateStageFacts::BackingUp,
                    UpdateStanding::BackingUp {
                        board: x.clone(),
                        to: y.clone(),
                        percent: some,
                    },
                ),
                (
                    UpdateStageFacts::Updating,
                    UpdateStanding::Updating {
                        board: x.clone(),
                        to: y.clone(),
                        link: crate::app::devices::device_update_route::UpdateLink::Usb,
                        percent: some,
                    },
                ),
                (
                    UpdateStageFacts::Restoring,
                    UpdateStanding::Restoring {
                        board: x.clone(),
                        percent: some,
                        running: true,
                    },
                ),
                (
                    UpdateStageFacts::Finishing,
                    UpdateStanding::Finishing {
                        board: x.clone(),
                        to: y.clone(),
                        percent: some,
                        running: true,
                        resumed: false,
                    },
                ),
                (
                    UpdateStageFacts::Waiting,
                    UpdateStanding::AnotherDevice {
                        board: x.clone(),
                        to: Some(y.clone()),
                        percent: some,
                    },
                ),
            ];
            for (stage, standing) in standings {
                assert_eq!(
                    Some(whole_update_percent(stage, percent)),
                    standing.whole_percent(),
                    "{stage:?} at {percent}%"
                );
            }
        }
        assert_eq!(whole_update_percent(UpdateStageFacts::Updating, 99), 49);
        assert_eq!(whole_update_percent(UpdateStageFacts::Finishing, 0), 50);
    }

    #[test]
    fn running_work_is_its_rows_alone_with_the_cancel_offer() {
        let mut fixture = CardFixture::ready().with_activity(activity(
            ActivityKind::Push,
            "Sending the project",
            Some(40),
        ));
        let input = fixture.input();
        let work = bar_work(&input, BarLayer::Project).expect("the push is project work");
        assert_eq!(work.words, "sending · 40%");
        assert_eq!(work.percent, Some(40));
        assert_eq!(work.state, BarWorkState::Running);
        assert_eq!(
            work.cancel.map(|path| path.to_string()).as_deref(),
            Some("devices/mac-a0f26287b48c/cancel")
        );
        assert_eq!(bar_work(&input, BarLayer::Firmware), None);
        assert_eq!(bar_work(&input, BarLayer::Usb), None);
    }

    #[test]
    fn done_shows_for_three_seconds_then_goes() {
        let mut fixture = CardFixture::ready();
        fixture.view.last_outcome = Some(OutcomeView {
            summary: "Project loaded".to_string(),
            ok: true,
        });
        fixture.ended = Some(ActivityEnd {
            kind: ActivityKind::Push,
            ok: true,
            at: fixture.now - 1.0,
        });
        let work = bar_work(&fixture.input(), BarLayer::Project).expect("done");
        assert_eq!(work.state, BarWorkState::Done);
        assert_eq!(work.words, "sent");
        assert_eq!(work.detail.as_deref(), Some("Project loaded"));
        assert_eq!(bar_work(&fixture.input(), BarLayer::Firmware), None);

        fixture.now += 3.0;
        assert_eq!(
            bar_work(&fixture.input(), BarLayer::Project),
            None,
            "the green goes after three seconds"
        );
    }

    #[test]
    fn a_failure_stays_striped_until_superseded_with_its_rows_retry() {
        let mut fixture = CardFixture::ready();
        fixture.view.last_outcome = Some(OutcomeView {
            summary: "The board did not answer".to_string(),
            ok: false,
        });
        fixture.ended = Some(ActivityEnd {
            kind: ActivityKind::Push,
            ok: false,
            at: fixture.now - 60.0,
        });
        let work = bar_work(&fixture.input(), BarLayer::Project).expect("failed");
        assert_eq!(work.words, "send failed");
        assert_eq!(
            work.detail.as_deref(),
            Some("The board did not answer"),
            "the reason is the details'"
        );
        let BarWorkState::Failed { retry: Some(retry) } = work.state else {
            panic!("striped with Retry: {work:?}");
        };
        assert_eq!(retry.word, "Retry");
        assert_eq!(retry.offer.to_string(), "devices/mac-a0f26287b48c/push");
        assert!(matches!(retry.draw, UiActionDraw::ProjectPick { .. }));

        // The next activity clears the model's outcome: no more stripes.
        fixture.view.last_outcome = None;
        assert_eq!(bar_work(&fixture.input(), BarLayer::Project), None);
    }
}
