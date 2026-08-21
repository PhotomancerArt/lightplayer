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
//! - **The brand lockup is the way to Home.** The logo links to `/` —
//!   Home is the root landing (Yona 2026-08-06) — and there is
//!   deliberately no Home tab, and no Studio tab either (the sections
//!   replaced it).
//! - **One overflow menu** (G3 ruling, 2026-08-05: a row of separate
//!   menus read as clutter — merge them ALL). The single ⋯ at the bar's
//!   end always holds the tools, and grows the secondary sections at
//!   narrow widths when the inline tabs collapse (the bar is a
//!   container; the cut is where three secondary tabs stop fitting, not
//!   a viewport magic number). The brand word yields at narrow too — the
//!   mark stays.
//! - **The narrow ladder is conditional on load.** A bar carrying the
//!   session control and its Play toggle stops fitting ~220px sooner
//!   than a plain one, so the crowded bar folds earlier (G1 of the
//!   session·project control, 2026-08-19 — the squished phone bar):
//!   <900 the world nav retreats to ⋯, the toggle and the Share pill go
//!   icon-only, and the version chip hides (the device name already
//!   folds there); <680 the brand word yields with the ↺; <560 the phone
//!   bar — Devices/Projects become ⋯ rows and gaps tighten. What never
//!   folds: the control, Save, Play, settings, the account slot, and ⋯ —
//!   the phone bar is the session, the one action on it, and the doors.
//!   (Patch stopped being a chrome mode in R5 — the workbench band tab
//!   and the ≤820px summon strip carry the Patching view at every width,
//!   so the ladder has no Patch rungs.)
//! - **One session per tab, and the tab IS the session** (single-session
//!   web policy): opening a project or connecting a device tears the
//!   other kind of session down first, so there is never a strip or a
//!   session list to navigate — the header session·project control
//!   ([`SessionProjectControl`]) is the ONE piece of session UI the
//!   chrome carries, standing for the tab's session and the project on
//!   it together. No nav tab ends the session either; navigation does,
//!   through the route listener, same as the back button (see below).
//! - **Navigation is studio OR site** (single-session policy): from a
//!   lens route, going anywhere else ENDS the tab's session. Docs and
//!   Boards are the exception, and they earn it by not going anywhere —
//!   in studio mode they open a NEW tab, so reference material never
//!   costs you the thing you were building. Explore is a plain exit: it
//!   is a gallery of live projects, a real section of the app (ruling
//!   R8-3, amended 8.1).
//! - **The editors are tools, not sections.** The mapping editor and
//!   board editor stay outside the tab row, in the ⋯ menu's Tools group.
//!
//! The chrome is presentational. Nav tabs are plain hash links: `web_app`
//! owns the route signal and swaps only the body beneath this bar, so
//! moving between sections never unloads the actor, the runtime pool, or
//! any open sim/device session. Nothing here reloads the page.

use dioxus::prelude::*;
use dioxus_icons::lucide::{Archive, Play, UserRound};

use crate::app::layout::session_control::{ChromeSessionControl, SessionProjectControl};
use crate::base::{
    IconMenuButton, IconMenuTone, LogoLockup, PopoverCloseHandle, StudioIcon, StudioIconName,
};

/// The lens routes' mode toggle (Play): the target of the plain hash link
/// and whether the mode is currently on. A prop rather than a child so
/// the chrome's narrow ladder can reason about it. (Patch's toggle is
/// gone — R5 made patching a workbench view, carried by the band tab and
/// the summon strip at every width.)
#[derive(Clone, PartialEq)]
pub struct ChromeModeToggle {
    /// The mode's variant of the CURRENT route (same session, other zoom).
    pub href: String,
    /// The mode is on — the toggle wears the active treatment and reads
    /// "Exit …".
    pub active: bool,
}

/// The project-scoped rows the ⋯ menu grows while a project route is open
/// (spike `project-share` §5, ruling G4).
///
/// Sharing lives here as well as on the pill because a menu is where people
/// look for a project's *settings*; archive lives here and nowhere else
/// because the Share panel stays pure access control, the way Docs keeps
/// them apart. Both are handlers rather than markup so the chrome stays
/// presentational — what "archive" means to the service belongs to
/// `app::share`, and where the app goes afterwards belongs to `web_app`.
#[derive(Clone, PartialEq)]
pub struct ChromeProjectMenu {
    /// Open the Share panel (the same panel the pill opens).
    pub on_share: EventHandler<()>,
    /// Archive this project. Quiet, not destructive: it is reversible.
    pub on_archive: EventHandler<()>,
}

/// Which nav tab renders as the current section. Home has no tab — the
/// LOGO is its affordance, and at Home the logo wears the you're-here
/// underline in a tab's stead (G3 feedback). Session is the editor lens
/// fronted: no tab lights, because the header session·project control is
/// the current-place marker there (single-session policy — the control
/// is the editor's representation in the nav). Account lights no tab
/// either: `/account` is reached from the identity dropdown, and the
/// AVATAR is its marker — the same argument as Session's, one cluster to
/// the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteSection {
    Home,
    Devices,
    Projects,
    Explore,
    Boards,
    Docs,
    Session,
    Account,
}

/// The shared top bar. `children` render at the start of the right-hand
/// cluster (the studio shell passes its version chip and settings trigger
/// through here).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SiteChrome(
    section: SiteSection,
    /// Stories only: mount the narrow ⋯ menu open (capture can't hover).
    #[props(default = false)]
    overflow_menu_open: bool,
    /// The project rows the ⋯ menu grows on a project route; `None`
    /// everywhere else (the menu then reads exactly as it always has).
    #[props(default)]
    project_menu: Option<ChromeProjectMenu>,
    /// THE session·project control (the B lockup); `None` off the lens
    /// routes. Mounted UNGATED — no `tw:@min-*` — so it is present at every
    /// header width (Q10 ruling: one mount, no top-layer/container-query
    /// workaround); the FOLDS live inside the control.
    #[props(default)]
    session_control: Option<ChromeSessionControl>,
    /// The play-mode toggle (P12); lens routes only. Inline at every width
    /// (a phone is where play mode earns its keep).
    #[props(default)]
    play_toggle: Option<ChromeModeToggle>,
    /// The workbench routes' spacing (Final-gate ruling): the header's
    /// gap below shrinks so the full-height frame starts close under the
    /// chrome. Document routes keep the roomy default.
    #[props(default = false)]
    tight: bool,
    children: Element,
) -> Element {
    let margin = if tight { "tw:mb-1.5" } else { "tw:mb-[18px]" };
    // Studio mode: a lens route is fronted, so the tab IS a running
    // session (single-session policy — leaving ends it). Docs and Boards
    // are reference material you read WHILE building, so from here they
    // open a new tab and the session behind them keeps running (ruling
    // R8-3, amended 8.1). Explore deliberately does not: it is a gallery
    // of live projects — a real section of the app, and going there is
    // going somewhere.
    let studio_mode = section == SiteSection::Session;
    // The narrow ladder's rungs. "The cut is where things stop fitting"
    // (module docs) — and a bar carrying the session control plus its
    // toggles stops fitting ~220px sooner than a plain one, so the world
    // nav's cut is CONDITIONAL on that load, not a second magic number:
    //   crowded <900 — the world's tabs retreat to ⋯ where the device
    //     name already folds; the brand word yields at <680 with the ↺.
    //   crowded <560 — the phone bar: Devices/Projects join the ⋯ menu's
    //     sections (all five), Patch becomes a menu row, gaps tighten.
    //   plain  <680 — the original ladder, unchanged.
    let crowded = session_control.is_some();
    let bar_gap = if crowded {
        "tw:gap-4 tw:@max-[560px]:gap-2"
    } else {
        "tw:gap-4"
    };
    let primary_nav = if crowded {
        "tw:hidden tw:items-center tw:gap-1 tw:@min-[560px]:flex"
    } else {
        "tw:flex tw:items-center tw:gap-1"
    };
    let secondary_nav = if crowded {
        "tw:hidden tw:items-center tw:gap-1 tw:@min-[900px]:flex"
    } else {
        "tw:hidden tw:items-center tw:gap-1 tw:@min-[680px]:flex"
    };
    let cluster_gap = if crowded {
        "tw:ml-auto tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:@max-[560px]:gap-1.5"
    } else {
        "tw:ml-auto tw:flex tw:min-w-0 tw:items-center tw:gap-2"
    };
    // The ⋯ mount ladder (a top-layer popup cannot answer the header's
    // container query, so each rung is its own mount — Q10): the widest
    // carries no section rows, the middle mirrors the folded secondary
    // family, and the crowded bar's phone mount adds the primary family
    // and the Patch row. `overflow_menu_open` (stories) rides the
    // narrowest mount, matching the narrow captures it exists for.
    let menu_mounts: &[(&'static str, bool, bool)] = if crowded {
        &[
            ("tw:hidden tw:@min-[900px]:block", false, false),
            (
                "tw:hidden tw:@min-[560px]:block tw:@min-[900px]:hidden",
                true,
                false,
            ),
            ("tw:block tw:@min-[560px]:hidden", true, true),
        ]
    } else {
        &[
            ("tw:hidden tw:@min-[680px]:block", false, false),
            ("tw:block tw:@min-[680px]:hidden", true, false),
        ]
    };
    rsx! {
        // `tw:@container`: the collapse below responds to the BAR's own
        // width, not the viewport, so an embedded/narrow mount behaves.
        header { class: "tw:@container {margin} tw:flex tw:min-h-[46px] tw:items-center {bar_gap} tw:border-b tw:border-border-subtle tw:pb-2.5",
            // Brand lockup — the way to Home (see module docs). At Home it
            // wears the tabs' you're-here underline (G3 feedback: the logo
            // IS Home's tab, so it marks the place like one).
            span {
                class: if section == SiteSection::Home { LOGO_HOME_ACTIVE_WRAP } else { "tw:flex tw:flex-none" },
                LogoLockup { href: "/".to_string(), early_word_yield: crowded }
            }
            // Primary family: your things, by the brand, full weight —
            // inline while they fit; rows in the phone bar's ⋯ menu.
            nav { class: "{primary_nav}",
                NavTab {
                    label: "Devices",
                    href: "/devices",
                    active: section == SiteSection::Devices,
                }
                NavTab {
                    label: "Projects",
                    href: "/projects",
                    active: section == SiteSection::Projects,
                }
            }
            if let Some(control) = session_control {
                // THE control: after the primary family — this tab's
                // session and the project on it are the most local things
                // in the bar. It is the ONE piece of session UI the chrome
                // carries (single-session policy).
                SessionProjectControl { control }
            }
            div { class: "{cluster_gap}",
                // Secondary family: lighter, right cluster, no divider —
                // inline while three tabs fit the bar; in the ⋯ below
                // when they don't.
                nav { class: "{secondary_nav}",
                    NavTab {
                        label: "Explore",
                        href: "/explore",
                        active: section == SiteSection::Explore,
                        secondary: true,
                    }
                    NavTab {
                        label: "Boards",
                        href: "/boards",
                        active: section == SiteSection::Boards,
                        secondary: true,
                        new_tab: studio_mode,
                    }
                    NavTab {
                        label: "Docs",
                        href: "/docs",
                        active: section == SiteSection::Docs,
                        secondary: true,
                        new_tab: studio_mode,
                    }
                }
                if let Some(toggle) = play_toggle.clone() {
                    PlayToggle { href: toggle.href, playing: toggle.active }
                }
                {children}
                // THE overflow menu — one ⋯ at every width (G3 ruling),
                // one mount per rung (see `menu_mounts`).
                for (index , (wrap , sections , primary)) in menu_mounts.iter().enumerate() {
                    div { key: "{wrap}", class: "{wrap}",
                        ChromeOverflowMenu {
                            section,
                            include_sections: *sections,
                            include_primary: *primary,
                            initially_open: overflow_menu_open && index == menu_mounts.len() - 1,
                            project_menu: project_menu.clone(),
                        }
                    }
                }
            }
        }
    }
}

/// THE ⋯ menu (G3 ruling, 2026-08-05): sections (narrow only — they are
/// inline tabs while they fit) and the tools, in one place. Groups wear
/// mini-headers; rows keep their own grammars — section rows navigate
/// this tab and close the menu, tool cards open a new one.
///
/// `include_primary` is the phone rung (crowded bars <560): Devices and
/// Projects join the Sections group ahead of the world's three.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChromeOverflowMenu(
    section: SiteSection,
    include_sections: bool,
    #[props(default = false)] include_primary: bool,
    #[props(default = false)] initially_open: bool,
    #[props(default)] project_menu: Option<ChromeProjectMenu>,
) -> Element {
    // Same derivation, same promise as the inline tabs (see [`SiteChrome`]).
    let studio_mode = section == SiteSection::Session;
    rsx! {
        IconMenuButton {
            icon: StudioIconName::More,
            icon_size: 15,
            label: "More".to_string(),
            title: "Sections and tools".to_string(),
            tone: IconMenuTone::Quiet,
            initially_open,
            popup_class: OVERFLOW_POPUP_CLASS.to_string(),
            // One explicit grid wrapper: the popover primitive nests
            // children in its own content div, so the panel's classes
            // never reach them — inline rows would flow sideways.
            div { class: "tw:grid tw:gap-1",
                // The project group leads: on a project route it is the
                // most local thing in the menu, and the sections it sits
                // above are the same everywhere in the app.
                if let Some(project_menu) = project_menu {
                    span { class: GROUP_HEADER_CLASS, "Project" }
                    ProjectMenuRows { menu: project_menu }
                }
                if include_sections {
                    span { class: GROUP_HEADER_CLASS, "Sections" }
                    if include_primary {
                        NavMenuItem { label: "Devices", href: "/devices", active: section == SiteSection::Devices }
                        NavMenuItem { label: "Projects", href: "/projects", active: section == SiteSection::Projects }
                    }
                    NavMenuItem { label: "Explore", href: "/explore", active: section == SiteSection::Explore }
                    NavMenuItem {
                        label: "Boards",
                        href: "/boards",
                        active: section == SiteSection::Boards,
                        new_tab: studio_mode,
                    }
                    NavMenuItem {
                        label: "Docs",
                        href: "/docs",
                        active: section == SiteSection::Docs,
                        new_tab: studio_mode,
                    }
                }
                span { class: GROUP_HEADER_CLASS, "Tools" }
                // (The standalone mapping-editor card is gone: bare
                // `/mapping` died with the standalone page — mapping and
                // patching live in the project workbench's view tabs now.)
                ToolCard {
                    icon: StudioIconName::NodeKind(crate::base::NodeKindIcon::Compute),
                    title: "Board editor",
                    detail: "Draw and edit the board diagrams behind the catalog.",
                    href: "/boards/edit",
                }
            }
        }
    }
}

/// One section row of the ⋯ menu: a plain hash link that closes the menu
/// as it navigates (sections swap in place — without the close the
/// popover would linger over the new body).
///
/// `new_tab` mirrors [`NavTab`]'s: the narrow bar's Docs and Boards rows
/// have to make the same promise the inline tabs do, or the same click
/// would mean two different things at two widths.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NavMenuItem(
    label: String,
    href: String,
    active: bool,
    #[props(default = false)] new_tab: bool,
) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    let class = if active {
        NAV_MENU_ITEM_ACTIVE
    } else {
        NAV_MENU_ITEM_IDLE
    };
    let target = new_tab.then_some("_blank");
    let rel = new_tab.then_some("noopener noreferrer");
    rsx! {
        a {
            class: if new_tab { "{class} tw:flex tw:items-center tw:gap-1.5" } else { "{class}" },
            href: "{href}",
            aria_current: if active { "page" } else { "false" },
            target,
            rel,
            onclick: move |_| {
                if let Some(mut close) = close {
                    close.close();
                }
            },
            "{label}"
            if new_tab {
                span { class: "tw:flex-none tw:text-subtle-foreground/70",
                    StudioIcon { name: StudioIconName::ExternalLink, size: 11 }
                }
            }
        }
    }
}

/// The ⋯ menu's project rows: the sharing door, then the removal verb.
///
/// **Archive is a quiet row, not a red one** (spike §5): it is reversible —
/// the project stops resolving for everyone but its members and nothing is
/// thrown away — and dressing a reversible act as a destructive one teaches
/// people to fear the wrong control. There is no Delete forever here at
/// all; the archive drawer on the Projects page is where such a thing would
/// eventually have to live, spelled like what it is.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectMenuRows(menu: ChromeProjectMenu) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    let ChromeProjectMenu {
        on_share,
        on_archive,
    } = menu;
    rsx! {
        button {
            class: PROJECT_MENU_ROW,
            r#type: "button",
            onclick: move |_| {
                // Close FIRST: the panel this opens is another popover
                // anchored in the same bar, and two open at once reads as
                // a stuck menu.
                if let Some(mut close) = close {
                    close.close();
                }
                on_share.call(());
            },
            UserRound { size: 14 }
            // The row's type lives on the span: `style.css` resets
            // `button { font: inherit }` UNLAYERED, which beats every
            // (layered) Tailwind font utility on the button itself.
            span { class: "tw:min-w-0 tw:truncate tw:text-xs tw:font-semibold", "Sharing & access…" }
        }
        button {
            class: PROJECT_MENU_ROW_QUIET,
            r#type: "button",
            title: "Archive this project — reversible, and nothing is deleted",
            onclick: move |_| {
                if let Some(mut close) = close {
                    close.close();
                }
                on_archive.call(());
            },
            Archive { size: 14 }
            span { class: "tw:min-w-0 tw:truncate tw:text-xs tw:font-medium", "Archive project" }
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
            class: "{class} tw:inline-flex tw:items-center",
            href: "{href}",
            title: if playing { "Back to the editor" } else { "Play mode: the panel, full screen" },
            // Word above the utility cut, glyph below it (the crowded
            // ladder's <900 rung): the mode keeps its one-tap home on the
            // phone bar, at glyph cost. The active underline carries the
            // on-state either way.
            span { class: "tw:hidden tw:@min-[900px]:inline", "{label}" }
            span { class: "tw:inline-flex tw:@min-[900px]:hidden", Play { size: 14 } }
        }
    }
}

/// One nav tab. Active: heading color + accent underline; inactive: subtle
/// text that brightens on hover. `secondary` is the lighter family
/// treatment (reduced weight, dimmer at rest, full strength on
/// hover/active — the spike's `.secondary`).
///
/// Tabs are PLAIN links on purpose (P12): no tab dispatches a lens
/// detach anymore — navigation to a gallery route ENDS the tab's session
/// through the route listener, the same path as the back button.
///
/// `new_tab` is the studio-mode exit (ruling R8-3, amended 8.1): see
/// [`SiteChrome`]'s Docs/Boards mounts for why those two leave the tab
/// rather than the session.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NavTab(
    label: &'static str,
    href: &'static str,
    active: bool,
    #[props(default = false)] secondary: bool,
    /// Open in a new tab (`_blank` + `noopener noreferrer`) and wear the
    /// leaves-this-tab marker. The router's click interceptor skips any
    /// link with a `target`, so this needs no routing support at all.
    #[props(default = false)]
    new_tab: bool,
) -> Element {
    let class = match (secondary, active) {
        (false, true) => NAV_TAB_ACTIVE,
        (false, false) => NAV_TAB_IDLE,
        (true, true) => NAV_TAB_SECONDARY_ACTIVE,
        (true, false) => NAV_TAB_SECONDARY_IDLE,
    };
    // `None` omits the attribute entirely, which is what the click
    // interceptor reads as "in-app" — an empty `target=""` would work
    // too, but only by accident.
    let target = new_tab.then_some("_blank");
    let rel = new_tab.then_some("noopener noreferrer");
    rsx! {
        a {
            class: if new_tab { "{class} tw:inline-flex tw:items-center tw:gap-1" } else { "{class}" },
            href: "{href}",
            aria_current: if active { "page" } else { "false" },
            target,
            rel,
            "{label}"
            if new_tab {
                // Sized to the tab's own 12px text, and quiet: the mark
                // says where the click goes, it is not a second label.
                span { class: "tw:flex-none tw:text-subtle-foreground/70",
                    StudioIcon { name: StudioIconName::ExternalLink, size: 11 }
                }
            }
        }
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
pub(crate) const NAV_TAB_SECONDARY_IDLE: &str = "tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-medium tw:text-subtle-foreground/70 tw:no-underline tw:transition-colors tw:hover:bg-background-wash tw:hover:text-strong-foreground";

/// The lockup's wrapper at Home: the tabs' accent underline under the
/// brand — the logo IS Home's tab, so at Home it marks the place like
/// one. The offset differs from the tabs' because the lockup's box is
/// shorter; both land the bar on the header's border line.
const LOGO_HOME_ACTIVE_WRAP: &str = "tw:relative tw:flex tw:flex-none tw:after:absolute tw:after:inset-x-0 tw:after:-bottom-[14px] tw:after:h-0.5 tw:after:rounded-full tw:after:bg-accent tw:after:content-['']";

/// ⋯ menu section row, idle.
pub(crate) const NAV_MENU_ITEM_IDLE: &str = "tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:no-underline tw:transition-colors tw:hover:bg-card-raised tw:hover:text-strong-foreground";
/// ⋯ menu section row, current place.
const NAV_MENU_ITEM_ACTIVE: &str = "tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-bold tw:text-heading tw:no-underline tw:transition-colors tw:hover:bg-card-raised";
/// ⋯ menu PROJECT row. A `<button>`, so it must name its own background
/// and border explicitly — this build ships Tailwind without preflight, and
/// an unstyled button paints the UA's `buttonface` (crate README).
const PROJECT_MENU_ROW: &str = "tw:flex tw:w-full tw:cursor-pointer tw:items-center tw:gap-2.5 tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-2.5 tw:py-1.5 tw:text-left tw:text-muted-foreground tw:transition-colors tw:hover:bg-card-raised tw:hover:text-strong-foreground";
/// The same row, quieter: the reversible act (Archive) reads lighter than
/// the one above it, and never destructive.
const PROJECT_MENU_ROW_QUIET: &str = "tw:flex tw:w-full tw:cursor-pointer tw:items-center tw:gap-2.5 tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-2.5 tw:py-1.5 tw:text-left tw:text-subtle-foreground tw:transition-colors tw:hover:bg-card-raised tw:hover:text-strong-foreground";

/// The one ⋯ menu popup: wide enough for tool cards; section rows ride
/// the same width.
const OVERFLOW_POPUP_CLASS: &str = "tw:grid tw:w-[288px] tw:gap-1 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-1.5 tw:text-sm tw:text-muted-foreground tw:shadow-lg";
/// Mini-header labelling each group of the ⋯ menu.
pub(crate) const GROUP_HEADER_CLASS: &str = "tw:px-1.5 tw:pt-1.5 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground tw:first:pt-0.5";
/// Rows are cards, not text links: fixed three-column grid so the title and
/// detail wrap inside their own column instead of around the glyphs.
const TOOL_CARD_CLASS: &str = "tw:grid tw:grid-cols-[auto_minmax(0,1fr)_auto] tw:items-start tw:gap-2.5 tw:rounded-sm tw:border tw:border-transparent tw:px-2 tw:py-2 tw:no-underline tw:transition-colors tw:hover:border-border tw:hover:bg-card-raised";
