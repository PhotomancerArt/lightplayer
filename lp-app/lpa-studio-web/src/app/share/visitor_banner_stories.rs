//! Stories for the visitor strip (§3-A) and the visitor popover (§2-D).
//!
//! The strip mounts at chrome width — full-width is the point of the G3
//! ruling, and its wrap behavior on narrow viewports is what the sm/md
//! captures judge. The popover story mounts the real popover open at the
//! end of a chrome-width row, like the owner-pill stories (the agent
//! browser pane misdraws outline popovers, so the story PNG is the only
//! honest evidence).

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;
use lpc_cloud_api::Access;
use lpc_history::{PrefixedUid, UidPrefix};

use crate::app::share::share_url::ShareUrl;
use crate::app::share::visitor_banner::{VisitorBanner, VisitorBannerView};
use crate::app::share::visitor_popover::VisitorSharePopover;

#[story(
    label = "Visitor banner, pristine",
    description = "A tracking copy that IS the service's line: live tint, \"updates arrive as they happen\", quiet Copy link and the accent Fork. Project-name-centric copy — the API exposes no owner profile (deviation from the spike's owner-name phrasing, flagged for G1)."
)]
pub(crate) fn visitor_banner_pristine() -> Element {
    strip(VisitorBannerView::ViewPristine {
        name: "radiance-dome".to_string(),
    })
}

#[story(
    label = "Visitor banner, edited",
    description = "Local saves diverged the copy: warn tint, \"updates are paused\", Discard changes and Fork-to-keep. The banner is the fork's home (§3-A ruling) — the one moment the fork offer earns its keep."
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

#[story(
    label = "Visitor popover, open",
    description = "The §2-D card in the owner pill's slot: what this is (\"Shared with you · anyone with the link can view\"), the same URL hero the owner sees, and the one verb a visitor owns — Fork, with its one-line explanation."
)]
pub(crate) fn visitor_popover_open() -> Element {
    rsx! {
        div { class: "tw:min-h-[360px]",
            div { class: "tw:flex tw:items-center tw:justify-end tw:gap-2 tw:border-b tw:border-border-subtle tw:pb-2.5",
                VisitorSharePopover {
                    name: "radiance-dome".to_string(),
                    url: url(),
                    access: Access::View,
                    initially_open: true,
                }
            }
        }
    }
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

fn url() -> ShareUrl {
    ShareUrl {
        origin: "lightplayer.app".to_string(),
        slug: "radiance-dome".to_string(),
        uid: Some(PrefixedUid::mint(UidPrefix::Project, &[11u8; 16])),
    }
}
