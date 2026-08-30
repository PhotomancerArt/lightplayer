//! Stories for the pattern card's "New project from this…" row (module
//! authoring unit, P5).
//!
//! The row is the card-side twin of the picker's import: import brings a
//! pattern into the project you are already in, this hands you a workbench
//! built around it. It only exists on PATTERN cards — a general project has
//! no answer to "from which module" — so both baselines capture the menu
//! open, which is the only state where the row is visible at all.
//!
//! The pair is single-export vs family, because that is the only shape
//! difference: a family grows a select ahead of the name field, and a
//! single-export package must never see that control.

use dioxus::prelude::*;
use lpa_studio_core::UiPackageCard;
use lpa_studio_core::app::library::PackageHealth;
use lpa_studio_web_story_macros::story;

use crate::app::home::package_card::PackageCard;

/// Fixed clock so "edited N hours ago" cannot drift between captures.
const STORY_NOW: f64 = 1_770_000_000.0;

#[story(
    description = "A single-export pattern card with its menu open: the new \"New project from this…\" form leads, prefilled `effect-project` — named after what you are starting from, not \"Untitled\". No export chooser, because there is nothing to choose. Rename and the existing verbs keep their places below, so the pattern card is the general card plus one section rather than a different card."
)]
pub(crate) fn pattern_card_new_from() -> Element {
    card(pattern_card(vec!["effect".to_string()]))
}

#[story(
    description = "The same form on a FAMILY (two exports): a select appears ahead of the name field, because \"from this\" is ambiguous the moment a package ships two modules. The prefill follows the selected export until you type over it, at which point your name wins and stops moving under you."
)]
pub(crate) fn pattern_family_card_new_from() -> Element {
    card(pattern_card(vec!["fire".to_string(), "ice".to_string()]))
}

fn pattern_card(exports: Vec<String>) -> UiPackageCard {
    UiPackageCard {
        uid: "prj_7hTn4Vb88cRpZq2FjXsLmo".to_string(),
        kind: "Module".to_string(),
        project_kind: "Pattern".to_string(),
        exports,
        slug: "2026-08-06-0930-sparkle-pack".to_string(),
        last_saved_at: Some(STORY_NOW - 3.0 * 3600.0),
        provenance: None,
        on_device: None,
        open_elsewhere: false,
        connected_device: None,
        running_in_sim: false,
        target: None,
        health: PackageHealth::Ready,
    }
}

#[story(
    description = "The card-overlay redesign's depth surface on a BLOCKED card: the slim face carries only the amber headline (plus the attention glyph), and the remedy — the sentence telling you what to do about it — moves into the ⋯ popup's status section, above the two actions that work on raw files. The face stays a picture; the words that need room get room."
)]
pub(crate) fn blocked_card_menu() -> Element {
    let mut blocked = pattern_card(Vec::new());
    blocked.project_kind = "General".to_string();
    blocked.health = PackageHealth::Blocked {
        headline: "Format 3 — too old for this Studio".to_string(),
        remedy: "Export a copy or delete it.".to_string(),
    };
    card(blocked)
}

/// One card on enough canvas for its menu to open downward in place.
fn card(card: UiPackageCard) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-h-[560px] tw:content-start tw:p-4",
            div { class: "tw:w-[280px]",
                PackageCard {
                    card,
                    now_secs: STORY_NOW,
                    menu_initially_open: true,
                    on_action: move |_| {},
                }
            }
        }
    }
}
