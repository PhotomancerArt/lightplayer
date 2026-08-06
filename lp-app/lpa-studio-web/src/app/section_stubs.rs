//! Placeholder bodies for the Home and Explore sections (P07).
//!
//! Deliberately one screen each: the routes and nav exist now so the
//! chrome reorg can land, while the real content comes later — Home in
//! M3 (landing/marketing), Explore once modpack scaffolding gives it
//! real material (vision D6/D14).

use dioxus::prelude::*;

use crate::base::LogoStacked;

/// The `#/home` landing stub: the brand and the two places to go.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HomePage() -> Element {
    rsx! {
        section { class: "tw:flex tw:min-h-[60vh] tw:flex-col tw:items-center tw:justify-center tw:gap-6 tw:text-center",
            LogoStacked { size: 96 }
            p { class: "tw:max-w-md tw:text-sm tw:text-muted-foreground",
                "Shader-driven light, from first LED to full installation."
            }
            nav { class: "tw:flex tw:items-center tw:gap-3",
                a { class: STUB_LINK, href: "#/", "Devices" }
                a { class: STUB_LINK, href: "#/projects", "Projects" }
            }
        }
    }
}

/// The `#/explore` stub: names the destination, promises nothing yet.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ExplorePage() -> Element {
    rsx! {
        section { class: "tw:flex tw:min-h-[40vh] tw:flex-col tw:items-center tw:justify-center tw:gap-3 tw:text-center",
            h1 { class: "tw:text-lg tw:font-bold tw:text-heading", "Explore" }
            p { class: "tw:max-w-md tw:text-sm tw:text-muted-foreground",
                "Browsable patterns and example projects will land here."
            }
        }
    }
}

const STUB_LINK: &str = "tw:rounded-sm tw:border tw:border-border tw:px-3.5 tw:py-1.5 tw:text-xs tw:font-bold tw:text-strong-foreground tw:no-underline tw:transition-colors tw:hover:border-accent-border tw:hover:text-accent";
