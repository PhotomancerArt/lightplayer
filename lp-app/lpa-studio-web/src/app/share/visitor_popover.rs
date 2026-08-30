//! The visitor's share door (spike `project-share` §2-D): the same pill
//! slot the owner's Share control lives in, opening the read-only card —
//! what this link is, the link itself, and the one verb a visitor owns
//! (Fork).
//!
//! Deliberately the smallest honest door: no access segment (not theirs to
//! move), no people (the roster is answered to members only, P2), and no
//! pretend administration. The copy is project-name-centric — the API
//! exposes no owner profile, so "Shared with you" is the honest phrasing
//! (noted as a deviation from the spike's "Shared by Yona Appletree" for
//! the G1 gate).

use dioxus::prelude::*;
use dioxus_icons::lucide::{GitBranch, UserRound, X};
use lpc_cloud_api::Access;

use crate::app::share::share_panel::ShareUrlHero;
use crate::app::share::share_url::ShareUrl;
use crate::base::{
    InlineButtonTone, PopoverButton, PopoverCloseHandle, PopoverPlacement, inline_icon_button_class,
};

/// The pill-slot trigger and its anchored visitor card.
///
/// Wears the owner pill's exact chrome (same slot, same rest/open
/// treatment) so the bar reads identically for everyone; what differs is
/// only what the door opens onto.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn VisitorSharePopover(
    /// What the card's title calls this project (sidecar name).
    name: String,
    /// The canonical link, in the pieces the hero paints.
    url: ShareUrl,
    /// What holding the link grants — phrases the description line.
    access: Access,
    #[props(default)] on_copy: Option<EventHandler<()>>,
    #[props(default)] on_fork: Option<EventHandler<()>>,
    /// Stories only: mount the card open (capture cannot click).
    #[props(default = false)]
    initially_open: bool,
) -> Element {
    rsx! {
        PopoverButton {
            class: PILL_CLASS.to_string(),
            open_class: PILL_OPEN_CLASS.to_string(),
            trigger: rsx! {
                span { class: "tw:flex tw:flex-none tw:text-accent",
                    UserRound { size: 13 }
                }
                // Folds with the crowded bar's <900 rung, like the member
                // pill (site_chrome's narrow ladder).
                span { class: "tw:hidden tw:text-[11.5px] tw:font-bold tw:@min-[900px]:inline", "Share" }
            },
            label: "Share".to_string(),
            title: format!("\"{name}\" — shared with you"),
            popup_class: POPUP_CLASS.to_string(),
            chrome_class: "ux-popover-chrome-neutral".to_string(),
            placement: PopoverPlacement::BottomEnd,
            layer_keeps_layout: true,
            initially_open,
            VisitorShareCard { name, url, access, on_copy, on_fork }
        }
    }
}

/// The card's body. Pure — stories mount it without a popover.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn VisitorShareCard(
    name: String,
    url: ShareUrl,
    access: Access,
    #[props(default)] on_copy: Option<EventHandler<()>>,
    #[props(default)] on_fork: Option<EventHandler<()>>,
) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    let grants = match access {
        Access::Edit => "anyone with the link can edit",
        _ => "anyone with the link can view",
    };
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2.5 tw:p-3.5",
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                strong { class: "tw:min-w-0 tw:truncate tw:text-[12.5px] tw:font-bold tw:text-strong-foreground",
                    "{name}"
                }
                if let Some(mut close) = close {
                    button {
                        class: inline_icon_button_class(InlineButtonTone::Neutral, false),
                        r#type: "button",
                        aria_label: "Close",
                        onclick: move |_| close.close(),
                        X { size: 13 }
                    }
                }
            }
            p { class: "tw:m-0 tw:text-[11.5px] tw:leading-normal tw:text-muted-foreground",
                span { class: "tw:font-semibold tw:text-foreground", "Shared with you" }
                " · {grants}."
            }
            ShareUrlHero { url, on_copy }
            button {
                class: FORK_CLASS,
                r#type: "button",
                onclick: move |_| {
                    if let Some(on_fork) = on_fork {
                        on_fork.call(());
                    }
                },
                GitBranch { size: 12 }
                span { class: "tw:text-[11px] tw:font-bold", "Fork — make it yours" }
            }
            p { class: "tw:m-0 tw:px-0.5 tw:text-[10px] tw:leading-snug tw:text-dim-foreground",
                "A fork is a new project with its own URL; yours to keep and share."
            }
        }
    }
}

/// Same chrome family as the owner pill — the slot must read as one
/// control whoever is looking.
const PILL_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-pill tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-3 tw:py-1.5 tw:text-status-neutral-foreground tw:transition-colors tw:hover:border-accent-border tw:hover:text-strong-foreground";
const PILL_OPEN_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-pill tw:border tw:border-accent-border tw:bg-accent-wash tw:px-3 tw:py-1.5 tw:text-strong-foreground";
/// The spike's §2-D card is narrower than the owner panel (300px).
/// Material-free (P4): the merged-outline popover already paints
/// background/border/shadow.
const POPUP_CLASS: &str =
    "tw:grid tw:w-[300px] tw:min-w-0 tw:rounded-md tw:border tw:text-sm tw:text-muted-foreground";
/// The one filled verb a visitor owns.
const FORK_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:justify-self-start tw:gap-1.5 tw:rounded-sm tw:border tw:border-accent-border tw:bg-accent tw:px-2.5 tw:py-1.5 tw:text-accent-foreground tw:transition-colors tw:hover:bg-accent-hover";

#[cfg(test)]
mod tests {
    use super::*;

    /// No preflight (crate README): every button names its own background.
    #[test]
    fn every_visitor_button_names_a_background() {
        for class in [PILL_CLASS, PILL_OPEN_CLASS, FORK_CLASS] {
            assert!(class.contains("tw:bg-"), "no background in `{class}`");
        }
    }
}
