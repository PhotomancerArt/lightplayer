//! Build a board's card: [`board_card`] for a board on the roster,
//! [`pending_board_card`] for a link still saying who it is.
//!
//! Pure functions of their input. The card points only at verbs the board's
//! own offers publish (a debug assertion, and a test): a button the card
//! drew that the tree did not offer would be one the app agent cannot see
//! and a press that could only fail.

use lpa_devices::view::PendingLinkView;

use super::bar_work::bar_work;
use super::board_card_input::BoardCardInput;
use super::board_picture::board_picture;
use super::primary_action::{pending_primary, primary_action};
use super::status_corner::status_corner;
use super::ui_board_card::{UiBoardCard, UiBoardPresence};
use super::ui_board_picture::{PictureSource, UiBoardPicture};
use super::ui_detail_panel::UiDetailPanel;
use super::ui_name_bar::UiNameBar;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use super::ui_status_corner::{CornerMark, UiCornerDetails, UiStatusCorner};
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::{OfferPath, RichLine, RichSection, RichWeight, UiOffer, UiStatusKind};

/// A board's card, built from `input`.
pub fn board_card(input: &BoardCardInput<'_>) -> UiBoardCard {
    let bars: Vec<UiStackBar> = BarLayer::ALL
        .into_iter()
        .map(|layer| placeholder_bar(input, layer))
        .collect();
    let running_work = bars
        .iter()
        .filter_map(|bar| bar.work.as_ref())
        .find(|work| work.state == super::ui_bar_work::BarWorkState::Running);
    let card = UiBoardCard {
        board: input.board.clone(),
        device: input.view.id,
        presence: match input.offline() {
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
    };
    debug_assert_offered(&card, input.offers);
    card
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
        bars: BarLayer::ALL
            .into_iter()
            .map(|layer| empty_bar(layer, "Not known yet"))
            .collect(),
    };
    debug_assert_offered(&card, offers);
    card
}

/// A bar with its layer, its icon, the placeholder words the bar builders
/// replace, and its work.
fn placeholder_bar(input: &BoardCardInput<'_>, layer: BarLayer) -> UiStackBar {
    UiStackBar {
        work: bar_work(input, layer),
        ..empty_bar(layer, layer.as_str())
    }
}

/// A bar saying `summary` and nothing else.
fn empty_bar(layer: BarLayer, summary: &str) -> UiStackBar {
    UiStackBar {
        layer,
        icon: layer_icon(layer).to_string(),
        summary: summary.to_string(),
        aside: None,
        aside_icon: None,
        tone: UiStatusKind::Neutral,
        action: None,
        work: None,
        details: UiBarDetails::default(),
    }
}

/// Each bar's icon token.
fn layer_icon(layer: BarLayer) -> &'static str {
    match layer {
        BarLayer::Project => "project",
        BarLayer::Connection => "usb",
        BarLayer::Access => "lock",
        BarLayer::Firmware => "firmware",
        BarLayer::Hardware => "chip",
    }
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
    use lpa_devices::ActivityKind;
    use lpa_devices::view::OutcomeView;

    use super::super::card_fixtures::{CardFixture, activity, board};
    use super::super::primary_action::tests::pending_view;
    use super::*;
    use crate::app::devices::activity_ends::ActivityEnd;

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

    #[test]
    fn an_offline_board_is_an_offline_card() {
        let card = board_card(&CardFixture::offline().input());
        assert_eq!(card.presence, UiBoardPresence::Offline);
        assert_eq!(card.status.mark, CornerMark::Quiet);
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
    }
}
