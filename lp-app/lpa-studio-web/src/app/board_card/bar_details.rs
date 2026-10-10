//! [`BarDetails`]: the details card of a bar, or of the status corner —
//! Studio's detail card, merged with the bar that opened it
//! (`docs/style/ui.md` "The board card").
//!
//! It is the shared [`DetailPopover`] in its custom-trigger mode, the way
//! the Connections group's rows used it: the trigger is the bar's own
//! full-width button, and its top-layer copy keeps the button's layout. Not
//! the anchored mode (the `anchored-popover-open-render-drift` debt). A bar
//! is wider than the 320 px card, so the 2026-10-06 overlap fix in the
//! shared popover is what keeps a tall details card readable over its own
//! bar (`docs/defects/2026-10-06-a-card-row-shows-through-its-own-popover.md`).
//!
//! Inside, core's sections render in the order core gave them
//! ([`RichDetailSection`]), then the panels ([`BarDetailPanel`]), then the
//! Danger section, which always comes last. While core raises the details
//! (`raised`: a question that must be answered now, Q40) they open by
//! themselves and stay open until core lowers them — or, for a refusal,
//! until its Close closes it, which the details remember until core's panel
//! changes. Opening details that hold the Wi‑Fi panel asks the board for
//! its networks again, as opening today's Wi‑Fi popover did.
//!
//! One popover at a time: a pick in the details (a project pick, a board
//! pick) closes them and opens its picker over the same trigger, the bar's
//! own line ([`DetailsPick`], [`AnchoredPick`]) — never a popover inside
//! the details card.

use dioxus::prelude::*;
use lpa_studio_core::{
    NetworkCommand, OfferPath, RichSection, RichWeight, UiAction, UiCardAction, UiDetailPanel,
    UiLayoutPanel,
};

use super::bar_detail_panel::BarDetailPanel;
use super::card_action::{AnchoredPick, DetailsPick};
use crate::app::home::access_ui_context::network_handler;
use crate::base::{DetailPopover, PopoverPlacement, StudioIconName};
use crate::core::RichDetailSection;

/// One details card. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BarDetails(
    sections: Vec<RichSection<UiCardAction>>,
    panels: Vec<UiDetailPanel>,
    /// Core asks for these details now: open them, and keep them open.
    #[props(default)]
    raised: bool,
    /// `devices/<board ref>`: the board the panels act on.
    board: OfferPath,
    /// The trigger's accessible name ("Firmware details").
    label: String,
    /// The trigger's hover title (the whole line its row may cut).
    title: String,
    trigger: Element,
    trigger_class: String,
    trigger_open_class: String,
    #[props(default = PopoverPlacement::BottomStart)] placement: PopoverPlacement,
    /// Stories: mount open.
    #[props(default)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    // A refusal its Close closed stays closed until core's panel changes
    // (today's card's `closed_sheet`): `raised` does not reopen it.
    let mut closed_layout = use_signal(|| None::<UiLayoutPanel>);
    let layout = panels.iter().find_map(|panel| match panel {
        UiDetailPanel::Layout(layout) => Some(layout.clone()),
        _ => None,
    });
    let raised = raised && (layout.is_none() || *closed_layout.read() != layout);
    let mut open = use_signal(|| initially_open || raised);
    // Whether the details are open because core raised them, so lowering
    // them closes only what core opened.
    let mut opened_by_core = use_signal(|| raised);
    use_effect(use_reactive!(|raised| {
        if raised {
            opened_by_core.set(true);
            // Kept open while raised: a click outside does not dismiss a
            // question that must be answered.
            if !open() {
                open.set(true);
            }
        } else if *opened_by_core.peek() {
            opened_by_core.set(false);
            open.set(false);
        }
    }));
    // Opening details that hold the Wi‑Fi panel asks the board again, and
    // what it hears (core asks nothing of a board that cannot scan) — as
    // opening today's Wi‑Fi popover does.
    let wifi = panels.iter().find_map(|panel| match panel {
        UiDetailPanel::Wifi(wifi) => Some(wifi.device),
        _ => None,
    });
    let on_network = network_handler();
    use_effect(use_reactive!(|wifi| {
        if let Some(device) = wifi.filter(|_| open()) {
            on_network.call(NetworkCommand::Refresh { device });
            on_network.call(NetworkCommand::Scan { device });
        }
    }));
    let on_close_layout = use_callback(move |refusal: UiLayoutPanel| {
        closed_layout.set(Some(refusal));
    });
    // A pick pressed inside closes the details and comes back here, to be
    // drawn over the same trigger.
    let pick = use_signal(|| None::<UiCardAction>);
    use_context_provider(|| DetailsPick {
        details_open: open,
        pick,
    });
    let pick_trigger = trigger.clone();
    let pick_class = trigger_open_class.clone();
    let (danger, rest): (Vec<_>, Vec<_>) = sections
        .into_iter()
        .partition(|section| section.weight == RichWeight::Danger);
    rsx! {
        DetailPopover {
            icon: StudioIconName::Info,
            label,
            title,
            placement,
            initially_open: initially_open || raised,
            open_signal: Some(open),
            layer_keeps_layout: true,
            trigger: Some(trigger),
            trigger_class,
            trigger_open_class,
            div { class: DETAILS_BODY_CLASS,
                // Keyed by place: two sections may share a title (a bar's
                // notice and its facts are both "Firmware").
                for (index, section) in rest.into_iter().enumerate() {
                    RichDetailSection { key: "section-{index}", section, on_action }
                }
                for (index, panel) in panels.into_iter().enumerate() {
                    BarDetailPanel {
                        key: "panel-{index}",
                        panel,
                        board: board.clone(),
                        on_close_layout,
                        on_action,
                    }
                }
                for (index, section) in danger.into_iter().enumerate() {
                    RichDetailSection { key: "danger-{index}", section, on_action }
                }
            }
        }
        if let Some(action) = pick() {
            div { class: PICK_OVER_TRIGGER_CLASS,
                AnchoredPick {
                    action,
                    trigger: pick_trigger,
                    trigger_class: pick_class,
                    pick,
                    on_action,
                }
            }
        }
    }
}

/// A handed-back pick's slot: laid over the details' trigger, the same box,
/// its popover wrapper stretched to it, so the picker opens where the
/// details did.
const PICK_OVER_TRIGGER_CLASS: &str = "tw:absolute tw:inset-0 tw:grid tw:[&>span]:h-full tw:[&>span]:w-full tw:[&>span]:place-items-stretch";

/// The details card's body: one grid of sections, scrolling inside itself
/// past the viewport's budget so a long card (a Wi‑Fi list, a terminal)
/// never runs off the page.
const DETAILS_BODY_CLASS: &str = "tw:grid tw:max-h-[min(560px,calc(100vh-48px))] tw:min-w-0 tw:overflow-y-auto tw:overscroll-contain";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{BarLayer, UiActionDraw};

    use super::super::CardPart;
    use super::super::card_test_fixtures::{
        attribute_values, card_and_tree, porch_view, render_card,
    };
    use super::*;

    /// The body scrolls inside itself rather than growing past the page,
    /// and draws no frame of its own (the popover is the one box).
    #[test]
    fn the_body_scrolls_and_draws_no_frame() {
        assert!(DETAILS_BODY_CLASS.contains("tw:overflow-y-auto"));
        assert!(DETAILS_BODY_CLASS.contains("tw:max-h-"));
        assert!(!DETAILS_BODY_CLASS.contains("border"));
        assert!(!DETAILS_BODY_CLASS.contains("rounded"));
    }

    /// One popover at a time: a pick in a bar's details (the project
    /// bar's "Put another project on it") is a row that hands its picker
    /// back to the bar, not a popover trigger inside the details card.
    #[test]
    fn a_pick_in_the_details_is_a_row_not_a_second_popover() {
        let (card, tree) = card_and_tree(&porch_view());
        let picks: Vec<String> = card
            .bar(BarLayer::Project)
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter())
            .filter(|action| {
                matches!(
                    action.draw,
                    UiActionDraw::ProjectPick { .. } | UiActionDraw::BoardPick { .. }
                )
            })
            .map(|action| action.word.clone())
            .collect();
        assert!(!picks.is_empty(), "the project details hold a pick");
        let closed = render_card(card.clone(), tree.clone(), None);
        let open = render_card(card, tree, Some(CardPart::Bar(BarLayer::Project)));
        assert_eq!(
            attribute_values(&open, "aria-expanded").len(),
            attribute_values(&closed, "aria-expanded").len(),
            "the open details add no popover trigger"
        );
        let handed_back = attribute_values(&open, "aria-haspopup");
        assert_eq!(handed_back.len(), picks.len(), "{handed_back:?}");
        for word in picks {
            assert!(open.contains(&word), "{word} is drawn: {open}");
        }
    }

    /// A handed-back pick opens over the same box as the details' trigger.
    #[test]
    fn a_handed_back_pick_lies_over_the_trigger() {
        assert!(PICK_OVER_TRIGGER_CLASS.contains("tw:absolute tw:inset-0"));
        assert!(PICK_OVER_TRIGGER_CLASS.contains("tw:[&>span]:h-full"));
    }
}
