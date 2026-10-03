//! The "Reconnecting…" strip (plan D13): full-width over the project page
//! (or Play) while the editor's board rides out a link stall or reset, or
//! is away on a dropped link and expected back (core's `lens_hold`).
//!
//! Calm on purpose: a blip the link recovers from on its own is not an
//! error, so the strip wears the neutral tint, not the warning one, and
//! offers nothing to press — there is nothing for the user to do but wait a
//! moment. The page under it stays exactly as it was; the strip goes the
//! moment the board is heard again. A board that does NOT come back within
//! the grace closes the editor the old way (core's `lens_reconnect` and
//! `lens_hold`), so the strip never has to say "give up".
//!
//! The words are core's ([`UiLensReconnecting`]); this only lays them out,
//! in the visitor strip's grammar (`share::visitor_banner`).

use dioxus::prelude::*;
use dioxus_icons::lucide::RefreshCw;
use lpa_studio_core::UiLensReconnecting;

/// The strip. Renders whatever it is given; the shell decides when.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LinkReconnectingStrip(reconnecting: UiLensReconnecting) -> Element {
    let UiLensReconnecting { headline, detail } = reconnecting;
    rsx! {
        div { class: STRIP_CLASS, role: "status", aria_live: "polite",
            span { class: "ux-reconnect-spin tw:flex tw:flex-none tw:text-status-neutral-foreground",
                RefreshCw { size: 14 }
            }
            span { class: TEXT_CLASS,
                strong { class: "tw:font-bold tw:text-strong-foreground", "{headline}" }
                " {detail}"
            }
        }
    }
}

const STRIP_CLASS: &str = "tw:mb-3 tw:flex tw:min-w-0 tw:flex-none tw:items-center tw:gap-x-3 tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-4 tw:py-2";
const TEXT_CLASS: &str = "tw:min-w-0 tw:text-xs tw:leading-snug tw:text-muted-foreground";
