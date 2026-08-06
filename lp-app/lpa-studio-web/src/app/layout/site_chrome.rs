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
//! The chrome is presentational. Nav tabs are plain hash links: `web_app`
//! owns the route signal and swaps only the body beneath this bar, so
//! moving between sections never unloads the actor, the runtime pool, or
//! any open sim/device session. Nothing here reloads the page.

use dioxus::prelude::*;
use lpa_studio_core::UiAction;

use crate::base::{IconMenuButton, IconMenuTone, LogoLockup, StudioIcon, StudioIconName};

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
    /// lens detach (see [`NavTab`]). Absent only under stories, which mount
    /// the chrome with no actor behind it.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
    children: Element,
) -> Element {
    rsx! {
        header { class: "tw:mb-[18px] tw:flex tw:min-h-[46px] tw:items-center tw:gap-4 tw:border-b tw:border-border-subtle tw:pb-2.5",
            // Brand lockup — inert by design (see module docs).
            LogoLockup {}
            nav { class: "tw:flex tw:items-center tw:gap-1",
                NavTab {
                    label: "Studio",
                    href: "/",
                    active: section == SiteSection::Studio,
                    on_action,
                }
                NavTab {
                    label: "Boards",
                    href: "/boards",
                    active: section == SiteSection::Boards,
                }
                NavTab {
                    label: "Docs",
                    href: "/docs",
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

/// The play-mode toggle, shown in the right-hand cluster whenever a lens
/// route is open (`docs/design/panel.md` P12). It is a plain hash link to
/// the play variant of the CURRENT route — the same session at a different
/// zoom — so the route listener sees no new document and nothing re-opens.
///
/// Deliberately not a nav tab: play is a mode of the Studio section, not a
/// section of its own, so it borrows the tab's inactive styling (in play it
/// takes the active treatment, which is what the mode being on looks like)
/// and sits with the version chip instead of in the tab row.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PlayToggle(href: String, playing: bool) -> Element {
    let class = if playing {
        NAV_TAB_ACTIVE
    } else {
        NAV_TAB_IDLE
    };
    let label = if playing { "Exit play" } else { "Play" };
    rsx! {
        a {
            class: "{class}",
            href: "{href}",
            title: if playing { "Back to the editor" } else { "Play mode: the panel, full screen" },
            "{label}"
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
    let class = if active { NAV_TAB_ACTIVE } else { NAV_TAB_IDLE };
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
/// tabs (they are project-free editors, not destinations). Each entry is a
/// card: what the tool is, not just its name, since these are surfaces
/// most people meet rarely. They open in a new tab so the studio session
/// behind them keeps running.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ToolsMenu() -> Element {
    rsx! {
        IconMenuButton {
            icon: StudioIconName::More,
            icon_size: 15,
            label: "Tools".to_string(),
            title: "Tools".to_string(),
            tone: IconMenuTone::Quiet,
            popup_class: TOOLS_POPUP_CLASS.to_string(),
            span { class: "tw:px-1.5 tw:pt-0.5 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                "Tools"
            }
            ToolCard {
                icon: StudioIconName::MapArrows,
                title: "Mapping editor",
                detail: "Lay out where each LED sits in 2D, so shaders land where you expect.",
                href: "/mapping",
            }
            ToolCard {
                icon: StudioIconName::NodeKind(crate::base::NodeKindIcon::Compute),
                title: "Board editor",
                detail: "Draw and edit the board diagrams behind the catalog.",
                href: "/boards/edit",
            }
        }
    }
}

/// One tool card: glyph, name, one line of what it is, and the
/// opens-in-a-new-tab marker.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ToolCard(
    icon: StudioIconName,
    title: &'static str,
    detail: &'static str,
    href: &'static str,
) -> Element {
    rsx! {
        a {
            class: TOOL_CARD_CLASS,
            href: "{href}",
            target: "_blank",
            rel: "noopener noreferrer",
            span { class: "tw:mt-0.5 tw:flex tw:h-7 tw:w-7 tw:flex-none tw:items-center tw:justify-center tw:rounded-sm tw:border tw:border-border tw:bg-card-muted tw:text-accent",
                StudioIcon { name: icon, size: 15 }
            }
            span { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                span { class: "tw:text-xs tw:font-bold tw:text-strong-foreground", "{title}" }
                span { class: "tw:text-[11px] tw:leading-snug tw:text-dim-foreground", "{detail}" }
            }
            span { class: "tw:mt-0.5 tw:flex-none tw:self-start tw:text-subtle-foreground",
                StudioIcon { name: StudioIconName::ExternalLink, size: 13 }
            }
        }
    }
}

/// Current-destination treatment: heading color plus the accent underline.
const NAV_TAB_ACTIVE: &str = "tw:relative tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-bold tw:text-heading tw:no-underline tw:after:absolute tw:after:inset-x-2.5 tw:after:-bottom-[11px] tw:after:h-0.5 tw:after:rounded-full tw:after:bg-accent tw:after:content-['']";
/// Idle treatment: subtle text that brightens on hover.
const NAV_TAB_IDLE: &str = "tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-bold tw:text-subtle-foreground tw:no-underline tw:transition-colors tw:hover:bg-background-wash tw:hover:text-strong-foreground";

const TOOLS_POPUP_CLASS: &str = "tw:grid tw:w-[288px] tw:gap-1 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-1.5 tw:text-sm tw:text-muted-foreground tw:shadow-lg";
/// Rows are cards, not text links: fixed three-column grid so the title and
/// detail wrap inside their own column instead of around the glyphs.
const TOOL_CARD_CLASS: &str = "tw:grid tw:grid-cols-[auto_minmax(0,1fr)_auto] tw:items-start tw:gap-2.5 tw:rounded-sm tw:border tw:border-transparent tw:px-2 tw:py-2 tw:no-underline tw:transition-colors tw:hover:border-border tw:hover:bg-card-raised";
