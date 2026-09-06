//! Stories for the visitor strip (§3-A).
//!
//! The strip mounts at chrome width — full-width is the point of the G3
//! ruling, and its wrap behavior on narrow viewports is what the sm/md
//! captures judge.
//!
//! The §2-D visitor popover these used to cover retired with
//! relationship-control P5: the project segment's popover is the one door
//! now, and its `viewing someone else's` state is the visitor card's
//! successor (see `project_relationship_panel_stories`).

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::share::visitor_banner::{VisitorBanner, VisitorBannerView};

#[story(
    label = "Visitor banner, pristine",
    description = "A tracking copy that IS the service's line: live tint, \"updates arrive as they happen\", and one quiet Copy link. The strip's own Fork retired with relationship-control P5 — the project segment's popover carries the fork-family verb for every standing, and offering it twice one row apart was the duplication D2 named. Project-name-centric copy — the API exposes no owner profile (deviation from the spike's owner-name phrasing, flagged for G1)."
)]
pub(crate) fn visitor_banner_pristine() -> Element {
    strip(VisitorBannerView::ViewPristine {
        name: "radiance-dome".to_string(),
    })
}

#[story(
    label = "Visitor banner, edited",
    description = "Local saves diverged the copy: warn tint, \"updates are paused\", Discard changes and Fork-to-keep. This is the ONE fork the strip kept through P5: the state is only reachable on a persistent tracking copy, which the relationship derivation reads as \"Private\" — so the popover offers Duplicate there, not the fork-at-the-copy's-head this button dispatches."
)]
pub(crate) fn visitor_banner_edited() -> Element {
    strip(VisitorBannerView::ViewEdited)
}

#[story(
    label = "Visitor banner, edit link",
    description = "The access==Edit link-holder's third state (not in the spike; kept to one calm live-palette line per the phase ruling): saves go live for everyone, no fork nag, Copy link only."
)]
pub(crate) fn visitor_banner_edit_live() -> Element {
    strip(VisitorBannerView::EditLive {
        name: "radiance-dome".to_string(),
    })
}

/// Chrome-width mount: a bar-shaped row above, the strip below it, the way
/// it ships (full-width under the chrome).
fn strip(view: VisitorBannerView) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0",
            div { class: "tw:mb-3 tw:flex tw:items-center tw:justify-between tw:border-b tw:border-border-subtle tw:pb-2.5",
                span { class: "tw:text-sm tw:font-bold tw:text-strong-foreground", "LightPlayer" }
                span { class: "tw:text-xs tw:text-dim-foreground", "chrome stand-in" }
            }
            VisitorBanner { view }
        }
    }
}
