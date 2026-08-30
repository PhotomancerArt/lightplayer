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
use lpa_studio_core::{HomeOp, UiAction, UiHomeView};

use crate::app::home::brand_hero::BrandHero;
use crate::app::home::example_card::{ExampleCard, embedded_example_cards};
use crate::app::home::gallery_preview::HoveredCard;
use crate::app::home::package_card::home_action;
use crate::base::{StudioIcon, StudioIconName};
use crate::cloud::SharedOpenState;

/// The landing stub: the brand, what this is, three ways in, and the
/// example grid.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HomePage(
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The gallery slice, for the row's opening/busy card states. `None`
    /// in stories and host mounts — the examples themselves are
    /// compiled-in content and render regardless.
    #[props(default)]
    home: Option<UiHomeView>,
) -> Element {
    // A `/p/` link that landed here (P6): one quiet line about where it
    // stands — opening, or the calm refusal that never says which of
    // restricted/archived/absent it was. Stories provide no context and
    // render nothing.
    let shared_open = try_consume_context::<Signal<SharedOpenState>>();
    let shared_line = shared_open.and_then(|state| {
        let state = state();
        state.line().map(|line| (line, state.is_refusal()))
    });
    // Hover-to-play for the example grid, page-scoped like Explore's: one
    // signal names one hovered card, so the grid holds at most one live
    // preview lease at a time.
    use_context_provider(|| HoveredCard(Signal::new(None)));
    // ALL the examples, right here (G1 ruling 2026-08-29: "scrolling is
    // better than navigating" — curation and better organization are
    // future work). Same cards as Explore, hero's example included (its
    // card is the door to editing what the hero shows).
    let examples = embedded_example_cards();
    let opening = home.as_ref().and_then(|home| home.opening.clone());
    let busy = opening.is_some();
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
            // body text — with the door into the editor STACKED under it, not
            // beside it: a labelled call to action next to the tagline fought
            // it for the same line (polish round), and stacking also keeps the
            // pill off the tagline's width at narrow viewports.
            div { class: "tw:flex tw:flex-col tw:items-center tw:gap-3",
                p { class: "tw:m-0 tw:max-w-md tw:text-[15px] tw:font-medium tw:text-strong-foreground",
                    "Friendly shaders, everywhere"
                }
                EditArtworkPill { on_action }
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
            // The example grid (D5, widened at G1 to ALL examples): real,
            // running content one click deep — viewing is stateless (D2),
            // and the card copy's "becomes yours on first save" is
            // literally the model now. Rendered dispatcher-less too
            // (stories, host mounts): the cards are compiled-in content
            // and clicks just no-op there.
            section { class: "tw:grid tw:w-[min(1080px,100%)] tw:gap-3 tw:text-left",
                header { class: "tw:flex tw:items-baseline tw:justify-between",
                    h2 { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground",
                        "Examples"
                    }
                    a {
                        class: "tw:text-xs tw:font-semibold tw:text-accent tw:no-underline tw:hover:underline",
                        href: "/explore",
                        "Explore all →"
                    }
                }
                div { class: crate::app::home::card_grid_class(),
                    for card in examples {
                        ExampleCard {
                            key: "{card.id}",
                            opening: opening.as_deref() == Some(card.id.as_str()),
                            busy,
                            card,
                            on_action: on_action.unwrap_or_else(|| EventHandler::new(|_| {})),
                        }
                    }
                }
            }
        }
    }
}

/// The door from the hero into the editor: the "Edit the logo" pill,
/// opening the hero's example ([`crate::app::home::brand_hero::HERO_EXAMPLE`],
/// the brand's own Logo Sign) via the Explore-card path. Inert with a
/// says-why tooltip when there is no dispatcher (stories, host builds).
///
/// A labelled pill, not the earlier bare pencil: the hero's whole claim is
/// that the artwork is *editable*, and a 24px glyph beside the tagline
/// never said so out loud (G1 ruling, 2026-08-26). Deliberately a QUIET
/// secondary — neutral border and surface, accent only on hover — after
/// the solid accent fill proved the loudest thing on the page (G2 ruling,
/// 2026-08-28; a broader design-language pass is planned, so this stays
/// inside today's idiom rather than inventing ahead of it).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditArtworkPill(#[props(default)] on_action: Option<EventHandler<UiAction>>) -> Element {
    let live = on_action.is_some();
    let title = if live {
        "Edit the logo — opens it in the editor and keeps it in your projects"
    } else {
        "Only available in the running app"
    };
    rsx! {
        button {
            class: edit_artwork_pill_class(live),
            r#type: "button",
            disabled: !live,
            title: "{title}",
            "aria-label": "Edit the logo",
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
            StudioIcon { name: StudioIconName::Edited, size: 15 }
            span { "Edit the logo" }
        }
    }
}

/// The quiet secondary pill: neutral border on the card surface, strong
/// text, and the accent reserved for hover — the invite is the LABEL, the
/// hero above it already carries the color. Same footprint as the docs'
/// `open-in-studio` button so the product's one "open this example"
/// gesture keeps one size everywhere. `max-w-full` + `whitespace-nowrap`
/// keep it inside the narrowest viewport the site chrome supports without
/// the label breaking mid-word.
fn edit_artwork_pill_class(live: bool) -> &'static str {
    if live {
        "tw:inline-flex tw:max-w-full tw:cursor-pointer tw:items-center tw:gap-2 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-border tw:bg-card tw:px-4 tw:py-2 tw:text-sm tw:font-semibold tw:leading-none tw:text-strong-foreground tw:transition-colors tw:hover:border-accent-border tw:hover:text-accent"
    } else {
        "tw:inline-flex tw:max-w-full tw:cursor-not-allowed tw:items-center tw:gap-2 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-border-muted tw:bg-card-muted tw:px-4 tw:py-2 tw:text-sm tw:font-semibold tw:leading-none tw:text-dim-foreground"
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
