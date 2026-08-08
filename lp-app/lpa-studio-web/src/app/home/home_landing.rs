//! The Home landing page (`/`, vision D14 / spike §5) — **the M3
//! stub**: lockup, one-line tagline, three dive-in cards. Deliberately
//! no marketing depth and no demo strip; the real landing pass is M3's.
//! Reached through the logo only — there is no Home nav tab (D11).

use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, UiAction};

use crate::app::home::package_card::home_action;
use crate::base::{LogoStacked, StudioIcon, StudioIconName};
use crate::cloud::SharedOpenState;

/// The landing stub: the brand, what this is, and three ways in.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HomePage(#[props(default)] on_action: Option<EventHandler<UiAction>>) -> Element {
    // A `/p/` link that landed here (P6): one quiet line about where it
    // stands — opening, or the calm refusal that never says which of
    // restricted/archived/absent it was. Stories provide no context and
    // render nothing.
    let shared_open = try_consume_context::<Signal<SharedOpenState>>();
    let shared_line = shared_open.and_then(|state| {
        let state = state();
        state.line().map(|line| (line, state.is_refusal()))
    });
    rsx! {
        section { class: "tw:flex tw:min-h-[60vh] tw:flex-col tw:items-center tw:justify-center tw:gap-8 tw:text-center",
            if let Some((line, refusal)) = shared_line {
                p {
                    class: if refusal { "{SHARED_LINE_CLASS} tw:border-status-warning-border tw:bg-status-warning-bg tw:text-status-warning-foreground" } else { "{SHARED_LINE_CLASS} tw:border-border tw:bg-card tw:text-muted-foreground" },
                    role: "status",
                    "{line}"
                }
            }
            LogoStacked { size: 96 }
            p { class: "tw:m-0 tw:max-w-md tw:text-sm tw:text-muted-foreground",
                "Friendly shaders, everywhere"
            }
            nav { class: "tw:grid tw:w-[min(680px,100%)] tw:grid-cols-3 tw:gap-3 tw:max-[640px]:grid-cols-1",
                DiveInCard {
                    icon: StudioIconName::Usb,
                    title: "Devices",
                    detail: "Your boards and the simulator — set up, connect, play.",
                    href: "/devices",
                }
                // The sim path: land on Devices WITH the wizard already
                // walking the simulate-a-device flow (same op as the
                // Devices page's entry card).
                DiveInCard {
                    icon: StudioIconName::Simulator,
                    title: "Try the simulator",
                    detail: "Set up the simulator as a board — no hardware needed.",
                    href: "/devices",
                    on_press: on_action.map(|on_action| {
                        EventHandler::new(move |()| {
                            on_action.call(home_action(HomeOp::StartSetup { sim: true }));
                        })
                    }),
                }
                DiveInCard {
                    icon: StudioIconName::Play,
                    title: "Explore",
                    detail: "Example projects to open, run, and make yours.",
                    href: "/explore",
                }
            }
        }
    }
}

/// One dive-in card: glyph, name, a line of what it is. A plain hash
/// link; `on_press` rides the click for entries that also start a flow
/// (the sim path).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DiveInCard(
    icon: StudioIconName,
    title: &'static str,
    detail: &'static str,
    href: &'static str,
    #[props(default)] on_press: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        a {
            class: DIVE_IN_CARD_CLASS,
            href: "{href}",
            onclick: move |_| {
                if let Some(on_press) = on_press {
                    on_press.call(());
                }
            },
            span { class: "tw:flex tw:h-9 tw:w-9 tw:items-center tw:justify-center tw:rounded-sm tw:border tw:border-border tw:bg-card-muted tw:text-accent",
                StudioIcon { name: icon, size: 18 }
            }
            span { class: "tw:text-sm tw:font-bold tw:text-strong-foreground", "{title}" }
            span { class: "tw:text-xs tw:leading-snug tw:text-muted-foreground", "{detail}" }
        }
    }
}

const DIVE_IN_CARD_CLASS: &str = "tw:grid tw:justify-items-center tw:gap-2 tw:rounded-md tw:border tw:border-border tw:bg-card tw:px-4 tw:py-5 tw:no-underline tw:transition-colors tw:hover:border-accent-border tw:hover:bg-card-raised";

/// The one quiet line for a `/p/` link's fate (tone classes appended).
const SHARED_LINE_CLASS: &str =
    "tw:m-0 tw:max-w-md tw:rounded-md tw:border tw:px-4 tw:py-2.5 tw:text-xs tw:leading-snug";
