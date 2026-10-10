//! [`StatusCorner`]: the status corner, a 24 px notch cut out of the
//! picture's top-right corner (not laid over it): the mark — a blue dot when
//! all is fine, the worst notice's status icon in its family, nothing on a
//! board Studio is not watching — then the reading ("58 fps", "5 h ago").
//!
//! It is the trigger of the corner's own details ([`BarDetails`]): every
//! notice, how the board is running, the picture's words, and the board's
//! terminal. `data-board-corner` names the mark, so a walk reads the
//! board's words from the details it opens.

use dioxus::prelude::*;
use lpa_studio_core::{CornerMark, OfferPath, UiAction, UiStatusCorner, UiStatusKind};

use super::bar_details::BarDetails;
use super::stack_bar::tone_icon;
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
    let reading = corner.reading.clone();
    // A corner with no mark and no reading (a new board) still opens its
    // details: a quiet ⋯ says where they are.
    let empty = !draws_a_mark(mark) && reading.is_none();
    let title = reading.clone().unwrap_or_else(|| "Status".to_string());
    rsx! {
        div { class: "ux-board-corner", "data-board-corner": mark_name(mark),
            BarDetails {
                sections: corner.details.sections,
                panels: corner.details.panels,
                board,
                label: "Status details".to_string(),
                title,
                trigger: rsx! {
                    CornerMarkIcon { mark }
                    if let Some(reading) = reading {
                        span { class: reading_class(mark), "{reading}" }
                    }
                    if empty {
                        span { class: "tw:inline-flex tw:text-dim-foreground", aria_hidden: "true",
                            StudioIcon { name: StudioIconName::More, size: 13 }
                        }
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

/// The corner's mark.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CornerMarkIcon(mark: CornerMark) -> Element {
    match mark {
        CornerMark::Fine => rsx! {
            span { class: FINE_DOT_CLASS, aria_hidden: "true" }
        },
        CornerMark::Notice(kind) => match tone_icon(kind) {
            Some(icon) => rsx! {
                span { class: notice_class(kind), aria_hidden: "true",
                    StudioIcon { name: icon, size: 13 }
                }
            },
            None => rsx! {},
        },
        CornerMark::Quiet | CornerMark::Blank => rsx! {},
    }
}

/// Whether `mark` draws anything.
fn draws_a_mark(mark: CornerMark) -> bool {
    match mark {
        CornerMark::Fine => true,
        CornerMark::Notice(kind) => tone_icon(kind).is_some(),
        CornerMark::Quiet | CornerMark::Blank => false,
    }
}

/// The hook's word for the mark (`data-board-corner`): `fine`, the
/// notice's family (`attention`, `warning`, `error`), `quiet` or `blank`.
pub(crate) fn mark_name(mark: CornerMark) -> &'static str {
    match mark {
        CornerMark::Fine => "fine",
        CornerMark::Notice(kind) => match kind {
            UiStatusKind::Neutral => "neutral",
            UiStatusKind::Working => "working",
            UiStatusKind::Good => "good",
            UiStatusKind::Live => "live",
            UiStatusKind::Warning => "warning",
            UiStatusKind::Attention => "attention",
            UiStatusKind::Error => "error",
        },
        CornerMark::Quiet => "quiet",
        CornerMark::Blank => "blank",
    }
}

/// A notice's icon, in its family.
fn notice_class(kind: UiStatusKind) -> &'static str {
    match kind {
        UiStatusKind::Warning => "tw:inline-flex tw:flex-none tw:text-status-warning-foreground",
        UiStatusKind::Error => "tw:inline-flex tw:flex-none tw:text-status-error-foreground",
        UiStatusKind::Live => "tw:inline-flex tw:flex-none tw:text-status-live-foreground",
        UiStatusKind::Good => "tw:inline-flex tw:flex-none tw:text-status-good-foreground",
        UiStatusKind::Working => "tw:inline-flex tw:flex-none tw:text-status-working-foreground",
        UiStatusKind::Attention | UiStatusKind::Neutral => {
            "tw:inline-flex tw:flex-none tw:text-status-attention-foreground"
        }
    }
}

/// The reading: the live blue beside the fine dot ("58 fps"), else quiet
/// (a picture's age, a frame rate beside a notice).
fn reading_class(mark: CornerMark) -> &'static str {
    match mark {
        CornerMark::Fine => {
            "tw:font-mono tw:text-[10.5px] tw:font-bold tw:leading-none tw:text-status-live-foreground"
        }
        CornerMark::Notice(_) | CornerMark::Quiet | CornerMark::Blank => {
            "tw:text-[10.5px] tw:font-semibold tw:leading-none tw:text-subtle-foreground"
        }
    }
}

/// All is fine: the blue dot, glowing softly.
const FINE_DOT_CLASS: &str = "tw:mx-0.5 tw:inline-block tw:h-2 tw:w-2 tw:flex-none tw:rounded-full tw:bg-status-live-foreground tw:shadow-[0_0_6px_rgba(156,200,238,0.6)]";

/// The corner's button: the notch's 24 px, its mark and reading; the
/// notch's own shape is `.ux-board-corner` (style.css).
const TRIGGER_CLASS: &str = "tw:inline-flex tw:h-6 tw:min-w-6 tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:gap-[5px] tw:rounded-bl-[9px] tw:border-0 tw:bg-transparent tw:px-[9px] tw:text-muted-foreground tw:transition-colors tw:hover:text-strong-foreground ux-focus-ring";

/// …while its details are open.
const TRIGGER_OPEN_CLASS: &str = "tw:inline-flex tw:h-6 tw:min-w-6 tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:gap-[5px] tw:rounded-bl-[9px] tw:border-0 tw:bg-transparent tw:px-[9px] tw:text-strong-foreground ux-focus-ring";

#[cfg(test)]
mod tests {
    use super::*;

    /// Fine is the blue dot, a notice its family's icon, and a board Studio
    /// is not watching (or a new one) shows no mark; every mark has its
    /// hook word.
    #[test]
    fn the_mark_is_the_dot_the_notice_or_nothing() {
        assert!(draws_a_mark(CornerMark::Fine));
        assert!(FINE_DOT_CLASS.contains("tw:bg-status-live-foreground"));
        for kind in [
            UiStatusKind::Warning,
            UiStatusKind::Attention,
            UiStatusKind::Error,
        ] {
            assert!(draws_a_mark(CornerMark::Notice(kind)), "{kind:?}");
        }
        assert!(!draws_a_mark(CornerMark::Quiet));
        assert!(!draws_a_mark(CornerMark::Blank));
        assert_eq!(mark_name(CornerMark::Fine), "fine");
        assert_eq!(
            mark_name(CornerMark::Notice(UiStatusKind::Attention)),
            "attention"
        );
        assert_eq!(mark_name(CornerMark::Quiet), "quiet");
        assert_eq!(mark_name(CornerMark::Blank), "blank");
    }

    /// The notch is 24 px tall in both states, and its reading one line.
    #[test]
    fn the_notch_is_24px() {
        for class in [TRIGGER_CLASS, TRIGGER_OPEN_CLASS] {
            assert!(class.contains("tw:h-6"), "{class}");
        }
        assert!(reading_class(CornerMark::Fine).contains("tw:leading-none"));
    }
}
