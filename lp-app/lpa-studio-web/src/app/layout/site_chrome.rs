//! [`SiteChrome`]: the one top bar shared by every section of the app.
//!
//! Home, Devices, Projects, Explore, Boards, and Docs are sections of a
//! single cohesive app; this bar is their common navigation — "chrome C"
//! (vision D1/D11, gate-judged spike `spikes/gallery-rework/index.html`;
//! the original three-tab bar was spike PR #269, `spikes/top-bar/`):
//!
//! - **Split weights.** The primary family (Devices, Projects — your
//!   things) sits by the brand at full weight; the secondary family
//!   (Explore, Boards, Docs — the world's things) rides the right
//!   cluster, lighter, with no divider between the families.
//! - **The brand lockup is the way to Home.** The logo links to `#/home`
//!   — there is deliberately no Home tab, and no Studio tab either (the
//!   sections replaced it).
//! - **One overflow menu** (G3 ruling, 2026-08-05: a row of separate
//!   menus read as clutter — merge them ALL). The single ⋯ at the bar's
//!   end always holds the tools and the full session list, and grows the
//!   secondary sections at narrow widths when the inline tabs collapse
//!   (the bar is a container; the cut is where three secondary tabs stop
//!   fitting, not a viewport magic number). The brand word yields at
//!   narrow too — the mark stays.
//! - **Running sessions are places** (vision D15/D16): the session strip
//!   docks one chip per live runtime session behind a hairline divider
//!   after the primary family (wide bars — narrow bars list sessions in
//!   the ⋯ menu instead). Chips are wayfinding only — name, glyph,
//!   status dot — never controls or thumbnails (D43). The active chip is
//!   the editor's representation in the nav; no nav tab detaches the
//!   lens (navigation does, through the route listener, same as the back
//!   button).
//! - **The editors are tools, not sections.** The mapping editor and
//!   board editor stay outside the tab row, in the ⋯ menu's Tools group.
//!
//! The chrome is presentational. Nav tabs are plain hash links: `web_app`
//! owns the route signal and swaps only the body beneath this bar, so
//! moving between sections never unloads the actor, the runtime pool, or
//! any open sim/device session. Nothing here reloads the page.

use dioxus::prelude::*;
use lpa_studio_core::{UiChromeSession, UiChromeSessionStatus, UiChromeSessionTarget};

use crate::base::{
    IconMenuButton, IconMenuTone, LogoLockup, PopoverCloseHandle, StudioIcon, StudioIconName,
};
use crate::router::StudioRoute;

/// Which nav tab renders as the current section. Home has no tab — the
/// LOGO is its affordance, and at Home the logo wears the you're-here
/// underline in a tab's stead (G3 feedback). Session is the editor lens
/// fronted: no tab lights, because the active session CHIP is the
/// current-place marker there (D15 — the chip is the editor's
/// representation in the nav).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteSection {
    Home,
    Devices,
    Projects,
    Explore,
    Boards,
    Docs,
    Session,
}

/// The shared top bar. `children` render at the start of the right-hand
/// cluster (the studio shell passes its version chip and settings trigger
/// through here).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SiteChrome(
    section: SiteSection,
    /// The session strip's live sessions (D15); empty renders no strip
    /// and no divider. Stories and chrome-only mounts default to none.
    #[props(default)]
    sessions: Vec<UiChromeSession>,
    /// A lens route is the current route — the lensed chip reads *here*
    /// (accent); with it false the lensed chip reads *open* (washed).
    #[props(default = false)]
    on_editor: bool,
    /// Stories only: mount the narrow ⋯ menu open (capture can't hover).
    #[props(default = false)]
    overflow_menu_open: bool,
    children: Element,
) -> Element {
    rsx! {
        // `tw:@container`: the collapse below responds to the BAR's own
        // width, not the viewport, so an embedded/narrow mount behaves.
        header { class: "tw:@container tw:mb-[18px] tw:flex tw:min-h-[46px] tw:items-center tw:gap-4 tw:border-b tw:border-border-subtle tw:pb-2.5",
            // Brand lockup — the way to Home (see module docs). At Home it
            // wears the tabs' you're-here underline (G3 feedback: the logo
            // IS Home's tab, so it marks the place like one).
            span {
                class: if section == SiteSection::Home { LOGO_HOME_ACTIVE_WRAP } else { "tw:flex tw:flex-none" },
                LogoLockup { href: "#/home".to_string() }
            }
            // Primary family: your things, by the brand, full weight.
            nav { class: "tw:flex tw:items-center tw:gap-1",
                NavTab {
                    label: "Devices",
                    href: "#/",
                    active: section == SiteSection::Devices,
                }
                NavTab {
                    label: "Projects",
                    href: "#/projects",
                    active: section == SiteSection::Projects,
                }
            }
            if !sessions.is_empty() {
                // Hairline divider + the strip (concept A dock) — wide
                // bars only; narrow bars carry sessions in the ⋯ menu.
                div { class: "tw:hidden tw:min-w-0 tw:items-center tw:gap-4 tw:@min-[680px]:flex",
                    span { class: "tw:h-5 tw:w-px tw:flex-none tw:self-center tw:bg-border-subtle" }
                    SessionStrip { sessions: sessions.clone(), on_editor }
                }
            }
            div { class: "tw:ml-auto tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                // Secondary family: lighter, right cluster, no divider —
                // inline while three tabs fit the bar; in the ⋯ below
                // when they don't.
                nav { class: "tw:hidden tw:items-center tw:gap-1 tw:@min-[680px]:flex",
                    NavTab {
                        label: "Explore",
                        href: "#/explore",
                        active: section == SiteSection::Explore,
                        secondary: true,
                    }
                    NavTab {
                        label: "Boards",
                        href: "#/boards",
                        active: section == SiteSection::Boards,
                        secondary: true,
                    }
                    NavTab {
                        label: "Docs",
                        href: "#/docs",
                        active: section == SiteSection::Docs,
                    secondary: true,
                    }
                }
                {children}
                // THE overflow menu — one ⋯ at every width (G3 ruling).
                // Two mounts because a top-layer popup cannot answer the
                // header's container query: the wide form (no section
                // rows) and the narrow form (sections included) swap by
                // the same breakpoint as the tabs they mirror.
                div { class: "tw:hidden tw:@min-[680px]:block",
                    ChromeOverflowMenu {
                        section,
                        sessions: sessions.clone(),
                        on_editor,
                        include_sections: false,
                    }
                }
                div { class: "tw:@min-[680px]:hidden",
                    ChromeOverflowMenu {
                        section,
                        sessions,
                        on_editor,
                        include_sections: true,
                        initially_open: overflow_menu_open,
                    }
                }
            }
        }
    }
}

/// THE ⋯ menu (G3 ruling, 2026-08-05): sections (narrow only — they are
/// inline tabs while they fit), the full session list, and the tools, in
/// one place. Groups wear mini-headers; rows keep their own grammars —
/// section/session rows navigate this tab and close the menu, tool cards
/// open a new one.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChromeOverflowMenu(
    section: SiteSection,
    sessions: Vec<UiChromeSession>,
    on_editor: bool,
    include_sections: bool,
    #[props(default = false)] initially_open: bool,
) -> Element {
    rsx! {
        IconMenuButton {
            icon: StudioIconName::More,
            icon_size: 15,
            label: "More".to_string(),
            title: "Sections, sessions, and tools".to_string(),
            tone: IconMenuTone::Quiet,
            initially_open,
            popup_class: OVERFLOW_POPUP_CLASS.to_string(),
            // One explicit grid wrapper: the popover primitive nests
            // children in its own content div, so the panel's classes
            // never reach them — inline rows would flow sideways.
            div { class: "tw:grid tw:gap-1",
                if include_sections {
                    span { class: GROUP_HEADER_CLASS, "Sections" }
                    NavMenuItem { label: "Explore", href: "#/explore", active: section == SiteSection::Explore }
                    NavMenuItem { label: "Boards", href: "#/boards", active: section == SiteSection::Boards }
                    NavMenuItem { label: "Docs", href: "#/docs", active: section == SiteSection::Docs }
                }
                if !sessions.is_empty() {
                    span { class: GROUP_HEADER_CLASS, "Sessions" }
                    for session in sessions.iter() {
                        SessionMenuRow { key: "{session.key}", session: session.clone(), on_editor }
                    }
                }
                span { class: GROUP_HEADER_CLASS, "Tools" }
                ToolCard {
                    icon: StudioIconName::MapArrows,
                    title: "Mapping editor",
                    detail: "Lay out where each LED sits in 2D, so shaders land where you expect.",
                    href: "#/mapping",
                }
                ToolCard {
                    icon: StudioIconName::NodeKind(crate::base::NodeKindIcon::Compute),
                    title: "Board editor",
                    detail: "Draw and edit the board diagrams behind the catalog.",
                    href: "#/boards/edit",
                }
            }
        }
    }
}

/// One section row of the ⋯ menu: a plain hash link that closes the menu
/// as it navigates (sections swap in place — without the close the
/// popover would linger over the new body).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NavMenuItem(label: &'static str, href: &'static str, active: bool) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    let class = if active {
        NAV_MENU_ITEM_ACTIVE
    } else {
        NAV_MENU_ITEM_IDLE
    };
    rsx! {
        a {
            class: "{class}",
            href: "{href}",
            aria_current: if active { "page" } else { "false" },
            onclick: move |_| {
                if let Some(mut close) = close {
                    close.close();
                }
            },
            "{label}"
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
/// text that brightens on hover. `secondary` is the lighter family
/// treatment (reduced weight, dimmer at rest, full strength on
/// hover/active — the spike's `.secondary`).
///
/// Tabs are PLAIN links on purpose (P12): no tab dispatches a lens
/// detach anymore — navigation to a gallery route detaches through the
/// route listener, the same path as the back button, and returning to
/// the editor is the active session chip's job. (The old Studio-tab
/// direct dispatch existed for the URL-less D29 device editor at `#/`;
/// identity-at-probe made device lenses addressable, closing that gap.)
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NavTab(
    label: &'static str,
    href: &'static str,
    active: bool,
    #[props(default = false)] secondary: bool,
) -> Element {
    let class = match (secondary, active) {
        (false, true) => NAV_TAB_ACTIVE,
        (false, false) => NAV_TAB_IDLE,
        (true, true) => NAV_TAB_SECONDARY_ACTIVE,
        (true, false) => NAV_TAB_SECONDARY_IDLE,
    };
    rsx! {
        a {
            class: "{class}",
            href: "{href}",
            aria_current: if active { "page" } else { "false" },
            "{label}"
        }
    }
}

/// The session strip (D15/D16): squared tab-chips for live sessions on
/// wide bars — up to four inline (two on middling bars); the rest, and
/// every session at narrow widths, live in the ⋯ menu's Sessions group.
/// All width variants render and container queries pick, so the strip
/// needs no measurement code.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SessionStrip(sessions: Vec<UiChromeSession>, on_editor: bool) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
            for (index , session) in sessions.iter().take(4).enumerate() {
                span {
                    key: "{session.key}",
                    // Chips 0-1 appear whenever the strip does; 2-3 need
                    // a wide bar. Everything else is ⋯-menu material.
                    class: if index < 2 { "tw:flex tw:min-w-0" } else { "tw:hidden tw:min-w-0 tw:@min-[900px]:flex" },
                    SessionChip { session: session.clone(), on_editor }
                }
            }
        }
    }
}

/// One session chip: transport glyph, status dot, ellipsized name.
/// States: *here* (accent — lensed and the editor is fronted), *open*
/// (washed — lensed, another section fronted), idle. A session without
/// an honest route renders inert (no fake URLs — the same rule the URL
/// bar follows).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SessionChip(session: UiChromeSession, on_editor: bool) -> Element {
    let class = chip_state_class(&session, on_editor);
    let body = rsx! {
        SessionGlyph { sim: session.sim }
        SessionDot { status: session.status }
        span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap",
            "{session.name}"
        }
    };
    match session_href(&session) {
        Some(href) => rsx! {
            a {
                class: "{class}",
                href: "{href}",
                aria_current: if session.lensed && on_editor { "page" } else { "false" },
                title: "{session.name}",
                {body}
            }
        },
        None => rsx! {
            span { class: "{class}", title: "{session.name}", {body} }
        },
    }
}

/// The chip's route (D37 keys): `#/sim/<project-uid>` for the sim,
/// `#/device/<dev-uid>` for hardware; `None` while no honest address
/// exists.
fn session_href(session: &UiChromeSession) -> Option<String> {
    match &session.target {
        UiChromeSessionTarget::Sim { project_key } => project_key.as_ref().map(|key| {
            StudioRoute::Sim {
                key: key.clone(),
                play: false,
            }
            .hash()
        }),
        UiChromeSessionTarget::Device { uid } => uid.as_ref().map(|uid| {
            StudioRoute::Device {
                uid: uid.clone(),
                play: false,
            }
            .hash()
        }),
    }
}

fn chip_state_class(session: &UiChromeSession, on_editor: bool) -> &'static str {
    match (session.lensed, on_editor) {
        (true, true) => SESSION_CHIP_HERE,
        (true, false) => SESSION_CHIP_OPEN,
        (false, _) => SESSION_CHIP_IDLE,
    }
}

/// The session's kind glyph: violet sim mark for the simulator (the
/// bound-family convention the sim card wears), transport icon for
/// hardware — USB today; the slot grows network/BT glyphs with those
/// transports.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SessionGlyph(sim: bool) -> Element {
    if sim {
        rsx! {
            span { class: "tw:flex-none tw:text-status-bound-foreground",
                StudioIcon { name: StudioIconName::Simulator, size: 12 }
            }
        }
    } else {
        rsx! {
            span { class: "tw:flex-none",
                StudioIcon { name: StudioIconName::Usb, size: 12 }
            }
        }
    }
}

/// The chip's status dot (D16): accent run / amber attention / hollow
/// connected-empty.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SessionDot(status: UiChromeSessionStatus) -> Element {
    let class = match status {
        UiChromeSessionStatus::Run => "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-accent",
        UiChromeSessionStatus::Attention => {
            "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-attention-foreground"
        }
        UiChromeSessionStatus::Empty => {
            "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:border tw:border-border-strong tw:bg-transparent"
        }
    };
    rsx! {
        span { class: "{class}" }
    }
}

/// One session row of the ⋯ menu: the chip anatomy at menu width,
/// closing the menu as it navigates. *Here* rows read heading-bold.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SessionMenuRow(session: UiChromeSession, on_editor: bool) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    let class = match (session.lensed, on_editor) {
        (true, true) => NAV_MENU_ITEM_ACTIVE,
        _ => NAV_MENU_ITEM_IDLE,
    };
    let body = rsx! {
        SessionGlyph { sim: session.sim }
        SessionDot { status: session.status }
        span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap",
            "{session.name}"
        }
    };
    match session_href(&session) {
        Some(href) => rsx! {
            a {
                class: "{class} tw:flex tw:items-center tw:gap-1.5",
                href: "{href}",
                onclick: move |_| {
                    if let Some(mut close) = close {
                        close.close();
                    }
                },
                {body}
            }
        },
        None => rsx! {
            span { class: "{class} tw:flex tw:items-center tw:gap-1.5", {body} }
        },
    }
}

/// One tool card: glyph, name, one line of what it is, and the
/// opens-in-a-new-tab marker. Tools open in a new tab so the studio
/// session behind them keeps running.
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
/// Secondary-family active: the same current-destination grammar, one
/// weight lighter — the family reads quieter even when it is where you
/// are.
const NAV_TAB_SECONDARY_ACTIVE: &str = "tw:relative tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-semibold tw:text-heading tw:no-underline tw:after:absolute tw:after:inset-x-2.5 tw:after:-bottom-[11px] tw:after:h-0.5 tw:after:rounded-full tw:after:bg-accent tw:after:content-['']";
/// Secondary-family idle: reduced weight and dimmed, full strength on
/// hover.
const NAV_TAB_SECONDARY_IDLE: &str = "tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-medium tw:text-subtle-foreground/70 tw:no-underline tw:transition-colors tw:hover:bg-background-wash tw:hover:text-strong-foreground";

/// The lockup's wrapper at Home: the tabs' accent underline under the
/// brand — the logo IS Home's tab, so at Home it marks the place like
/// one. The offset differs from the tabs' because the lockup's box is
/// shorter; both land the bar on the header's border line.
const LOGO_HOME_ACTIVE_WRAP: &str = "tw:relative tw:flex tw:flex-none tw:after:absolute tw:after:inset-x-0 tw:after:-bottom-[14px] tw:after:h-0.5 tw:after:rounded-full tw:after:bg-accent tw:after:content-['']";

/// ⋯ menu section/session row, idle.
const NAV_MENU_ITEM_IDLE: &str = "tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:no-underline tw:transition-colors tw:hover:bg-card-raised tw:hover:text-strong-foreground";
/// ⋯ menu section/session row, current place.
const NAV_MENU_ITEM_ACTIVE: &str = "tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-bold tw:text-heading tw:no-underline tw:transition-colors tw:hover:bg-card-raised";

// Session chips (D16): squared tab-chips in the entity grammar — never
// status pills. One shared geometry, three states.
/// *Here*: lensed and the editor is the fronted route — the chip IS the
/// current-place marker (no nav tab lights during a lens route).
const SESSION_CHIP_HERE: &str = "tw:inline-flex tw:max-w-[148px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:rounded-sm tw:border tw:border-accent-border tw:px-2 tw:py-1 tw:text-[11px] tw:font-bold tw:text-heading tw:no-underline";
/// *Open*: lensed, but another section is fronted — washed presence.
const SESSION_CHIP_OPEN: &str = "tw:inline-flex tw:max-w-[148px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:rounded-sm tw:border tw:border-border tw:bg-background-wash tw:px-2 tw:py-1 tw:text-[11px] tw:font-semibold tw:text-muted-foreground tw:no-underline tw:transition-colors tw:hover:text-strong-foreground";
/// Idle: a live session the lens is not on.
const SESSION_CHIP_IDLE: &str = "tw:inline-flex tw:max-w-[148px] tw:min-w-0 tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-transparent tw:px-2 tw:py-1 tw:text-[11px] tw:font-semibold tw:text-subtle-foreground tw:no-underline tw:transition-colors tw:hover:border-border-strong tw:hover:text-strong-foreground";

/// The one ⋯ menu popup: wide enough for tool cards; section and session
/// rows ride the same width.
const OVERFLOW_POPUP_CLASS: &str = "tw:grid tw:w-[288px] tw:gap-1 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-1.5 tw:text-sm tw:text-muted-foreground tw:shadow-lg";
/// Mini-header labelling each group of the ⋯ menu.
const GROUP_HEADER_CLASS: &str = "tw:px-1.5 tw:pt-1.5 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground tw:first:pt-0.5";
/// Rows are cards, not text links: fixed three-column grid so the title and
/// detail wrap inside their own column instead of around the glyphs.
const TOOL_CARD_CLASS: &str = "tw:grid tw:grid-cols-[auto_minmax(0,1fr)_auto] tw:items-start tw:gap-2.5 tw:rounded-sm tw:border tw:border-transparent tw:px-2 tw:py-2 tw:no-underline tw:transition-colors tw:hover:border-border tw:hover:bg-card-raised";
