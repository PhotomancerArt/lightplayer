//! [`NameBar`]: the board's name (one line, cut, its whole on hover), its
//! place under it when core gives one (the bar keeps its 50 px either way),
//! and the one primary as a flush section at the bar's end — icon before
//! word, the spectrum ring on hover (`docs/style/ui.md` "The board card").
//!
//! An unavailable primary ([`UiPrimary::Unavailable`]) is the same section,
//! disabled, its reason its title and its hidden text: "why can't I?" is
//! answered where it is asked. It presses nothing, so it is not an action.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiNameBar, UiPrimary};

use super::card_action::{CardAction, CardActionLook};
use crate::base::{StudioIcon, action_icon_name};
use crate::core::ActionButtonVariant;
use crate::core::action::action_variant_class;

/// The name bar. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NameBar(bar: UiNameBar, on_action: EventHandler<UiAction>) -> Element {
    let primary = match bar.primary {
        Some(UiPrimary::Offer(action)) => rsx! {
            CardAction { action, look: CardActionLook::Primary, on_action }
        },
        Some(UiPrimary::Unavailable { word, icon, reason }) => rsx! {
            UnavailablePrimary { word, icon, reason }
        },
        None => rsx! {},
    };
    rsx! {
        div { class: NAME_BAR_CLASS,
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

/// The primary it would be, disabled, saying why.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UnavailablePrimary(word: String, icon: String, reason: String) -> Element {
    let icon = action_icon_name(Some(icon.as_str()));
    rsx! {
        div { class: PRIMARY_SLOT_CLASS,
            button {
                class: action_variant_class(ActionButtonVariant::RowPrimary, false),
                r#type: "button",
                disabled: true,
                title: "{reason}",
                if let Some(icon) = icon {
                    span { class: "tw:inline-flex tw:flex-none tw:items-center tw:justify-center", aria_hidden: "true",
                        StudioIcon { name: icon, size: 12 }
                    }
                }
                span { class: "tw:inline-flex", "{word}" }
            }
            span { class: "tw:sr-only", "{reason}" }
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

/// The unavailable primary's slot: the row's height, as a pressable one's.
const PRIMARY_SLOT_CLASS: &str = "tw:grid tw:min-w-0 tw:flex-none tw:self-stretch";

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
        for class in [NAME_BAR_CLASS, NAME_BLOCK_CLASS, PRIMARY_SLOT_CLASS] {
            assert!(!class.contains("overflow"), "{class}");
        }
        let primary = action_variant_class(ActionButtonVariant::RowPrimary, false);
        assert!(primary.contains("ux-ir-ring"), "{primary}");
        assert!(!primary.contains("ux-spectrum-cta"), "{primary}");
        assert!(PRIMARY_SLOT_CLASS.contains("tw:self-stretch"));
    }
}
