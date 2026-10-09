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
//! themselves and stay open until core lowers them.

use dioxus::prelude::*;
use lpa_studio_core::{OfferPath, RichSection, RichWeight, UiAction, UiCardAction, UiDetailPanel};

use super::bar_detail_panel::BarDetailPanel;
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
                    BarDetailPanel { key: "panel-{index}", panel, board: board.clone(), on_action }
                }
                for (index, section) in danger.into_iter().enumerate() {
                    RichDetailSection { key: "danger-{index}", section, on_action }
                }
            }
        }
    }
}

/// The details card's body: one grid of sections, scrolling inside itself
/// past the viewport's budget so a long card (a Wi‑Fi list, a terminal)
/// never runs off the page.
const DETAILS_BODY_CLASS: &str = "tw:grid tw:max-h-[min(560px,calc(100vh-48px))] tw:min-w-0 tw:overflow-y-auto tw:overscroll-contain";

#[cfg(test)]
mod tests {
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
}
