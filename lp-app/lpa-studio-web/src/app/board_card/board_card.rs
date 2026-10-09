//! [`BoardCard`]: one board, drawn from core's [`UiBoardCard`] — the
//! picture with the status corner cut out of it, the name bar and its one
//! primary, then the five bars, always project · connection · access ·
//! firmware · hardware.
//!
//! **One height in every state** (AC5, `docs/style/ui.md` "Stable
//! Layout"): the card is an explicit grid — the picture's row (138 px, or
//! 108 px when the card is narrow), the 50 px name bar, five 28 px bars —
//! so a board's events never move it. Work, notices and details live inside
//! those rows or float above them.
//!
//! The card leases its board's picture while it is mounted (the feed pulls
//! frames only for a card on screen) — plumbing, not a verb: it is no offer
//! and builds none. A new board has no picture to lease.

use dioxus::prelude::*;
use lpa_studio_core::{
    BarLayer, DeviceFeedOp, OfferPath, UiAction, UiBoardCard, UiBoardPresence, UiExampleCard,
    UiPackageCard,
};

use super::board_picture::BoardPicture;
use super::card_action::{CardScope, use_provide_card_scope};
use super::name_bar::NameBar;
use super::stack_bar::StackBar;
use super::status_corner::StatusCorner;

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
) -> Element {
    let device = card.device;
    let leases = card.presence != UiBoardPresence::New;
    // The mount lease (today's card's): a card on screen wants its board's
    // picture; a card leaving the page stops the pull.
    use_effect(move || {
        if leases {
            on_action.call(DeviceFeedOp::action_for(device, true));
        }
    });
    use_drop(move || {
        if leases {
            on_action.call(DeviceFeedOp::action_for(device, false));
        }
    });
    use_provide_card_scope(CardScope {
        projects,
        examples,
        board_title: card.name_bar.title.clone(),
        armed: armed_preview,
    });
    let board = card.board.clone();
    rsx! {
        article { class: CARD_CLASS, "data-board-card": "{card.board}",
            div { class: PICTURE_SLOT_CLASS,
                BoardPicture { picture: card.picture.clone() }
                StatusCorner {
                    corner: card.status.clone(),
                    board: board.clone(),
                    initially_open: details_open == Some(CardPart::Corner),
                    on_action,
                }
            }
            NameBar { bar: card.name_bar.clone(), on_action }
            for bar in card.bars {
                StackBar {
                    key: "{bar.layer.as_str()}",
                    initially_open: details_open == Some(CardPart::Bar(bar.layer)),
                    bar,
                    board: board.clone(),
                    on_action,
                }
            }
        }
    }
}

/// The card: an explicit grid — the picture's row, the 50 px name bar, five
/// 28 px bars — in one border, on the panel ground. `ux-board-card` makes
/// it the query container the picture's narrow height reads (style.css).
/// It clips nothing: the primary's glow reaches past its section.
const CARD_CLASS: &str = "ux-board-card tw:relative tw:grid tw:min-w-0 tw:grid-rows-[auto_50px_repeat(5,28px)] tw:rounded-[8px] tw:border tw:border-solid tw:border-border tw:bg-panel-primary tw:text-[11.5px] tw:text-muted-foreground";

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
    /// (138 px, 108 px narrow), the 50 px name bar and five 28 px bars.
    #[test]
    fn every_bar_the_name_bar_and_the_picture_have_fixed_heights() {
        assert!(
            CARD_CLASS.contains("tw:grid-rows-[auto_50px_repeat(5,28px)]"),
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

    /// Nothing on the card clips: the primary's hover glow is a bloom past
    /// its section (the picture's own row clips its lamps, nothing else).
    #[test]
    fn the_card_clips_nothing_but_its_picture() {
        assert!(!CARD_CLASS.contains("overflow"), "{CARD_CLASS}");
        assert!(PICTURE_SLOT_CLASS.contains("tw:overflow-hidden"));
    }

    /// The walk hooks are on the DOM: the card's board, the corner's mark,
    /// the five bars in their order, and the offer path on every action the
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
                lpa_studio_core::UiPrimary::Unavailable { .. } => None,
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
