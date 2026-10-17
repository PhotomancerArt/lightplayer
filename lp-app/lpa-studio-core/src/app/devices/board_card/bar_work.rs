//! Work in progress, in the row doing it (D34): which row an activity
//! belongs to, what its state says, and how the row reads for a few seconds
//! after work that went well (Done) or until the next try after work that
//! failed (Failed, with Retry, the reason in its details).
//!
//! An activity narrates in the row whose subject it changes, so the row
//! that moves says what kind of work it is: a push or a removal changes
//! what the board plays (Project); a flash, an erase or an update changes
//! what runs it (Firmware); identifying finds out what the board is
//! (Hardware); a Wi‑Fi or relay connect is the board being reached (Link).
//!
//! The words are the row's state, in label voice, with one number when it
//! has one (`docs/ux/anatomy/board-card.md`): "sending · 40%", "updating ·
//! 42%" (one number for the whole update, never "1 of 2"), "backing up ·
//! 12%", "installing · 30%", "identifying".

use lpa_devices::ActivityKind;
use lpa_devices::view::ActivityView;

use super::board_card_input::BoardCardInput;
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_stack_bar::BarLayer;
use crate::app::devices::device_identity::device_chip;
use crate::app::devices::device_update_standing::UpdateStanding;
use crate::app::devices::device_update_words::UpdateRowKind;
use crate::app::devices::ui_link_kind::UiLinkKind;

/// The row an activity of `kind` narrates in.
pub fn activity_bar(kind: ActivityKind) -> BarLayer {
    match kind {
        ActivityKind::Push | ActivityKind::RemoveProject => BarLayer::Project,
        ActivityKind::Flash | ActivityKind::Erase | ActivityKind::Update => BarLayer::Firmware,
        ActivityKind::Identify => BarLayer::Hardware,
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
    }
}

/// An update under way, in its row's words and one number for the whole
/// update: the new firmware is the first half and its last part the
/// second, so "updating · 42%" never restarts at "2 of 2". `None` for a
/// standing that is not an update running.
pub(crate) fn update_work_words(standing: &UpdateStanding) -> Option<(String, Option<u8>)> {
    let half = |percent: Option<u8>, second: bool| {
        percent.map(|percent| (u32::from(percent.min(100)) / 2 + u32::from(second) * 50) as u8)
    };
    let (verb, percent) = match standing {
        UpdateStanding::BackingUp { percent, .. } => ("backing up", *percent),
        UpdateStanding::Updating { percent, .. } => ("updating", half(*percent, false)),
        UpdateStanding::Finishing {
            percent, resumed, ..
        } => match resumed {
            true => ("resuming", half(*percent, true)),
            false => ("updating", half(*percent, true)),
        },
        UpdateStanding::Restoring { percent, .. } => ("restoring", *percent),
        UpdateStanding::AnotherDevice { percent, .. } => ("updating", *percent),
        _ => return None,
    };
    Some((state_words(verb, percent), percent))
}

/// The work `layer` shows on this board: the running activity when it is
/// this row's, a Wi‑Fi or relay connect under way on the Link row, else how
/// this row's last work ended.
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
            .and_then(|update| update_work_words(&update.standing));
        let (words, percent) = match story {
            Some((words, percent)) if !activity.cancel_requested => {
                (words, percent.or(activity.percent))
            }
            _ => (activity_words(activity), activity.percent),
        };
        return Some(UiBarWork {
            cancel: input
                .offer("cancel")
                .filter(|_| activity.cancellable)
                .map(|offer| offer.path.clone()),
            ..UiBarWork::running(words, percent)
        });
    }
    if layer == BarLayer::Connection
        && let Some(connect) = input.wifi_connect.filter(|connect| connect.connecting)
    {
        let link = match connect.through_relay {
            true => UiLinkKind::Relay,
            false => UiLinkKind::Wifi,
        };
        return Some(UiBarWork::running(
            format!("{} · connecting", link.label()),
            None,
        ));
    }
    ended_work(input, layer)
}

/// How this row's last work ended: Done for a few seconds after it went
/// well, Failed while the model keeps its failed outcome (until the next
/// activity clears it), the outcome's words in its details.
///
/// A failed identify is not striped: how it ended is the board's own state
/// now (not responding, needs firmware), and the Link row's state for that
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
    let failed = outcome
        .filter(|outcome| !outcome.ok && !end.ok)
        .filter(|_| end.kind != ActivityKind::Identify)?;
    input.idle().then(|| {
        UiBarWork::failed(
            failed_words(end.kind),
            Some(failed.summary.clone()),
            retry(input, end.kind),
        )
    })
}

/// Retry for a failed `kind`: the row's own verb, when it is offered again —
/// the project picker for a push, the update for an update, the board pick
/// for a flash. Nothing else retries from the row.
fn retry(input: &BoardCardInput<'_>, kind: ActivityKind) -> Option<UiCardAction> {
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
        ActivityKind::Flash => retry(input.offer("flash")?, board_pick(input)),
        ActivityKind::Update => {
            let update = input.offer("update-firmware")?;
            let draw = match update.params().is_empty() {
                true => UiActionDraw::Press,
                false => board_pick(input),
            };
            retry(update, draw)
        }
        ActivityKind::Erase | ActivityKind::RemoveProject | ActivityKind::Identify => None,
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
        assert_eq!(
            update_work_words(&updating),
            Some(("updating · 42%".to_string(), Some(42)))
        );
        let finishing = UpdateStanding::Finishing {
            board: x.clone(),
            to: y.clone(),
            percent: Some(40),
            running: true,
            resumed: false,
        };
        assert_eq!(
            update_work_words(&finishing),
            Some(("updating · 70%".to_string(), Some(70)))
        );
        let backing_up = UpdateStanding::BackingUp {
            board: x.clone(),
            to: y,
            percent: Some(12),
        };
        assert_eq!(
            update_work_words(&backing_up),
            Some(("backing up · 12%".to_string(), Some(12)))
        );
        assert_eq!(
            update_work_words(&UpdateStanding::UpToDate { version: x }),
            None
        );
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
        assert_eq!(bar_work(&input, BarLayer::Connection), None);
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

    #[test]
    fn a_wifi_connect_under_way_is_link_work_naming_the_link() {
        let mut fixture = CardFixture::offline();
        fixture.wifi_connect = Some(crate::UiWifiConnect {
            host: "10.0.0.5".to_string(),
            through_relay: false,
            connecting: true,
            error: None,
            busy: false,
        });
        let work = bar_work(&fixture.input(), BarLayer::Connection).expect("connecting");
        assert_eq!(work.words, "Wi\u{2011}Fi · connecting");
        fixture.wifi_connect.as_mut().unwrap().through_relay = true;
        assert_eq!(
            bar_work(&fixture.input(), BarLayer::Connection)
                .unwrap()
                .words,
            "lightplayer.app · connecting"
        );
    }
}
