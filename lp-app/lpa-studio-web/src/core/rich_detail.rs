//! Rich-object sections rendered into the standard detail card.
//!
//! One renderer for every rich object's detail popover content — today the
//! board card's details (a bar's, and the status corner's): each core
//! [`RichSection`] becomes a [`DetailSection`] in the order the builder
//! emitted (Q4 — never worst-first). A section's sentence reads above its
//! lines; its verbs are the card's actions ([`UiCardAction`]), each drawn
//! from the offer it presses by [`CardAction`] as a menu row, so the web
//! builds none of them. [`RichWeight::Danger`] sections render as the inline
//! red-tinted zone behind a hard red separator (Q5).

use dioxus::prelude::*;
use lpa_studio_core::{
    RichChip, RichLine, RichSection, RichWeight, UiAction, UiCardAction, UiStatus, UiStatusKind,
};

use crate::app::board_card::{CardAction, CardActionLook};
use crate::base::{DetailSection, DetailSectionTint};
use crate::core::StatusChip;

/// One rich section inside a detail popover. A regular section shows its
/// sentence, its fact rows, its advisory chip and its verbs as menu rows; a
/// Danger section shows its destructive verbs the same way, behind the hard
/// separator.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RichDetailSection(
    section: RichSection<UiCardAction>,
    on_action: EventHandler<UiAction>,
) -> Element {
    if section.weight == RichWeight::Danger {
        return rsx! {
            // The wrapper's red border is the hard separator; the section's
            // own divider drops via its `first:` rule inside the wrapper.
            div { class: DANGER_SEPARATOR_CLASS,
                DetailSection { title: section.title, tint: DetailSectionTint::Error,
                    if let Some(sentence) = section.sentence {
                        p { class: SENTENCE_CLASS, "{sentence}" }
                    }
                    div { class: "tw:grid tw:py-1",
                        for action in section.affordances {
                            CardAction {
                                key: "{action.offer}",
                                action,
                                look: CardActionLook::MenuItem,
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
            if let Some(sentence) = section.sentence {
                p { class: SENTENCE_CLASS, "{sentence}" }
            }
            if !section.lines.is_empty() {
                dl { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-1.5 tw:py-1 tw:text-xs",
                    for line in section.lines {
                        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[88px_minmax(0,1fr)] tw:gap-2",
                            dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                                "{line.label}"
                            }
                            dd { class: line_value_class(&line), "{line.value}" }
                        }
                    }
                }
            }
            if let Some(chip) = section.chip {
                div { class: "tw:py-1",
                    StatusChip { status: chip_status(&chip) }
                }
            }
            // Verbs are menu rows, same as the danger zone's: the popover is
            // an inspector, and its one box is the popover itself — actions
            // read as rows, never as nested buttons.
            if !section.affordances.is_empty() {
                div { class: "tw:grid tw:py-1",
                    for action in section.affordances {
                        CardAction {
                            key: "{action.offer}",
                            action,
                            look: CardActionLook::MenuItem,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// The hard red line above a Danger section.
const DANGER_SEPARATOR_CLASS: &str = "tw:border-t tw:border-status-error-border";

/// A section's sentence: a notice's words or a fact the rows cannot carry,
/// above the rows, wrapping.
const SENTENCE_CLASS: &str =
    "tw:m-0 tw:py-0.5 tw:text-xs tw:leading-snug tw:text-strong-foreground tw:break-words";

/// A fact's value, in its tone (a plain fact in the muted ink).
fn line_value_class(line: &RichLine) -> &'static str {
    match line.tone {
        UiStatusKind::Neutral => {
            "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words"
        }
        UiStatusKind::Working => {
            "tw:m-0 tw:min-w-0 tw:font-mono tw:text-status-working-foreground tw:break-words"
        }
        UiStatusKind::Good => {
            "tw:m-0 tw:min-w-0 tw:font-mono tw:text-status-good-foreground tw:break-words"
        }
        UiStatusKind::Live => {
            "tw:m-0 tw:min-w-0 tw:font-mono tw:text-status-live-foreground tw:break-words"
        }
        UiStatusKind::Warning => {
            "tw:m-0 tw:min-w-0 tw:font-mono tw:text-status-warning-foreground tw:break-words"
        }
        UiStatusKind::Attention => {
            "tw:m-0 tw:min-w-0 tw:font-mono tw:text-status-attention-foreground tw:break-words"
        }
        UiStatusKind::Error => {
            "tw:m-0 tw:min-w-0 tw:font-mono tw:text-status-error-foreground tw:break-words"
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
        UiStatusKind::Live => DetailSectionTint::Live,
        UiStatusKind::Warning => DetailSectionTint::Warning,
        UiStatusKind::Attention => DetailSectionTint::Attention,
        UiStatusKind::Error => DetailSectionTint::Error,
    }
}

/// Advisory chip → `StatusChip` status: a section's standing chip, in its
/// own tone.
pub(crate) fn chip_status(chip: &RichChip) -> UiStatus {
    match chip.tone {
        UiStatusKind::Neutral => UiStatus::neutral(chip.text.clone()),
        UiStatusKind::Working => UiStatus::working(chip.text.clone()),
        UiStatusKind::Good => UiStatus::good(chip.text.clone()),
        UiStatusKind::Live => UiStatus::new(chip.text.clone(), UiStatusKind::Live),
        UiStatusKind::Warning => UiStatus::warning(chip.text.clone()),
        UiStatusKind::Attention => UiStatus::attention(chip.text.clone()),
        UiStatusKind::Error => UiStatus::error(chip.text.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tone has a tint, Live included (blue: Update), and Neutral is
    /// untinted.
    #[test]
    fn every_tone_maps_to_its_tint() {
        assert_eq!(
            rich_section_tint(UiStatusKind::Neutral),
            DetailSectionTint::None
        );
        assert_eq!(
            rich_section_tint(UiStatusKind::Live),
            DetailSectionTint::Live
        );
        assert_eq!(
            rich_section_tint(UiStatusKind::Error),
            DetailSectionTint::Error
        );
        assert_eq!(
            rich_section_tint(UiStatusKind::Attention),
            DetailSectionTint::Attention
        );
    }

    /// A Danger section keeps its hard red separator, and a sentence reads
    /// as a wrapping paragraph, never a clipped line.
    #[test]
    fn danger_keeps_its_separator_and_the_sentence_wraps() {
        assert!(DANGER_SEPARATOR_CLASS.contains("tw:border-t"));
        assert!(DANGER_SEPARATOR_CLASS.contains("tw:border-status-error-border"));
        assert!(SENTENCE_CLASS.contains("tw:break-words"));
        assert!(!SENTENCE_CLASS.contains("truncate"));
    }

    /// Rendered: a section's sentence reads above its lines, and a Danger
    /// section sits behind its hard red line.
    #[test]
    fn the_sentence_renders_and_danger_keeps_its_line() {
        let notice = RichSection {
            title: "Firmware".to_string(),
            tone: UiStatusKind::Live,
            sentence: Some("A newer firmware is ready.".to_string()),
            lines: vec![RichLine::new("Version", "2026.10.05-2")],
            chip: None,
            affordances: Vec::new(),
            weight: RichWeight::Actionable,
        };
        let html = render_section(notice);
        let sentence = html
            .find("A newer firmware is ready.")
            .expect("the sentence");
        let line = html.find("2026.10.05-2").expect("the line");
        assert!(
            sentence < line,
            "the sentence reads above the lines: {html}"
        );
        assert!(!html.contains(DANGER_SEPARATOR_CLASS), "{html}");

        let danger = RichSection {
            title: "Danger zone".to_string(),
            tone: UiStatusKind::Error,
            sentence: None,
            lines: Vec::new(),
            chip: None,
            affordances: Vec::new(),
            weight: RichWeight::Danger,
        };
        assert!(render_section(danger).contains(DANGER_SEPARATOR_CLASS));
    }

    /// A fact's value reads in its tone; a plain fact in the muted ink.
    #[test]
    fn a_line_reads_in_its_tone() {
        let plain = RichLine::new("Wi‑Fi", "home");
        assert!(line_value_class(&plain).contains("tw:text-muted-foreground"));
        let warn = RichLine::new("Wi‑Fi", "wrong password").toned(UiStatusKind::Warning);
        assert!(line_value_class(&warn).contains("tw:text-status-warning-foreground"));
    }

    /// `section` rendered to markup on the host.
    fn render_section(section: RichSection<UiCardAction>) -> String {
        crate::app::board_card::card_test_fixtures::render(
            SectionRoot,
            SectionRootProps { section },
        )
    }

    #[component]
    #[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
    fn SectionRoot(section: RichSection<UiCardAction>) -> Element {
        rsx! {
            RichDetailSection { section, on_action: |_| {} }
        }
    }
}
