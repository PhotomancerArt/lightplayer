//! The four kinds of section a bar's details are made of, so every bar
//! builds them the same way: its notice (first, Actionable, in the bar's
//! tone — what the status corner rolls up), its facts, its verbs, and its
//! danger section (last, the Lasting verbs that lose something).

use super::ui_card_action::UiCardAction;
use crate::{RichLine, RichSection, RichWeight, UiStatusKind};

/// A bar's notice: a sentence in the bar's tone, with the verb that answers
/// it when there is one. Actionable, so the status corner rolls it up.
pub(crate) fn notice(
    title: &str,
    tone: UiStatusKind,
    sentence: impl Into<String>,
    answer: Option<UiCardAction>,
) -> RichSection<UiCardAction> {
    RichSection {
        title: title.to_string(),
        tone,
        sentence: Some(sentence.into()),
        lines: Vec::new(),
        chip: None,
        affordances: answer.into_iter().collect(),
        weight: RichWeight::Actionable,
    }
}

/// A tint a bar wears that the status corner does not report: a standing
/// fact worth its colour, not something to fix now.
pub(crate) fn caution(
    title: &str,
    tone: UiStatusKind,
    sentence: impl Into<String>,
) -> RichSection<UiCardAction> {
    RichSection {
        weight: RichWeight::Advisory,
        ..notice(title, tone, sentence, None)
    }
}

/// Label→value facts under `title`.
pub(crate) fn facts(title: &str, lines: Vec<RichLine>) -> RichSection<UiCardAction> {
    RichSection {
        title: title.to_string(),
        tone: UiStatusKind::Neutral,
        sentence: None,
        lines,
        chip: None,
        affordances: Vec::new(),
        weight: RichWeight::Advisory,
    }
}

/// The bar's verbs, in order.
pub(crate) fn verbs(actions: Vec<UiCardAction>) -> RichSection<UiCardAction> {
    RichSection {
        title: "Actions".to_string(),
        tone: UiStatusKind::Neutral,
        sentence: None,
        lines: Vec::new(),
        chip: None,
        affordances: actions,
        weight: RichWeight::Advisory,
    }
}

/// The verbs that lose something, last and apart.
pub(crate) fn danger(actions: Vec<UiCardAction>) -> RichSection<UiCardAction> {
    RichSection {
        title: "Danger zone".to_string(),
        tone: UiStatusKind::Error,
        sentence: None,
        lines: Vec::new(),
        chip: None,
        affordances: actions,
        weight: RichWeight::Danger,
    }
}

/// `sections`, without the verb and danger sections that came out empty.
pub(crate) fn without_empty(
    sections: Vec<RichSection<UiCardAction>>,
) -> Vec<RichSection<UiCardAction>> {
    sections
        .into_iter()
        .filter(|section| {
            section.sentence.is_some()
                || !section.lines.is_empty()
                || !section.affordances.is_empty()
        })
        .collect()
}
