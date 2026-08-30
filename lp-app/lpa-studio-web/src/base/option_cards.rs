//! The EXPLAINING selector: a single-select row whose options are cards —
//! icon, title, and a line saying what picking it means.
//!
//! Studio reaches for this whenever a choice is a MODE rather than a value:
//! a bare toggle button names one state and leaves the user to guess the
//! other, and a `<select>` hides both behind a click. Cards show every
//! option, what it is called, and what it does, all at once — which is what
//! a walk-up user needs from a control they have never met.
//!
//! The look was ruled in the node face's space section (the shape/modifier
//! tiles, G1b: "inline tiles, no popover, no dropdown", selected = a strong
//! border + wash + a check badge — recolored to the app-wide neutral
//! SELECTION family by the accent reckoning: a picked card IS a selection,
//! and selection never wears a hue). That site drew its own faces from
//! projection glyphs and dispatches slot-op sequences, so it keeps its own
//! component and shares the STYLING here ([`option_card_grid_class`],
//! [`option_card_class`], [`OPTION_CARD_CHECK_CLASS`]) — one visual
//! language, whatever a card's face happens to be.

use dioxus::prelude::*;

use crate::base::icon::{StudioIcon, StudioIconName};

/// One option: what it is called, what it means, and the glyph that stands
/// for it. `id` is the caller's own key — it comes back on pick.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionCard {
    pub id: String,
    pub icon: StudioIconName,
    pub title: String,
    /// One terse line: the consequence of picking this, in the user's terms.
    pub blurb: String,
}

impl OptionCard {
    pub fn new(id: &str, icon: StudioIconName, title: &str, blurb: &str) -> Self {
        Self {
            id: id.to_string(),
            icon,
            title: title.to_string(),
            blurb: blurb.to_string(),
        }
    }
}

/// The grid every card set lays out in — one template, so sets align.
pub fn option_card_grid_class() -> &'static str {
    "tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] tw:gap-1.5"
}

/// One card. Selected = selection border + selection wash + the check
/// badge — three signals, because the old filled-grey treatment was ruled
/// hard to read.
pub fn option_card_class(selected: bool) -> &'static str {
    if selected {
        "tw:relative tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:gap-0.5 tw:rounded-xs tw:border tw:border-selection-border tw:bg-selection-bg tw:p-1.5 tw:text-left tw:text-strong-foreground"
    } else {
        "tw:relative tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:gap-0.5 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-transparent tw:p-1.5 tw:text-left tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground"
    }
}

/// The selected card's check badge, over its top-right corner — the
/// selection border tone filled solid, with the dark page color for the
/// glyph so the check reads at 10px.
pub const OPTION_CARD_CHECK_CLASS: &str = "tw:absolute tw:right-1 tw:top-1 tw:inline-flex tw:h-4 tw:w-4 tw:items-center tw:justify-center tw:rounded-pill tw:bg-selection-border tw:text-background";

/// A single-select row of explaining cards.
///
/// Picking the option that is ALREADY selected fires nothing: the control is
/// a selector, not a toggle, so re-picking is a no-op rather than a write.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OptionCards(
    /// Optional row label above the cards (the field name).
    #[props(default)]
    label: Option<String>,
    options: Vec<OptionCard>,
    /// The selected option's id, when one is.
    #[props(default)]
    selected: Option<String>,
    on_pick: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1",
            if let Some(label) = label {
                span { class: "tw:font-mono tw:text-[9.5px] tw:uppercase tw:tracking-[0.13em] tw:text-dim-foreground",
                    "{label}"
                }
            }
            div { class: option_card_grid_class(), role: "radiogroup",
                for option in options {
                    {
                        let picked = selected.as_deref() == Some(option.id.as_str());
                        let id = option.id.clone();
                        rsx! {
                            button {
                                key: "{option.id}",
                                class: option_card_class(picked),
                                r#type: "button",
                                role: "radio",
                                aria_checked: if picked { "true" } else { "false" },
                                title: "{option.blurb}",
                                onclick: move |event: MouseEvent| {
                                    event.stop_propagation();
                                    if !picked {
                                        on_pick.call(id.clone());
                                    }
                                },
                                if picked {
                                    span { class: OPTION_CARD_CHECK_CLASS, aria_hidden: "true",
                                        StudioIcon { name: StudioIconName::StepComplete, size: 10 }
                                    }
                                }
                                span { class: "tw:flex tw:items-center tw:gap-1",
                                    span { class: "tw:inline-flex tw:flex-none tw:items-center", aria_hidden: "true",
                                        StudioIcon { name: option.icon, size: 12 }
                                    }
                                    span { class: "tw:min-w-0 tw:truncate tw:text-[11.5px] tw:font-medium",
                                        "{option.title}"
                                    }
                                }
                                span { class: "tw:min-w-0 tw:text-[10px] tw:leading-snug tw:text-dim-foreground",
                                    "{option.blurb}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Selected cards wear all three signals; unselected ones wear none of
    /// them — the G1b ruling, in the one place both card sets read it from.
    /// A picked card is a SELECTION, so it wears the neutral selection
    /// family (accent reckoning), never a hue.
    #[test]
    fn the_selected_card_wears_the_selection_family_and_the_plain_one_does_not() {
        let picked = option_card_class(true);
        assert!(picked.contains("tw:border-selection-border"));
        assert!(picked.contains("tw:bg-selection-bg"));
        assert!(!picked.contains("accent"));
        let plain = option_card_class(false);
        assert!(!plain.contains("tw:border-selection-border"));
        assert!(!plain.contains("tw:bg-selection-bg"));
        assert!(plain.contains("tw:hover:border-border-strong"));
        // Both position the check badge, which is absolutely placed.
        assert!(picked.contains("tw:relative") && plain.contains("tw:relative"));
        assert!(OPTION_CARD_CHECK_CLASS.contains("tw:absolute"));
    }

    #[test]
    fn a_card_carries_its_id_icon_title_and_blurb() {
        let card = OptionCard::new(
            "manual",
            StudioIconName::Edited,
            "manual",
            "only what you patch lights up",
        );
        assert_eq!(card.id, "manual");
        assert_eq!(card.title, "manual");
        assert_eq!(card.blurb, "only what you patch lights up");
        assert_eq!(card.icon, StudioIconName::Edited);
    }
}
