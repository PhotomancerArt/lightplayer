//! The "Reconnecting…" curtain: while the editor's board rides out a link
//! stall or reset (core's `lens_reconnect`), or is away on a dropped link
//! and expected back (core's `lens_hold`), a quiet card floats over the
//! project page or Play, and a dim curtain behind it holds the page still.
//!
//! Over, never in: the card and the curtain are laid OVER the page, so
//! nothing below moves when they come and go (a strip pushed into the
//! flow reflowed the whole page on every blip). The page under them is
//! `inert` while the link is away, so nothing can be pressed or typed into
//! a board that is not there to hear it.
//!
//! Smooth, not jarring: both fade in after a short beat, so a blip that
//! recovers at once never shows at all, and fade out when the board is
//! back. The card keeps its last words through the fade-out rather than
//! blanking mid-fade. No buttons: there is nothing for the user to do but
//! wait a moment, and a board that does NOT come back closes the editor the
//! old way after core's grace, so the card never has to say "give up".
//!
//! The words are core's ([`UiLensReconnecting`]); this only lays them out.

use dioxus::prelude::*;
use dioxus_icons::lucide::RefreshCw;
use lpa_studio_core::UiLensReconnecting;

/// The page body, with the curtain over it while `reconnecting` is set.
/// `class` is the wrapper's own layout (it stands where the body stood).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ReconnectingCurtain(
    reconnecting: Option<UiLensReconnecting>,
    class: String,
    children: Element,
) -> Element {
    let on = reconnecting.is_some();
    // The last words shown, kept for the fade-out.
    let mut last = use_signal(|| reconnecting.clone());
    use_effect(use_reactive!(|reconnecting| {
        if reconnecting.is_some() {
            last.set(reconnecting);
        }
    }));
    let shown = reconnecting.clone().or_else(|| last());
    rsx! {
        div { class: "tw:relative {class}",
            // `inert` is not in Dioxus's boolean list, so `false` would
            // still set it: absent unless the link is away.
            div { class: "tw:contents", inert: on.then_some("true"), {children} }
            div {
                class: CURTAIN_CLASS,
                "data-reconnecting": "{on}",
                aria_hidden: (!on).then_some("true"),
                if let Some(reconnecting) = shown {
                    LinkReconnectingStrip { reconnecting }
                }
            }
        }
    }
}

/// The card itself. Renders whatever it is given; the curtain decides when.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LinkReconnectingStrip(reconnecting: UiLensReconnecting) -> Element {
    let UiLensReconnecting { headline, detail } = reconnecting;
    rsx! {
        div { class: CARD_CLASS, role: "status", aria_live: "polite",
            span { class: "ux-reconnect-spin tw:mt-0.5 tw:flex tw:flex-none tw:text-muted-foreground",
                RefreshCw { size: 14 }
            }
            span { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                strong { class: "tw:text-sm tw:font-semibold tw:text-strong-foreground", "{headline}" }
                span { class: "tw:text-xs tw:leading-snug tw:text-muted-foreground", "{detail}" }
            }
        }
    }
}

/// The curtain: the page's whole body, dimmed; its card rides at the top of
/// the visible area (sticky, so a scrolled Play page still shows it).
/// `ux-reconnect-curtain` carries the fade and the `data-reconnecting` switch.
const CURTAIN_CLASS: &str = "ux-reconnect-curtain tw:absolute tw:inset-0 tw:z-30 tw:flex tw:items-start tw:justify-center tw:bg-background/55 tw:px-4 tw:pt-6";
const CARD_CLASS: &str = "ux-reconnect-card tw:sticky tw:top-6 tw:flex tw:w-full tw:max-w-[26rem] tw:items-start tw:gap-x-3 tw:rounded-lg tw:border tw:border-border tw:bg-card tw:px-4 tw:py-3 tw:shadow-lg";
