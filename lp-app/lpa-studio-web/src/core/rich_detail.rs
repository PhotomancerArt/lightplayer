//! Rich-object sections rendered into the standard detail card.
//!
//! One renderer for every rich object's detail popover content: each core
//! [`RichSection`] becomes a [`DetailSection`] in the FIXED schema order
//! the builder emitted (Q4 — never worst-first). Affordances arrive
//! already wired as [`UiAction`]s (the domain layer owns identity→action
//! mapping); [`RichWeight::Danger`] sections render as the inline
//! red-tinted zone behind a hard red separator (Q5) with destructive menu
//! rows.

use dioxus::prelude::*;
use lpa_studio_core::{RichChip, RichSection, RichWeight, UiAction, UiStatus, UiStatusKind};

use crate::base::{DetailSection, DetailSectionTint};
use crate::core::{ActionButton, ActionButtonVariant, StatusChip};

/// One rich section inside a detail popover. Regular sections show their
/// fact rows, advisory chip, and ≤1 affordance as a menu row; a Danger
/// section shows its destructive verbs the same way, behind the hard
/// separator.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RichDetailSection(
    section: RichSection<UiAction>,
    on_action: EventHandler<UiAction>,
) -> Element {
    if section.weight == RichWeight::Danger {
        return rsx! {
            // The wrapper's red border is the hard separator; the section's
            // own divider drops via its `first:` rule inside the wrapper.
            div { class: "tw:border-t tw:border-status-error-border",
                DetailSection { title: section.title, tint: DetailSectionTint::Error,
                    div { class: "tw:grid tw:py-1",
                        for action in section.affordances {
                            ActionButton {
                                action,
                                running: false,
                                variant: ActionButtonVariant::MenuItem,
                                on_action,
                            }
                        }
                    }
                }
            }
        };
    }

    rsx! {
        DetailSection { title: section.title, tint: rich_section_tint(section.tone),
            if !section.lines.is_empty() {
                dl { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-1.5 tw:py-1 tw:text-xs",
                    for line in section.lines {
                        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[88px_minmax(0,1fr)] tw:gap-2",
                            dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                                "{line.label}"
                            }
                            dd { class: "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words",
                                "{line.value}"
                            }
                        }
                    }
                }
            }
            if let Some(chip) = section.chip {
                div { class: "tw:py-1",
                    StatusChip { status: chip_status(&chip) }
                }
            }
            // Affordances are menu rows, same as the danger zone's: the
            // popover is an inspector, and its one box is the popover
            // itself — actions read as rows, never as nested buttons.
            for action in section.affordances {
                div { class: "tw:py-1",
                    ActionButton {
                        action,
                        running: false,
                        variant: ActionButtonVariant::MenuItem,
                        on_action,
                    }
                }
            }
        }
    }
}

/// Section tone → detail-section tint (the status families map 1:1;
/// Neutral renders untinted).
pub fn rich_section_tint(tone: UiStatusKind) -> DetailSectionTint {
    match tone {
        UiStatusKind::Neutral => DetailSectionTint::None,
        UiStatusKind::Working => DetailSectionTint::Working,
        UiStatusKind::Good => DetailSectionTint::Good,
        UiStatusKind::Warning => DetailSectionTint::Warning,
        UiStatusKind::Error => DetailSectionTint::Error,
    }
}

fn chip_status(chip: &RichChip) -> UiStatus {
    match chip.tone {
        UiStatusKind::Neutral => UiStatus::neutral(chip.text.clone()),
        UiStatusKind::Working => UiStatus::working(chip.text.clone()),
        UiStatusKind::Good => UiStatus::good(chip.text.clone()),
        UiStatusKind::Warning => UiStatus::warning(chip.text.clone()),
        UiStatusKind::Error => UiStatus::error(chip.text.clone()),
    }
}
