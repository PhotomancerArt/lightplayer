//! [`SiteChrome`]: the one top bar shared by every section of the app.
//!
//! Studio (the gallery + editor), Boards, and Docs are sections of a single
//! cohesive app; this bar is their common navigation. Two deliberate
//! product decisions live here (gate-judged, spike PR #269 —
//! `spikes/top-bar/index.html` is the design record):
//!
//! - **The brand lockup is inert.** The logo is reserved for a future
//!   landing/marketing page; navigating home is the Studio tab's job, so
//!   the lockup renders as a plain span, not a link.
//! - **The editors are tools, not sections.** The mapping editor and board
//!   editor stay outside the tab row, reachable from the overflow menu.
//!
//! The chrome is presentational: sections pass `active` and (in the studio
//! app) an `on_action` hook; standalone pages omit `on_action` and get
//! plain hash navigation.

use dioxus::prelude::*;
use lpa_studio_core::UiAction;

use crate::base::{IconMenuButton, IconMenuTone, LogoMark, StudioIconName};

/// Which nav tab renders as the current section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteSection {
    Studio,
    Boards,
    Docs,
}

/// The shared top bar. `children` render at the start of the right-hand
/// cluster (the studio shell passes its version chip and settings trigger
/// through here).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SiteChrome(
    section: SiteSection,
    /// Studio-app action hook. Present: the Studio tab ALSO dispatches the
    /// lens detach (see [`studio_tab_onclick`]). Absent (standalone pages):
    /// tabs are plain hash links and the page reload machinery takes over.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
    children: Element,
) -> Element {
    rsx! {
        header { class: "tw:mb-[18px] tw:flex tw:min-h-[46px] tw:items-center tw:gap-4 tw:border-b tw:border-border-subtle tw:pb-2.5",
            // Brand lockup — inert by design (see module docs).
            span {
                class: "tw:flex tw:flex-none tw:cursor-default tw:items-center tw:gap-2 tw:text-accent",
                title: "LightPlayer",
                LogoMark { size: 22 }
                span { class: "tw:text-[12.5px] tw:font-bold tw:text-strong-foreground tw:max-[520px]:hidden",
                    "LightPlayer"
                }
            }
            nav { class: "tw:flex tw:items-center tw:gap-1",
                NavTab {
                    label: "Studio",
                    href: "#/",
                    active: section == SiteSection::Studio,
                    on_action,
                }
                NavTab {
                    label: "Boards",
                    href: "#/boards",
                    active: section == SiteSection::Boards,
                }
                NavTab {
                    label: "Docs",
                    href: "#/docs",
                    active: section == SiteSection::Docs,
                }
            }
            div { class: "tw:ml-auto tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                {children}
                ToolsMenu {}
            }
        }
    }
}

/// One nav tab. Active: heading color + accent underline; inactive: subtle
/// text that brightens on hover.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NavTab(
    label: &'static str,
    href: &'static str,
    active: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let class = if active {
        "tw:relative tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-bold tw:text-heading tw:no-underline tw:after:absolute tw:after:inset-x-2.5 tw:after:-bottom-[11px] tw:after:h-0.5 tw:after:rounded-full tw:after:bg-accent tw:after:content-['']"
    } else {
        "tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-bold tw:text-subtle-foreground tw:no-underline tw:transition-colors tw:hover:bg-background-wash tw:hover:text-strong-foreground"
    };
    rsx! {
        a {
            class: "{class}",
            href: "{href}",
            aria_current: if active { "page" } else { "false" },
            onclick: move |_| {
                // The Studio tab is the way home. Navigating to `#/` fires
                // `hashchange`, which the route listener turns into the lens
                // detach (runtime-pool P3: the editor closes, sessions keep
                // running) — the same path as the browser back button. The
                // click ALSO dispatches the detach directly: the D29 device
                // editor lives at `#/` (no URL until M5), so a Studio click
                // there changes no hash and the listener never fires — the
                // direct dispatch is its way home. Detaching an
                // already-detached lens is a no-op, so the doubled dispatch
                // on project routes is harmless.
                if let Some(on_action) = on_action {
                    on_action.call(UiAction::from_op(
                        lpa_studio_core::ProjectController::NODE_ID,
                        lpa_studio_core::ProjectOp::DetachLens,
                    ));
                }
            },
            "{label}"
        }
    }
}

/// Overflow menu for the standalone authoring tools — deliberately not nav
/// tabs (they are project-free editors, not destinations).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ToolsMenu() -> Element {
    rsx! {
        IconMenuButton {
            icon: StudioIconName::More,
            icon_size: 15,
            label: "Tools".to_string(),
            title: "Tools".to_string(),
            tone: IconMenuTone::Neutral,
            popup_class: TOOLS_POPUP_CLASS.to_string(),
            ToolsMenuLink { label: "Mapping editor", href: "#/mapping" }
            ToolsMenuLink { label: "Board editor", href: "#/boards/edit" }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ToolsMenuLink(label: &'static str, href: &'static str) -> Element {
    rsx! {
        a {
            class: "tw:rounded-sm tw:px-2 tw:py-1.5 tw:text-xs tw:font-bold tw:text-muted-foreground tw:no-underline tw:hover:bg-card-raised tw:hover:text-strong-foreground",
            href: "{href}",
            "{label}"
        }
    }
}

const TOOLS_POPUP_CLASS: &str = "tw:grid tw:w-[200px] tw:gap-0.5 tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card tw:p-1.5 tw:text-sm tw:text-muted-foreground tw:shadow-lg";
