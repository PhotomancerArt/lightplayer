//! [`NameBar`]: the board's name (one line, cut, its whole on hover), its
//! place under it when core gives one (the bar keeps its 50 px either way),
//! and the one primary as a flush section at the bar's end — icon before
//! word, the spectrum ring on hover (`docs/style/ui.md` "The board card").
//!
//! The bar carries `data-name-bar`, the walks' hook for the primary (an
//! empty board's Add a project is pressed there).
//!
//! While the board is being reached or identified the primary is Studio's
//! conic spinner alone in the same section ([`UiPrimary::Working`]), its
//! label the accessible name: the Link row (or the Hardware row) says how.
//! When nothing can be pressed there is no section at all — never a greyed
//! verb, and never a row's words said again here (P2, P7). The spinner
//! presses nothing, so it is not an action.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiNameBar, UiPrimary};

use super::card_action::{CardAction, CardActionLook};
use crate::base::conic_spinner_class;

/// The name bar. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NameBar(bar: UiNameBar, on_action: EventHandler<UiAction>) -> Element {
    let primary = match bar.primary {
        Some(UiPrimary::Offer(action)) => rsx! {
            CardAction { action, look: CardActionLook::Primary, on_action }
        },
        Some(UiPrimary::Working { label }) => rsx! {
            WorkingPrimary { label }
        },
        None => rsx! {},
    };
    rsx! {
        div { class: NAME_BAR_CLASS, "data-name-bar": "true",
            div { class: NAME_BLOCK_CLASS,
                h3 { class: TITLE_CLASS, title: "{bar.title}", "{bar.title}" }
                if let Some(place) = bar.place {
                    p { class: PLACE_CLASS, title: "{place}", "{place}" }
                }
            }
            {primary}
        }
    }
}

/// The spinner alone, in the primary's section: the board is being
/// reached or identified.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WorkingPrimary(label: String) -> Element {
    rsx! {
        div {
            class: WORKING_CLASS,
            role: "status",
            aria_label: "{label}",
            title: "{label}",
            "data-primary-working": "true",
            span { class: "{conic_spinner_class()} ux-board-spinner", aria_hidden: "true" }
        }
    }
}

/// The bar: 50 px with its top hairline, flush to the card's right edge so
/// the primary's section reaches it. Nothing here clips: the primary's
/// hover glow reaches past its section.
const NAME_BAR_CLASS: &str = "tw:flex tw:h-[50px] tw:min-w-0 tw:items-stretch tw:gap-2 tw:border-0 tw:border-t tw:border-solid tw:border-border-muted tw:pl-[11px]";

/// The name and its place, centred in the bar.
const NAME_BLOCK_CLASS: &str = "tw:grid tw:min-w-0 tw:flex-1 tw:content-center tw:gap-px";

/// The name: one line, cut.
const TITLE_CLASS: &str = "tw:m-0 tw:min-w-0 tw:truncate tw:text-[13.5px] tw:font-bold tw:leading-tight tw:text-strong-foreground";

/// The place under the name.
const PLACE_CLASS: &str =
    "tw:m-0 tw:min-w-0 tw:truncate tw:text-[11.5px] tw:leading-tight tw:text-subtle-foreground";

/// The spinner's section: the row's height and the primary's own leading
/// hairline, flush to the card's edge, quiet (no wash, no ring: nothing to
/// press).
const WORKING_CLASS: &str = "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:self-stretch tw:border-0 tw:border-l tw:border-solid tw:border-border tw:px-[18px]";

#[cfg(test)]
mod tests {
    use super::*;

    /// The name bar is one fixed 50 px row whose name never wraps.
    #[test]
    fn the_name_bar_is_fixed_and_its_name_one_line() {
        assert!(NAME_BAR_CLASS.contains("tw:h-[50px]"));
        assert!(!NAME_BAR_CLASS.contains("min-h"));
        assert!(TITLE_CLASS.contains("tw:truncate"));
        assert!(PLACE_CLASS.contains("tw:truncate"));
    }

    /// The primary's glow is a bloom past its section, so nothing around it
    /// clips; and the primary's look is the ringed flush section.
    #[test]
    fn nothing_around_the_primary_verb_clips_its_glow() {
        use crate::core::ActionButtonVariant;
        use crate::core::action::action_variant_class;
        for class in [NAME_BAR_CLASS, NAME_BLOCK_CLASS, WORKING_CLASS] {
            assert!(!class.contains("overflow"), "{class}");
        }
        let primary = action_variant_class(ActionButtonVariant::RowPrimary, false);
        assert!(primary.contains("ux-ir-ring"), "{primary}");
        assert!(!primary.contains("ux-spectrum-cta"), "{primary}");
        assert!(WORKING_CLASS.contains("tw:self-stretch"));
    }

    /// Connecting or identifying: the spinner alone, named for a screen
    /// reader, no words and no button.
    #[test]
    fn a_working_primary_is_the_spinner_alone() {
        let html = super::super::card_test_fixtures::render(working, ());
        assert!(html.contains("data-primary-working"), "{html}");
        assert!(html.contains("aria-label=\"Connecting\""), "{html}");
        assert!(!html.contains("<button"), "nothing to press: {html}");
    }

    fn working() -> Element {
        rsx! {
            WorkingPrimary { label: "Connecting".to_string() }
        }
    }
}
