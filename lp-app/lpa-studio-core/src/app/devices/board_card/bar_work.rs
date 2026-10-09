//! Work in progress, in the bar doing it (D34): which bar an activity
//! belongs to, what its line says, and how the bar reads for a few seconds
//! after work that went well (Done) or until the next try after work that
//! failed (Failed, with Retry).
//!
//! An activity narrates in the bar whose subject it changes, so the bar that
//! moves says what kind of work it is without a word: a push or a removal
//! changes what the board plays (Project); a flash, an erase or an update
//! changes what is under it (Firmware); identifying, and a Wi‑Fi or relay
//! connect, is the board being reached (Connection).

use lpa_devices::ActivityKind;
use lpa_devices::view::ActivityView;

use super::board_card_input::BoardCardInput;
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_stack_bar::BarLayer;
use crate::app::devices::device_identity::device_chip;
use crate::app::devices::ui_link_kind::UiLinkKind;

/// The bar an activity of `kind` narrates in.
pub fn activity_bar(kind: ActivityKind) -> BarLayer {
    match kind {
        ActivityKind::Push | ActivityKind::RemoveProject => BarLayer::Project,
        ActivityKind::Flash | ActivityKind::Erase | ActivityKind::Update => BarLayer::Firmware,
        ActivityKind::Identify => BarLayer::Connection,
    }
}

/// An activity as one line: its label, the cancel it has been asked for,
/// and its percentage when it has one.
///
/// A requested cancel is a state, not the absence of one: the activity is
/// winding down and will be evicted if it does not. A flash's cancel can
/// hold for the rest of the write window — esptool cannot stop mid-image.
pub fn activity_words(activity: &ActivityView) -> String {
    let mut text = match activity.cancel_requested {
        true => format!(
            "{} — cancelling (finishing the current write)",
            activity.label
        ),
        false => activity.label.clone(),
    };
    if let Some(percent) = activity.percent {
        text.push_str(&format!(" · {}%", u32::from(percent).min(100)));
    }
    text
}

/// The work `layer` shows on this board: the running activity when it is
/// this bar's, a Wi‑Fi or relay connect under way on the connection bar,
/// else how this bar's last work ended.
pub(crate) fn bar_work(input: &BoardCardInput<'_>, layer: BarLayer) -> Option<UiBarWork> {
    if let Some(activity) = &input.view.activity
        && activity_bar(activity.kind) == layer
    {
        return Some(UiBarWork {
            words: activity_words(activity),
            percent: activity.percent,
            state: BarWorkState::Running,
            cancel: input
                .offer("cancel")
                .filter(|_| activity.cancellable)
                .map(|offer| offer.path.clone()),
            other_device: false,
        });
    }
    if layer == BarLayer::Connection
        && let Some(connect) = input.wifi_connect.filter(|connect| connect.connecting)
    {
        let link = match connect.through_relay {
            true => UiLinkKind::Relay,
            false => UiLinkKind::Wifi,
        };
        return Some(UiBarWork {
            words: format!("Connecting over {}…", link.label()),
            percent: None,
            state: BarWorkState::Running,
            cancel: None,
            other_device: false,
        });
    }
    ended_work(input, layer)
}

/// How this bar's last work ended: Done for a few seconds after it went
/// well, Failed while the model keeps its failed outcome (until the next
/// activity clears it).
///
/// A failed identify is not striped: how it ended is the board's own state
/// now (not responding, needs firmware), and the connection bar's row for
/// that state says it, with Retry.
fn ended_work(input: &BoardCardInput<'_>, layer: BarLayer) -> Option<UiBarWork> {
    let end = input.ended.filter(|end| activity_bar(end.kind) == layer)?;
    let outcome = input.view.last_outcome.as_ref();
    if end.shows_done(input.now) {
        return Some(UiBarWork {
            words: outcome
                .filter(|outcome| outcome.ok)
                .map_or_else(|| "Done".to_string(), |outcome| outcome.summary.clone()),
            percent: None,
            state: BarWorkState::Done,
            cancel: None,
            other_device: false,
        });
    }
    let failed = outcome
        .filter(|outcome| !outcome.ok && !end.ok)
        .filter(|_| end.kind != ActivityKind::Identify)?;
    input.idle().then(|| UiBarWork {
        words: failed.summary.clone(),
        percent: None,
        state: BarWorkState::Failed {
            retry: retry(input, end.kind),
        },
        cancel: None,
        other_device: false,
    })
}

/// Retry for a failed `kind`: the bar's own verb, when it is offered again —
/// the project picker for a push, the update for an update, the board pick
/// for a flash. Nothing else retries from the bar.
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
/// else the catalog family of the hello's board id (today's `joined_chip`).
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
    use lpa_devices::view::OutcomeView;

    /// An activity narrates in ONE bar — the one whose subject it changes.
    #[test]
    fn each_activity_belongs_to_the_zone_whose_subject_it_changes() {
        assert_eq!(activity_bar(ActivityKind::Push), BarLayer::Project);
        assert_eq!(activity_bar(ActivityKind::RemoveProject), BarLayer::Project);
        assert_eq!(activity_bar(ActivityKind::Flash), BarLayer::Firmware);
        assert_eq!(activity_bar(ActivityKind::Erase), BarLayer::Firmware);
        assert_eq!(activity_bar(ActivityKind::Update), BarLayer::Firmware);
        assert_eq!(activity_bar(ActivityKind::Identify), BarLayer::Connection);
    }

    /// The activity's own reading: label, the cancel it was asked for, and
    /// the percentage — all on the one line the bar allows.
    #[test]
    fn an_activity_reads_as_one_line_with_its_percentage() {
        let flashing = activity(ActivityKind::Flash, "Flashing firmware", Some(42));
        assert_eq!(activity_words(&flashing), "Flashing firmware · 42%");
        let cancelling = ActivityView {
            cancel_requested: true,
            percent: None,
            ..flashing
        };
        assert_eq!(
            activity_words(&cancelling),
            "Flashing firmware — cancelling (finishing the current write)"
        );
    }

    #[test]
    fn running_work_is_its_bars_alone_with_the_cancel_offer() {
        let mut fixture = CardFixture::ready().with_activity(activity(
            ActivityKind::Push,
            "Sending the project",
            Some(40),
        ));
        let input = fixture.input();
        let work = bar_work(&input, BarLayer::Project).expect("the push is project work");
        assert_eq!(work.words, "Sending the project · 40%");
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
        assert_eq!(work.words, "Project loaded");
        assert_eq!(bar_work(&fixture.input(), BarLayer::Firmware), None);

        fixture.now += 3.0;
        assert_eq!(
            bar_work(&fixture.input(), BarLayer::Project),
            None,
            "the green goes after three seconds"
        );
    }

    #[test]
    fn a_failure_stays_striped_until_superseded_with_its_bars_retry() {
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
        assert_eq!(work.words, "The board did not answer");
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
    fn a_wifi_connect_under_way_is_connection_work() {
        let mut fixture = CardFixture::offline();
        fixture.wifi_connect = Some(crate::UiWifiConnect {
            host: "10.0.0.5".to_string(),
            through_relay: false,
            connecting: true,
            error: None,
            busy: false,
        });
        let work = bar_work(&fixture.input(), BarLayer::Connection).expect("connecting");
        assert_eq!(work.words, "Connecting over Wi\u{2011}Fi…");
        fixture.wifi_connect.as_mut().unwrap().through_relay = true;
        assert_eq!(
            bar_work(&fixture.input(), BarLayer::Connection)
                .unwrap()
                .words,
            "Connecting over Wi\u{2011}Fi via lightplayer.app…"
        );
    }
}
