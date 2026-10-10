//! Build a board's card: [`board_card`] for a board on the roster,
//! [`pending_board_card`] for a link still saying who it is.
//!
//! Pure functions of their input. The card points only at verbs the board's
//! own offers publish (a debug assertion, and a test): a button the card
//! drew that the tree did not offer would be one the app agent cannot see
//! and a press that could only fail.

use lpa_devices::view::PendingLinkView;

use super::access_bar::{access_bar, pending_access_bar};
use super::board_card_input::BoardCardInput;
use super::board_picture::board_picture;
use super::connection_bar::{connection_bar, pending_connection_bar};
use super::firmware_bar::{firmware_bar, pending_firmware_bar};
use super::hardware_bar::{hardware_bar, pending_hardware_bar};
use super::primary_action::{pending_primary, primary_action};
use super::project_bar::{edit_action, pending_project_bar, project_bar};
use super::status_corner::status_corner;
use super::ui_board_card::{UiBoardCard, UiBoardPresence};
use super::ui_board_panel::UiBoardPanel;
use super::ui_board_picture::{PictureSource, UiBoardPicture};
use super::ui_detail_panel::UiDetailPanel;
use super::ui_name_bar::UiNameBar;
use super::ui_status_corner::{CornerMark, UiCornerDetails, UiStatusCorner};
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::{OfferPath, RichLine, RichSection, RichWeight, UiOffer, UiStatusKind};

/// A board's card, built from `input`.
pub fn board_card(input: &BoardCardInput<'_>) -> UiBoardCard {
    let bars = vec![
        project_bar(input),
        connection_bar(input),
        access_bar(input),
        firmware_bar(input),
        hardware_bar(input),
    ];
    let running_work = bars
        .iter()
        .filter_map(|bar| bar.work.as_ref())
        .find(|work| work.state == super::ui_bar_work::BarWorkState::Running);
    let card = UiBoardCard {
        board: input.board.clone(),
        device: input.view.id,
        // A board another tab holds is plugged in and running: Online,
        // whatever it is here (`split_roster` says the same).
        presence: match input.offline() && input.view.held_elsewhere.is_none() {
            true => UiBoardPresence::Offline,
            false => UiBoardPresence::Online,
        },
        picture: board_picture(input),
        status: status_corner(input, &bars),
        name_bar: UiNameBar {
            title: input.view.title.clone(),
            place: None,
            primary: primary_action(input, running_work),
        },
        bars,
        panel: connected_panel(input),
    };
    debug_assert_offered(&card, input.offers);
    card
}

/// The board's panel on its card: the controller's picks, while the session
/// is connected here and its project is ready (CD7), with Edit (or its lock)
/// at the end of the All controls row — the project bar's own action, built
/// by the same function (CD15) — and auto-save only at the edit tier (Q12).
fn connected_panel(input: &BoardCardInput<'_>) -> Option<UiBoardPanel> {
    let picks = input.panel.filter(|_| input.connection.is_connected())?;
    Some(UiBoardPanel {
        auto_save: picks.auto_save.filter(|_| input.can_edit()),
        edit: edit_action(input),
        ..picks.clone()
    })
}

/// A new board's card: a link the roster is still identifying. `link` is
/// the kind of the link it arrived on.
pub fn pending_board_card(
    pending: &PendingLinkView,
    board: &OfferPath,
    offers: &[UiOffer],
    link: UiLinkKind,
) -> UiBoardCard {
    let state = match pending.detail.as_deref() {
        Some(detail) => format!("{} · {detail}", pending.state_label),
        None => pending.state_label.clone(),
    };
    let card = UiBoardCard {
        board: board.clone(),
        device: pending.device,
        presence: UiBoardPresence::New,
        picture: UiBoardPicture {
            source: PictureSource::None,
            frame: None,
            dim: false,
            light: None,
        },
        status: UiStatusCorner {
            mark: CornerMark::Blank,
            reading: None,
            details: UiCornerDetails {
                sections: vec![RichSection {
                    title: "Running".to_string(),
                    tone: UiStatusKind::Neutral,
                    sentence: None,
                    lines: vec![RichLine::new("State", state)],
                    chip: None,
                    affordances: Vec::new(),
                    weight: RichWeight::Advisory,
                }],
                // A link that has said nothing yet has nothing in its
                // terminal; the panel stands so the corner reads the same.
                panels: vec![UiDetailPanel::Terminal {
                    lines: Vec::new(),
                    dropped: 0,
                }],
            },
        },
        name_bar: UiNameBar {
            title: pending.title.clone(),
            place: None,
            primary: Some(pending_primary(pending, board, offers, link)),
        },
        bars: vec![
            pending_project_bar(),
            pending_connection_bar(pending, link),
            pending_access_bar(),
            pending_firmware_bar(pending),
            pending_hardware_bar(pending, board, offers),
        ],
        panel: None,
    };
    debug_assert_offered(&card, offers);
    card
}

/// Every offer path the card points at must be one `offers` publishes.
fn debug_assert_offered(card: &UiBoardCard, offers: &[UiOffer]) {
    if cfg!(debug_assertions) {
        for path in card.offer_paths() {
            debug_assert!(
                offers.iter().any(|offer| &offer.path == path),
                "the card points at `{path}`, which is not offered: {:?}",
                offers
                    .iter()
                    .map(|offer| offer.path.to_string())
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::view::OutcomeView;
    use lpa_devices::{ActivityKind, HoldLevel};

    use super::super::board_connection::BoardConnection;
    use super::super::card_fixtures::{CardFixture, activity, board, feed};
    use super::super::primary_action::tests::pending_view;
    use super::super::ui_stack_bar::BarLayer;
    use super::*;
    use crate::app::devices::activity_ends::ActivityEnd;
    use crate::app::devices::device_card_feed_view::FeedLiveness;

    #[test]
    fn a_card_has_its_five_bars_in_order() {
        let card = board_card(&CardFixture::ready().input());
        let layers: Vec<BarLayer> = card.bars.iter().map(|bar| bar.layer).collect();
        assert_eq!(layers, BarLayer::ALL);
        assert_eq!(card.name_bar.title, "Porch");
        assert_eq!(card.name_bar.place, None);
        assert_eq!(card.presence, UiBoardPresence::Online);
        assert_eq!(card.board.to_string(), "devices/mac-a0f26287b48c");
    }

    /// The card can only point at verbs core offered: in every state a test
    /// can reach, each path it names is published.
    #[test]
    fn every_card_action_names_a_published_offer() {
        let mut failed = CardFixture::ready();
        failed.view.last_outcome = Some(OutcomeView {
            summary: "The board did not answer".to_string(),
            ok: false,
        });
        failed.ended = Some(ActivityEnd {
            kind: ActivityKind::Push,
            ok: false,
            at: failed.now - 5.0,
        });
        let mut offline = CardFixture::offline();
        offline.wifi_address = true;
        for mut fixture in [
            CardFixture::ready(),
            CardFixture::ready().with_activity(activity(
                ActivityKind::Push,
                "Sending the project",
                Some(10),
            )),
            CardFixture::ready()
                .over(crate::UiLinkKind::Bluetooth)
                .locked(),
            offline,
            failed,
        ] {
            let offers = fixture.offers();
            let card = board_card(&fixture.input());
            assert!(!card.offer_paths().is_empty(), "{card:?}");
            for path in card.offer_paths() {
                assert!(
                    offers.iter().any(|offer| &offer.path == path),
                    "{path} is not offered"
                );
            }
        }
    }

    /// CD7 and CD15: connected with its project ready, the card carries the
    /// panel (the bars are still built), its primary is Done, and Edit sits
    /// at the end of the All controls row — the project bar's own action.
    /// Auto-save rides along at the edit tier.
    #[test]
    fn a_connected_card_carries_the_panel_with_edit_at_its_end() {
        let mut fixture = connected(CardFixture::ready());
        let card = board_card(&fixture.input());
        let panel = card.panel.clone().expect("the panel");
        assert_eq!(panel.controls.len(), 1);
        assert_eq!(panel.more, 2);
        assert_eq!(panel.auto_save, Some(true), "USB is the edit tier");
        let edit = panel.edit.clone().expect("Edit on the All controls row");
        assert_eq!(edit.word, "Edit");
        assert_eq!(edit.icon.as_deref(), Some("edit"));
        assert_eq!(
            Some(&edit),
            card.bar(BarLayer::Project).action.as_ref(),
            "one function builds both"
        );
        assert_eq!(card.bars.len(), 5, "the bars are still built");
        assert_eq!(
            card.name_bar.primary.as_ref().map(|primary| primary.word()),
            Some("Done")
        );
        assert!(card.offer_paths().contains(&&edit.offer));

        // Not connected (opening, reconnecting, watched): no panel.
        for connection in [
            BoardConnection::Watched,
            BoardConnection::Connecting,
            BoardConnection::Reconnecting,
        ] {
            fixture.connection = connection.clone();
            assert_eq!(board_card(&fixture.input()).panel, None, "{connection:?}");
        }
        // Connected, but the project not ready (the controller has no
        // picks yet): no panel.
        let mut opening = connected(CardFixture::ready());
        opening.panel = None;
        assert_eq!(board_card(&opening.input()).panel, None);
    }

    /// AC7: a play-only link's panel hides auto-save, and its Edit wears
    /// the lock (`unlock`, the password sheet).
    #[test]
    fn a_play_only_connected_card_hides_auto_save_and_locks_edit() {
        let mut fixture = connected(CardFixture::ready().over(crate::UiLinkKind::Bluetooth));
        fixture.access = Some(crate::UiDeviceAccess {
            unlock: Some(crate::UiUnlockOffer::PlayOnly),
            grant: Some(crate::UiAccessGrant {
                tier: lpc_access::Tier::Play,
                key: Some("friends".to_string()),
            }),
            ..crate::UiDeviceAccess::default()
        });
        let panel = board_card(&fixture.input()).panel.expect("the panel");
        assert_eq!(panel.auto_save, None, "below the edit tier");
        let edit = panel.edit.expect("Edit, locked");
        assert_eq!(edit.icon.as_deref(), Some("lock"));
        assert!(edit.offer.to_string().ends_with("/unlock"));
        assert_eq!(edit.draw, crate::UiActionDraw::Sheet);
    }

    #[test]
    fn an_offline_board_is_an_offline_card() {
        let card = board_card(&CardFixture::offline().input());
        assert_eq!(card.presence, UiBoardPresence::Offline);
        assert_eq!(card.status.mark, CornerMark::Quiet);
    }

    /// A board another tab holds is an Online card (A3: it is plugged in
    /// and running), with the picture that tab saved, dimmed and aged, and
    /// Connect as its primary.
    #[test]
    fn a_board_another_tab_holds_is_an_online_card_with_its_saved_picture() {
        for status in [DeviceStatus::Attached, DeviceStatus::Offline] {
            let mut fixture = CardFixture::held(HoldLevel::Open);
            fixture.view.status = status;
            let mut saved = feed(FeedLiveness::Offline, true);
            saved.frame_age_secs = Some(300.0);
            fixture.feed = Some(saved);
            let card = board_card(&fixture.input());

            assert_eq!(card.presence, UiBoardPresence::Online, "{status:?}");
            assert_eq!(card.picture.source, PictureSource::Saved);
            assert!(card.picture.dim);
            assert!(card.picture.frame.is_some());
            assert_eq!(card.status.reading.as_deref(), Some("5 min ago"));
            assert_eq!(
                card.status.mark,
                CornerMark::Notice(UiStatusKind::Attention),
                "someone has it: orange"
            );
            let running = card.status.details.sections.last().expect("running");
            let lines: Vec<(&str, &str)> = running
                .lines
                .iter()
                .map(|line| (line.label.as_str(), line.value.as_str()))
                .collect();
            assert_eq!(
                lines,
                [
                    ("State", "Open in another tab"),
                    ("Picture", "The last picture another tab saved.")
                ]
            );
            assert_eq!(
                card.name_bar.primary.as_ref().map(|primary| primary.word()),
                Some("Connect")
            );
            assert_eq!(
                card.bar(BarLayer::Connection).summary,
                "Open in another tab"
            );
        }
    }

    /// A held board nobody has saved a picture for says so, instead of
    /// "Not connected".
    #[test]
    fn a_held_board_with_no_picture_says_who_has_it() {
        let mut fixture = CardFixture::held(HoldLevel::Watching);
        let card = board_card(&fixture.input());
        assert_eq!(card.picture.source, PictureSource::None);
        let running = card.status.details.sections.last().expect("running");
        assert!(running.lines.iter().any(|line| line.label == "Picture"
            && line.value == "Another tab has this board. It has saved no picture yet."));
    }

    #[test]
    fn a_new_board_is_a_blank_new_card_saying_its_state() {
        let card = pending_board_card(&pending_view(), &board(), &[], UiLinkKind::Bluetooth);
        assert_eq!(card.presence, UiBoardPresence::New);
        assert_eq!(card.status.mark, CornerMark::Blank);
        assert_eq!(card.name_bar.title, "New board");
        assert_eq!(card.bars.len(), 5);
        assert!(matches!(
            card.status.details.panels.as_slice(),
            [UiDetailPanel::Terminal { lines, dropped: 0 }] if lines.is_empty()
        ));
        assert_eq!(
            card.name_bar.primary.as_ref().map(|primary| primary.word()),
            Some("Connect")
        );
        assert_eq!(card.panel, None);
    }

    /// `fixture`, connected on its card with its project's picks: one
    /// master fader, and two controls the card does not draw.
    fn connected(mut fixture: CardFixture) -> CardFixture {
        fixture.editor_holds_it = true;
        fixture.connection = BoardConnection::Connected;
        fixture.feed = Some(super::super::card_fixtures::lens_feed());
        let control = |channel: &str, widget| {
            crate::UiPanelControlView::new(
                channel,
                crate::UiPanelControl {
                    label: channel.to_string(),
                    address: None,
                    widget,
                    value: crate::UiSlotValue::f32(0.5),
                    emit: crate::UiPanelEmit::Value,
                    live_value: None,
                    live_gradient: None,
                    panel_target: None,
                    unit: None,
                    state: crate::UiSlotFieldState::editable(),
                    aspects: Vec::new(),
                    wires: Vec::new(),
                },
            )
        };
        let fader = || crate::UiPanelWidget::Fader {
            min: 0.0,
            max: 1.0,
            step: None,
        };
        let root = crate::UiPanelGroup::new("Porch", "/").with_controls(vec![
            control(super::super::MASTER_CHANNEL, fader()),
            control("level", fader()),
            control("palette", crate::UiPanelWidget::PaletteSwatch),
        ]);
        fixture.panel = Some(super::super::board_panel_picks(&root, Some(true)));
        fixture
    }
}
