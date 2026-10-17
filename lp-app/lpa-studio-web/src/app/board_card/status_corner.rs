//! [`StatusCorner`]: the status corner, a 24 px notch cut out of the
//! picture's top-right corner (not laid over it): a quiet ⋯ in every state
//! (`docs/ux/anatomy/board-card.md`, "Corner"). It says nothing itself —
//! what needs you, and the editor holding the board, are the surface's to
//! say — and it keeps the card's details one press away.
//!
//! It is the trigger of the corner's own details ([`BarDetails`]): every
//! notice, how the board is running, the picture's facts, and the board's
//! terminal. `data-board-corner` names what the corner knows of the board
//! (`fine`, `quiet`, `blank`), so a walk reads the board's words from the
//! details it opens.

use dioxus::prelude::*;
use lpa_studio_core::{CornerMark, OfferPath, UiAction, UiStatusCorner};

use super::bar_details::BarDetails;
use crate::base::{PopoverPlacement, StudioIcon, StudioIconName};

/// The corner. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StatusCorner(
    corner: UiStatusCorner,
    /// `devices/<board ref>`: the board the terminal belongs to.
    board: OfferPath,
    /// Stories: mount the corner's details open.
    #[props(default)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mark = corner.mark;
    rsx! {
        div { class: "ux-board-corner", "data-board-corner": mark_name(mark),
            BarDetails {
                sections: corner.details.sections,
                panels: corner.details.panels,
                board,
                label: "Status details".to_string(),
                title: "Status".to_string(),
                trigger: rsx! {
                    span { class: "tw:inline-flex tw:text-dim-foreground", aria_hidden: "true",
                        StudioIcon { name: StudioIconName::More, size: 13 }
                    }
                },
                trigger_class: TRIGGER_CLASS.to_string(),
                trigger_open_class: TRIGGER_OPEN_CLASS.to_string(),
                placement: PopoverPlacement::BottomEnd,
                initially_open,
                on_action,
            }
        }
    }
}

/// The hook's word for what the corner knows (`data-board-corner`):
/// `fine` (watched), `quiet` (not watched) or `blank` (a new board).
pub(crate) fn mark_name(mark: CornerMark) -> &'static str {
    match mark {
        CornerMark::Fine => "fine",
        CornerMark::Quiet => "quiet",
        CornerMark::Blank => "blank",
    }
}

/// The corner's button: the notch's 24 px and its ⋯; the notch's own shape
/// is `.ux-board-corner` (style.css).
const TRIGGER_CLASS: &str = "tw:inline-flex tw:h-6 tw:min-w-6 tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:gap-[5px] tw:rounded-bl-[9px] tw:border-0 tw:bg-transparent tw:px-[9px] tw:text-muted-foreground tw:transition-colors tw:hover:text-strong-foreground ux-focus-ring";

/// …while its details are open.
const TRIGGER_OPEN_CLASS: &str = "tw:inline-flex tw:h-6 tw:min-w-6 tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:gap-[5px] tw:rounded-bl-[9px] tw:border-0 tw:bg-transparent tw:px-[9px] tw:text-strong-foreground ux-focus-ring";

#[cfg(test)]
mod tests {
    use super::*;

    /// Every mark has its hook word; the corner draws the same quiet ⋯ for
    /// each (P4: what needs you is the surface's).
    #[test]
    fn every_mark_has_its_hook_word() {
        assert_eq!(mark_name(CornerMark::Fine), "fine");
        assert_eq!(mark_name(CornerMark::Quiet), "quiet");
        assert_eq!(mark_name(CornerMark::Blank), "blank");
    }

    /// The notch is 24 px tall in both states.
    #[test]
    fn the_notch_is_24px() {
        for class in [TRIGGER_CLASS, TRIGGER_OPEN_CLASS] {
            assert!(class.contains("tw:h-6"), "{class}");
        }
    }
}
