//! [`BoardCard`]: one board, drawn from core's [`UiBoardCard`] — the
//! picture with the surface over it ([`BoardSurface`]: what matters now,
//! when anything does) and the status corner cut out of it, the name bar
//! and its one primary, then the eight rows, always project · USB · Wi‑Fi ·
//! Bluetooth · lightplayer.app · access · firmware · hardware — for a new
//! board, its setup list, the rows with something left to do lit.
//! Connected, the board's whole panel stands in the rows' place
//! ([`CompactPanel`]: `card.panel`), Edit at its foot.
//!
//! **One height in every state but one** (AC5, `docs/style/ui.md` "Stable
//! Layout"): the card is an explicit grid — the picture's row (138 px, or
//! 108 px when the card is narrow), the 50 px name bar, eight 28 px rows —
//! so a board's events never move it. Work, notices and details live inside
//! those rows or float above them. **Connected, the card grows once**, to
//! hold the panel (its last row is as tall as the panel), and shrinks back
//! on Done (the board card ADR's 2026-10-19 amendment).
//!
//! The card leases its board's picture while it is mounted (the feed pulls
//! frames only for a card on screen) — plumbing, not a verb: it is no offer
//! and builds none. A new board has no picture to lease.

use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_core::{
    BarLayer, DeviceFeedOp, DeviceId, OfferPath, SurfacePicture, UiAction, UiBoardCard,
    UiBoardPresence, UiExampleCard, UiPackageCard,
};

use super::board_picture::BoardPicture;
use super::board_surface::BoardSurface;
use super::card_action::{CardPreviews, CardScope, use_provide_card_scope};
use super::name_bar::NameBar;
use super::stack_bar::StackBar;
use super::status_corner::StatusCorner;
use crate::app::module::CompactPanel;

/// A part of the card whose details a story mounts open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardPart {
    /// One bar's details.
    Bar(BarLayer),
    /// The status corner's details.
    Corner,
}

/// One board's card. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoardCard(
    card: UiBoardCard,
    /// The page's library projects: the project pick reads them.
    #[props(default)]
    projects: Vec<UiPackageCard>,
    /// The page's examples: the project pick reads them.
    #[props(default)]
    examples: Vec<UiExampleCard>,
    on_action: EventHandler<UiAction>,
    /// Stories only: mount this part's details open.
    #[props(default)]
    details_open: Option<CardPart>,
    /// Stories only: the action at this path renders already armed.
    #[props(default)]
    armed_preview: Option<OfferPath>,
    /// Stories only: panels in a state a capture cannot click to.
    #[props(default)]
    previews: CardPreviews,
) -> Element {
    let device = card.device;
    let leases = card.presence != UiBoardPresence::New;
    // The mount lease (today's card's): a card on screen wants its board's
    // picture; a card leaving the page stops the pull. It follows the card:
    // a new board's card is the same card once the board says who it is
    // (adoption keeps its handle), so the lease starts then, not only at
    // mount — a card mounted New would otherwise never ask for a picture.
    let leased = use_hook(|| Rc::new(Cell::new(None::<DeviceId>)));
    let leased_now = leased.clone();
    use_effect(use_reactive!(|leases, device| {
        let (release, lease) = lease_change(leased_now.get(), leases.then_some(device));
        if let Some(old) = release {
            on_action.call(DeviceFeedOp::action_for(old, false));
        }
        if let Some(new) = lease {
            on_action.call(DeviceFeedOp::action_for(new, true));
        }
        leased_now.set(leases.then_some(device));
    }));
    use_drop(move || {
        if let Some(old) = leased.get() {
            on_action.call(DeviceFeedOp::action_for(old, false));
        }
    });
    use_provide_card_scope(CardScope {
        projects,
        examples,
        board_title: card.name_bar.title.clone(),
        device: Some(device),
        armed: armed_preview,
        previews,
    });
    let board = card.board.clone();
    let class = match card.panel.is_some() {
        true => CONNECTED_CARD_CLASS,
        false => CARD_CLASS,
    };
    rsx! {
        article { class, "data-board-card": "{card.board}",
            div { class: PICTURE_SLOT_CLASS,
                BoardPicture {
                    picture: card.picture.clone(),
                    covered: card.surface.picture == SurfacePicture::Dimmed,
                }
                BoardSurface { surface: card.surface.clone() }
                StatusCorner {
                    corner: card.status.clone(),
                    board: board.clone(),
                    initially_open: details_open == Some(CardPart::Corner),
                    on_action,
                }
            }
            NameBar { bar: card.name_bar.clone(), on_action }
            match card.panel {
                Some(panel) => rsx! {
                    CompactPanel { panel, on_action }
                },
                None => rsx! {
                    for bar in card.bars {
                        StackBar {
                            key: "{bar.layer.as_str()}",
                            initially_open: details_open == Some(CardPart::Bar(bar.layer)),
                            bar,
                            board: board.clone(),
                            on_action,
                        }
                    }
                },
            }
        }
    }
}

/// How the picture lease moves when the card's board, or whether it leases,
/// changes: the board to release, and the board to lease. Nothing moves
/// while it stays the same.
fn lease_change(
    had: Option<DeviceId>,
    want: Option<DeviceId>,
) -> (Option<DeviceId>, Option<DeviceId>) {
    match had == want {
        true => (None, None),
        false => (had, want),
    }
}

/// The card: an explicit grid — the picture's row, the 50 px name bar,
/// eight 28 px rows — in one border, on the panel ground. `ux-board-card` makes
/// it the query container the picture's narrow height reads (style.css).
/// It clips nothing: the primary's glow reaches past its section.
const CARD_CLASS: &str = "ux-board-card tw:relative tw:grid tw:min-w-0 tw:grid-rows-[auto_50px_repeat(8,28px)] tw:rounded-[8px] tw:border tw:border-solid tw:border-border tw:bg-panel-primary tw:text-[11.5px] tw:text-muted-foreground";

/// The connected card: the same grid, its last row the board's panel at the
/// panel's own height — the card grows once, on connect.
const CONNECTED_CARD_CLASS: &str = "ux-board-card tw:relative tw:grid tw:min-w-0 tw:grid-rows-[auto_50px_auto] tw:rounded-[8px] tw:border tw:border-solid tw:border-border tw:bg-panel-primary tw:text-[11.5px] tw:text-muted-foreground";

/// The picture's row: its height is `.ux-board-picture-slot`'s (138 px,
/// 108 px narrow — style.css); the corner is cut out of its top right.
const PICTURE_SLOT_CLASS: &str =
    "ux-board-picture-slot tw:relative tw:overflow-hidden tw:rounded-t-[7px] tw:bg-[#07080a]";

#[cfg(test)]
mod tests {
    use super::super::card_test_fixtures::{
        attribute_values, card_and_tree, porch_view, render_card,
    };
    use super::*;

    /// Every bar, the name bar and the picture have fixed heights, so the
    /// card is one height in every state: an explicit grid of a picture row
    /// (138 px, 108 px narrow), the 50 px name bar and eight 28 px rows.
    #[test]
    fn every_bar_the_name_bar_and_the_picture_have_fixed_heights() {
        assert!(
            CARD_CLASS.contains("tw:grid-rows-[auto_50px_repeat(8,28px)]"),
            "{CARD_CLASS}"
        );
        assert!(css_rule(".ux-board-picture-slot").contains("height: 138px"));
        let css = include_str!("../../style.css");
        assert!(
            css.contains("@container board-card (max-width: 239px)"),
            "the narrow card's picture rule"
        );
        assert!(css_rule(".ux-board-card").contains("container: board-card / inline-size"));
    }

    /// The picture lease follows the card: a new board's card asks for
    /// nothing, the same card asks once its board says who it is, a card
    /// keeping its board asks nothing again, and one handed another board
    /// releases the first.
    #[test]
    fn the_picture_lease_follows_the_cards_presence() {
        let board = Some(DeviceId(7));
        // Mounted New: nothing leased.
        assert_eq!(lease_change(None, None), (None, None));
        // The board said who it is: lease it.
        assert_eq!(lease_change(None, board), (None, board));
        // Re-rendered, the same board: nothing moves.
        assert_eq!(lease_change(board, board), (None, None));
        // Another board in the same card: release, then lease.
        assert_eq!(
            lease_change(board, Some(DeviceId(8))),
            (board, Some(DeviceId(8)))
        );
    }

    /// Nothing on the card clips: the primary's hover glow is a bloom past
    /// its section (the picture's own row clips its lamps, nothing else).
    #[test]
    fn the_card_clips_nothing_but_its_picture() {
        assert!(!CARD_CLASS.contains("overflow"), "{CARD_CLASS}");
        assert!(
            !CONNECTED_CARD_CLASS.contains("overflow"),
            "{CONNECTED_CARD_CLASS}"
        );
        assert!(PICTURE_SLOT_CLASS.contains("tw:overflow-hidden"));
    }

    /// The walk hooks are on the DOM: the card's board, the corner's mark,
    /// the eight rows in their order, and the offer path on every action the
    /// face draws — the primary and each bar's action.
    #[test]
    fn the_walk_hooks_are_on_the_dom() {
        let (card, tree) = card_and_tree(&porch_view());
        let primary = card
            .name_bar
            .primary
            .iter()
            .filter_map(|primary| match primary {
                lpa_studio_core::UiPrimary::Offer(action) => Some(action.offer.to_string()),
                lpa_studio_core::UiPrimary::Working { .. } => None,
            });
        let bar_actions = card
            .bars
            .iter()
            .filter_map(|bar| bar.action.as_ref().map(|action| action.offer.to_string()));
        let face: Vec<String> = primary.chain(bar_actions).collect();
        assert!(!face.is_empty(), "the porch board offers its Edit");
        let html = render_card(card.clone(), tree, None);
        assert_eq!(
            attribute_values(&html, "data-board-card"),
            vec![card.board.to_string()]
        );
        assert_eq!(attribute_values(&html, "data-board-corner"), vec!["fine"]);
        assert_eq!(
            attribute_values(&html, "data-bar"),
            BarLayer::ALL.map(|layer| layer.as_str().to_string())
        );
        assert!(attribute_values(&html, "data-bar-work").is_empty());
        let marked = attribute_values(&html, "data-offer-path");
        for path in face {
            assert!(
                marked.contains(&path),
                "{path} is drawn unmarked: {marked:?}"
            );
        }
    }

    /// Connected (`panel: Some`), the card draws the board's panel in the
    /// bars' place and no bar, its last row as tall as the panel (the card
    /// grows once); watched (`None`), the eight rows at their fixed height.
    #[test]
    fn a_connected_card_draws_its_panel_in_the_bars_place() {
        let (watched, tree) = card_and_tree(&porch_view());
        assert_eq!(watched.panel, None);
        let html = render_card(watched.clone(), tree.clone(), None);
        assert_eq!(attribute_values(&html, "data-bar").len(), 8);
        assert!(!html.contains("data-board-panel"), "{html}");
        assert!(html.contains(CARD_CLASS), "{html}");

        let connected = UiBoardCard {
            panel: Some(lpa_studio_core::UiBoardPanel {
                target: None,
                sections: Vec::new(),
                auto_save: None,
                edit: None,
            }),
            ..watched
        };
        let html = render_card(connected, tree, None);
        assert!(html.contains("data-board-panel"), "{html}");
        assert!(!html.contains("data-all-controls"), "{html}");
        assert!(attribute_values(&html, "data-bar").is_empty(), "{html}");
        assert!(html.contains(CONNECTED_CARD_CLASS), "{html}");
        assert!(
            CONNECTED_CARD_CLASS.contains("tw:grid-rows-[auto_50px_auto]"),
            "{CONNECTED_CARD_CLASS}"
        );
    }

    /// A bar at work draws its fill in a clipped track that is the bar's own
    /// child, a sibling of the details' trigger: the fill cannot leave the
    /// bar (the 2026-10-10 defect), and nothing that clips wraps the details
    /// that float over it. Only the bar doing the work has a track.
    #[test]
    fn a_bar_at_work_draws_its_fill_in_a_clipped_track() {
        use lpa_studio_core::{DeviceActivityKind, DeviceActivityView, DeviceStatus, DeviceView};
        let view = DeviceView {
            usb: None,
            status: DeviceStatus::Busy,
            state_label: "Identifying".to_string(),
            activity: Some(DeviceActivityView {
                kind: DeviceActivityKind::Identify,
                label: "Identifying…".to_string(),
                percent: None,
                cancellable: true,
                cancel_requested: false,
                layout: None,
                replace: None,
                flash_step: None,
                update: None,
            }),
            ..porch_view()
        };
        let (card, tree) = card_and_tree(&view);
        // The surface says "Identifying this board.": the row is carried,
        // its spinner and no track (P2).
        let html = render_card(card.clone(), tree.clone(), None);
        assert_eq!(attribute_values(&html, "data-bar-work"), vec!["running"]);
        assert!(
            attribute_values(&html, "data-work-track").is_empty(),
            "{html}"
        );
        // A row's own work (one the surface does not say) draws the track.
        let mut card = card;
        for bar in &mut card.bars {
            if let Some(work) = &mut bar.work {
                work.carried = false;
            }
        }
        let html = render_card(card, tree, None);
        assert_eq!(
            attribute_values(&html, "data-work-track").len(),
            1,
            "{html}"
        );
        // Identifying is the Hardware row's work: the track sits in that
        // row, after its trigger and action, with the fill as its only
        // child and nothing around it that clips.
        let bar_at = html
            .find("data-bar=\"hardware\"")
            .expect("the hardware row");
        let track_at = html.find("data-work-track").expect("a track");
        assert!(track_at > bar_at, "the track is the hardware row's");
        let track = &html[html[..track_at].rfind("<div").expect("its tag")..];
        let tag = &track[..track.find('>').expect("the track's tag closes")];
        assert!(tag.contains("tw:overflow-hidden"), "{tag}");
        assert!(tag.contains("tw:inset-x-0"), "{tag}");
        let bar_tag = &html[html[..bar_at].rfind("<div").expect("its tag")..bar_at];
        assert!(
            !bar_tag.contains("overflow"),
            "the bar does not clip: {bar_tag}"
        );
        assert!(!CARD_CLASS.contains("overflow"));
    }

    /// Every verb in an open details card is drawn from its offer and
    /// marked with its path — the danger zone's among them.
    #[test]
    fn every_details_verb_is_marked_with_its_offer_path() {
        let (card, tree) = card_and_tree(&porch_view());
        let mut danger_verbs = 0;
        for layer in BarLayer::ALL {
            let sections = &card.bar(layer).details.sections;
            danger_verbs += sections
                .iter()
                .filter(|section| section.weight == lpa_studio_core::RichWeight::Danger)
                .map(|section| section.affordances.len())
                .sum::<usize>();
            let html = render_card(card.clone(), tree.clone(), Some(CardPart::Bar(layer)));
            let marked = attribute_values(&html, "data-offer-path");
            for action in sections
                .iter()
                .flat_map(|section| section.affordances.iter())
            {
                let path = action.offer.to_string();
                assert!(
                    marked.contains(&path),
                    "{layer:?}: {path} unmarked in {marked:?}"
                );
            }
        }
        assert!(danger_verbs > 0, "a danger verb was among them");
    }

    /// The rule the card's height rests on (style.css), read whole.
    fn css_rule(selector: &str) -> String {
        let css = include_str!("../../style.css");
        let at = css
            .find(&format!("{selector} {{"))
            .unwrap_or_else(|| panic!("style.css has no `{selector}` rule"));
        let body = &css[at..];
        body[..body.find('}').expect("the rule closes") + 1].to_string()
    }
}
