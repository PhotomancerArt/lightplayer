//! Stories for the ⌘K command palette and its site-chrome hint.
//!
//! The palette renders `inline` here (pinned in its box rather than over
//! the viewport) and pre-typed, since a capture can neither press ⌘K nor
//! type. The trees are the ones a dirty project publishes: Save and Revert
//! to saved in the header, and a dirty playlist node's revert and remove.

use dioxus::prelude::*;
use lpa_studio_core::{OfferPath, UiOffer, UiOfferTree};
use lpa_studio_web_story_macros::story;

use super::command_palette_dialog::CommandPaletteDialog;
use super::command_palette_hint::CommandPaletteHint;
use crate::app::layout::site_chrome::{SiteChrome, SiteSection};
use crate::app::layout::version_badge::{BuildChip, VersionChipPreview};
use crate::app::node::node_story_fixtures::{PLAYLIST_NODE, node_delete_offer, node_revert_offer};
use crate::app::story_fixtures::project_save_revert_offers;
use crate::base::Platform;

#[story(
    description = "Open over a dirty project: every offer the view publishes, in publish order — Save, Revert to saved (Lasting, so tinted), the playlist's revert, and its removal (Undoable, so tinted). Each row's path sits beside it, quietly: the id the app agent presses it by. The first row is highlighted; ↑↓ move, ↵ presses, esc closes."
)]
fn dirty_project() -> Element {
    palette(dirty_project_offers(), "", false)
}

#[story(
    description = "Filtered by \"rev\": the two reverts lead (the query is in their labels), and Remove node follows (r·e…v, the query's letters in order from the word's start). Save drops out — its path has the letters, but scattered mid-word."
)]
fn filtered() -> Element {
    palette(dirty_project_offers(), "rev", false)
}

#[story(
    description = "A Lasting row armed: one ↵ on Revert to saved, which discards every unsaved edit, arms it instead of pressing — the row reads \"Confirm revert\" and the second ↵ presses. Moving off it, or four seconds, stands it down."
)]
fn lasting_armed() -> Element {
    palette(dirty_project_offers(), "revert", true)
}

#[story(
    description = "A disabled row: Save while a save is already running says why under its label, and the arrow keys pass over it — the highlight starts on the first row ↵ can press."
)]
fn disabled_row() -> Element {
    let mut offers = UiOfferTree::new();
    for offer in dirty_project_offers().iter() {
        let mut offer = offer.clone();
        if offer.path == OfferPath::project().child("save") {
            offer.action = offer.action.disabled("A save is already running.");
        }
        offers.publish(offer);
    }
    palette(offers, "", false)
}

#[story(
    description = "The empty tree: a page that publishes nothing yet (Home before any project is open) says so instead of drawing an empty box."
)]
fn empty() -> Element {
    palette(UiOfferTree::new(), "", false)
}

#[story(
    description = "The site chrome's hint: a quiet \"⌘K\" chip (\"Ctrl+K\" off a Mac) at the head of the right cluster, before the build chip. Clicking it opens the palette, for anyone who doesn't reach for the keyboard."
)]
fn chrome_hint() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2",
            for platform in [Platform::Mac, Platform::Other] {
                div {
                    class: "tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
                    style: "max-width: 1000px;",
                    SiteChrome { section: SiteSection::Home,
                        CommandPaletteHint { on_open: |_| {}, platform: Some(platform) }
                        VersionChipPreview {
                            chip: BuildChip::Release("2026.10.02-1".to_string()),
                        }
                    }
                }
            }
        }
    }
}

/// The palette, pinned in its story box, pre-typed with `query`.
fn palette(offers: UiOfferTree, query: &str, armed: bool) -> Element {
    rsx! {
        CommandPaletteDialog {
            offers,
            on_action: |_| {},
            on_close: |_| {},
            inline: true,
            initial_query: query.to_string(),
            armed_preview: armed,
        }
    }
}

/// What a dirty project publishes: the header's Save and Revert to saved,
/// then the dirty playlist's revert and its (Undoable) removal.
fn dirty_project_offers() -> UiOfferTree {
    let mut offers = project_save_revert_offers();
    let node_offers: [UiOffer; 2] = [node_revert_offer(PLAYLIST_NODE), node_delete_offer()];
    for offer in node_offers {
        offers.publish(offer);
    }
    offers
}
