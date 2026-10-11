//! Stories for the home page's New template menu, in the add row it shares
//! with Import and Paste.
//!
//! The menu is only ever seen open, so the baseline captures it open —
//! and beside the Import / Paste chips it shares the row with, because the
//! thing worth checking is that the trigger still reads as one of three
//! peers rather than a new kind of control.

use dioxus::prelude::*;
use lpa_studio_core::UiHomeSection;
use lpa_studio_web_story_macros::story;

use crate::app::home::page::page_section::PageSection;
use crate::app::home::page::project_add_row::ProjectAddRow;
use crate::core::OffersProvider;

#[story(
    description = "The New menu open on the add row under Other projects: one optional name field over three template rows, each a title over a dim one-liner. The field's placeholder says what blank means — the library names the package after the template, the ruling that a template needs no prompt — and a typed name rides whichever row is clicked, so naming is a keystroke away without becoming a step. Blank is what `New` has always meant and stays first; the two pattern rows say what rig they build AND that they export `effect/`, so the export boundary is legible before the project exists. Text-first by design — the spike's visual template cards fight the 320px detail-card cap, and the picker's flat-list grammar is what the rest of Studio's menus use."
)]
pub(crate) fn menu_open() -> Element {
    add_row(true)
}

#[story(
    description = "The same add row at rest: the New trigger keeps the quiet-chip look it shares with Import and Paste, so what changed is only what it opens."
)]
pub(crate) fn trigger_at_rest() -> Element {
    add_row(false)
}

/// The home page's Other projects section with nothing in it but the add
/// row — the shipped component itself, so the story cannot drift from the
/// page — with the New menu optionally open.
fn add_row(open: bool) -> Element {
    // `project/new` as core publishes it with a library mounted.
    let mut offers = lpa_studio_core::UiOfferTree::new();
    offers.publish(lpa_studio_core::new_project_offer(true));
    rsx! {
        OffersProvider { offers,
            div { class: "tw:grid tw:min-h-[320px] tw:content-start tw:p-4",
                PageSection { title: UiHomeSection::OtherProjects.label(),
                    ProjectAddRow { busy: false, menu_open: open, on_action: |_| {} }
                }
            }
        }
    }
}
