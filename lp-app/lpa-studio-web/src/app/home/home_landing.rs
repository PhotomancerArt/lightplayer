//! The Home landing page (`/`, vision D14 / spike §5): the brand hero, a
//! one-line tagline, three dive-in cards. Still no marketing depth — but
//! the stub's static lockup is gone: [`BrandHero`] makes the mark's
//! triangle a window onto a live engine shader, so the landing demonstrates
//! the product instead of describing it. Reached through the logo only —
//! there is no Home nav tab (D11).
//!
//! Direction: the hero is the seed of the fixture-hero (a module panel
//! under it, touch/sound driving it) — see the
//! `2026-08-24-1100-logo-triangle-chip` plan, D1, and `brand_hero.rs`.

use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, UiAction};

use crate::app::home::brand_hero::BrandHero;
use crate::app::home::package_card::home_action;
use crate::base::{StudioIcon, StudioIconName};
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
            BrandHero {}
            // The slogan reads as a slogan — strong ink, a hair larger than
            // body text — with the edit-shader door as a quiet pencil beside
            // it (polish round: a text button here fought the tagline).
            div { class: "tw:flex tw:items-center tw:justify-center tw:gap-1.5",
                p { class: "tw:m-0 tw:max-w-md tw:text-[15px] tw:font-medium tw:text-strong-foreground",
                    "Friendly shaders, everywhere"
                }
                EditShaderPencil { on_action }
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

/// The door from the hero into the editor: a pencil beside the slogan,
/// opening the hero's example via the Explore-card path. Inert with a
/// says-why tooltip when there is no dispatcher (stories, host builds).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditShaderPencil(#[props(default)] on_action: Option<EventHandler<UiAction>>) -> Element {
    let live = on_action.is_some();
    let title = if live {
        "Edit this shader — opens it in the editor and keeps it in your projects"
    } else {
        "Only available in the running app"
    };
    rsx! {
        button {
            class: edit_pencil_class(live),
            r#type: "button",
            disabled: !live,
            title: "{title}",
            "aria-label": "Edit this shader",
            onclick: move |_| {
                if let Some(on_action) = on_action {
                    on_action
                        .call(
                            home_action(HomeOp::OpenExample {
                                id: crate::app::home::brand_hero::HERO_EXAMPLE.to_string(),
                            }),
                        );
                }
            },
            StudioIcon { name: StudioIconName::Edited, size: 14 }
        }
    }
}

/// Quiet chrome: the slogan is the sentence, the pencil is a footnote
/// that brightens on hover.
fn edit_pencil_class(live: bool) -> &'static str {
    if live {
        "tw:flex tw:h-6 tw:w-6 tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:text-muted-foreground tw:transition-colors tw:hover:border-border tw:hover:text-strong-foreground"
    } else {
        "tw:flex tw:h-6 tw:w-6 tw:cursor-not-allowed tw:items-center tw:justify-center tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:text-dim-foreground"
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
            span { class: "tw:flex tw:h-9 tw:w-9 tw:items-center tw:justify-center tw:rounded-sm tw:border tw:border-border tw:bg-card-muted tw:text-heading",
                StudioIcon { name: icon, size: 18 }
            }
            span { class: "tw:text-sm tw:font-bold tw:text-strong-foreground", "{title}" }
            span { class: "tw:text-xs tw:leading-snug tw:text-muted-foreground", "{detail}" }
        }
    }
}

const DIVE_IN_CARD_CLASS: &str = "tw:grid tw:justify-items-center tw:gap-2 tw:rounded-md tw:border tw:border-border tw:bg-card tw:px-4 tw:py-5 tw:no-underline tw:transition-colors tw:hover:bg-card-raised ux-ir-ring ux-card-lift ux-focus-ring";

/// The one quiet line for a `/p/` link's fate (tone classes appended).
const SHARED_LINE_CLASS: &str =
    "tw:m-0 tw:max-w-md tw:rounded-md tw:border tw:px-4 tw:py-2.5 tw:text-xs tw:leading-snug";
