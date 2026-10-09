//! The home page (`/`): one page for everything the user's light lives in.
//!
//! It replaces the landing hero, the Devices page and the Projects page
//! (`docs/adr/2026-10-08-the-board-card-and-one-home-page.md`, section 1).
//! Top to bottom:
//!
//! 1. quiet lines — a `/p/` link's fate, a failed open, a library problem,
//!    and the sign-in line;
//! 2. the controls — the tab strip (All · Boards · Projects · Patterns) and
//!    the cards/list switch, both hidden on a first visit;
//! 3. the sections the selected tab shows, in the order core lists them
//!    ([`UiHomeSections::visible`](lpa_studio_core::UiHomeSections::visible)):
//!    Online boards, **Connect a board**, Offline boards, the "Unlocking
//!    your boards" fold, Other projects, Your patterns, then the catalog's
//!    Example projects and Example patterns;
//! 4. the footer.
//!
//! Core decides what is in each section and which tab shows it
//! (`UiHomeView::sections`, `UiHomeTab::shows`); this page draws. The tab and
//! the cards/list switch are view state (PD4): neither is an action nor in
//! the offer tree. The switch is remembered in the browser
//! ([`HomeViewMode`]); the tab is not.
//!
//! Every board's card is mounted by one component,
//! [`BoardCardSlot`](super::board_card_slot::BoardCardSlot), so the board
//! card (M2) swaps one body. The page also serves the shell's no-editor
//! arm (PD1): a cold `/device/<uid>` load draws it while the board
//! connects, its card being the connect evidence.
//!
//! The page takes `Option<UiHomeView>` and assumes no `StudioShell` around
//! it. With `None` (a story with no view, a frame while a project loads) it
//! draws the examples and the footer, as the landing did.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiHomeSection, UiHomeTab, UiHomeView, example_groups};

use super::example_groups::{ExampleGroups, ExampleHeading};
use super::filter_tabs::{FilterTabs, home_filter_tabs, home_tab_for_key};
use super::home_view_mode::HomeViewMode;
use super::keys_fold::KeysFold;
use super::offline_boards::OfflineBoards;
use super::online_boards::OnlineBoards;
use super::sign_in_prompt::SignInPrompt;
use super::view_switch::ViewSwitch;
use crate::app::home::connect_board::ConnectBoardSection;
use crate::app::home::connect_board::connect_board_section::{ConnectStoryPins, HOME_EXAMPLES_ID};
use crate::app::home::device_layout_sheet::BackupDownloadWatcher;
use crate::app::home::example_card::embedded_example_cards;
use crate::app::home::gallery_preview::HoveredCard;
use crate::app::home::project_opening_frame::OpenFailureNotice;
use crate::cloud::SharedOpenState;

/// The home page.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HomePage(
    /// The page's data. `None` draws only the examples and the footer.
    #[props(default)]
    home: Option<UiHomeView>,
    /// A fixed clock for stories ("edited 3 days ago" is read against it).
    #[props(default)]
    now_secs: Option<f64>,
    /// `None` in stories and host mounts: the examples render and clicks
    /// are no-ops.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
    /// Stories only: the tab the page starts on.
    #[props(default)]
    initial_tab: Option<UiHomeTab>,
    /// Stories only: the mode the page starts in (the real page reads the
    /// browser's).
    #[props(default)]
    initial_mode: Option<HomeViewMode>,
    /// Stories only: the Connect a board section's pins.
    #[props(default)]
    connect_pins: ConnectStoryPins,
) -> Element {
    // Read by the projects sections ("edited 3 days ago"); the page frame
    // itself shows no time.
    let _ = now_secs;
    let mut tab = use_signal(|| initial_tab.unwrap_or_default());
    let mut mode = use_signal(|| initial_mode.unwrap_or_else(HomeViewMode::load));
    // Hover-to-play for the example grid: one signal names one hovered
    // card, so the page holds at most one live preview lease at a time.
    use_context_provider(|| HoveredCard(Signal::new(None)));
    // A `/p/` link that landed here (P6): one quiet line about where it
    // stands — opening, or the calm refusal that never says which of
    // restricted/archived/absent it was. Stories provide no context and
    // render nothing.
    let shared_open = try_consume_context::<Signal<SharedOpenState>>();
    let shared_line = shared_open.and_then(|state| {
        let state = state();
        state.line().map(|line| (line, state.is_refusal()))
    });
    // A View link whose fetch and format/content checks passed can still
    // fail once the open itself runs — a sim that won't boot, a device or
    // engine failure. The same notice Explore shows for the identical
    // terminal state.
    let failure = match lpa_studio_core::open_stage() {
        lpa_studio_core::OpenStage::Failed(failure) => Some(failure),
        _ => None,
    };

    let newcomer = home.as_ref().is_some_and(|home| home.sections.newcomer);
    // A first visit has nothing to filter: the page is the All tab.
    let shown_tab = match newcomer {
        true => UiHomeTab::All,
        false => tab(),
    };
    let visible: Vec<UiHomeSection> = home
        .as_ref()
        .map(|home| home.sections.visible(shown_tab))
        .unwrap_or_default();
    let examples = embedded_example_cards();
    let groups: Vec<_> = example_groups(&examples)
        .into_iter()
        .filter(|group| {
            let section = match group.key {
                "patterns" => UiHomeSection::ExamplePatterns,
                _ => UiHomeSection::ExampleProjects,
            };
            shown_tab.shows(section)
        })
        .collect();
    let opening = home.as_ref().and_then(|home| home.opening.clone());
    let busy = opening.is_some();
    let on_action_or_none = on_action.unwrap_or_else(|| EventHandler::new(|_| {}));

    rsx! {
        div { class: "tw:mx-auto tw:grid tw:w-full tw:max-w-[1140px] tw:content-start tw:gap-7",
            if let Some((line, refusal)) = shared_line {
                p {
                    class: if refusal { "{SHARED_LINE_CLASS} tw:border-status-warning-border tw:bg-status-warning-bg tw:text-status-warning-foreground" } else { "{SHARED_LINE_CLASS} tw:border-border tw:bg-card tw:text-muted-foreground" },
                    role: "status",
                    "{line}"
                }
            }
            if let Some(failure) = failure {
                OpenFailureNotice {
                    message: failure.message,
                    retry: failure.retry,
                    on_action,
                }
            }
            if let Some(issue) = home.as_ref().and_then(|home| home.issue.clone()) {
                div { class: "tw:flex tw:items-center tw:gap-3 tw:rounded-md tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-4 tw:py-2.5 tw:text-sm tw:text-status-error-foreground",
                    span { "{issue.message}" }
                }
            }
            if home.is_some() {
                SignInPrompt { newcomer }
            }
            if let Some(home) = home.as_ref().filter(|_| !newcomer) {
                div { class: "tw:flex tw:items-end tw:gap-5 tw:border-b tw:border-border-muted",
                    FilterTabs {
                        tabs: home_filter_tabs(&home.sections),
                        selected: shown_tab.key().to_string(),
                        on_select: move |key: String| tab.set(home_tab_for_key(&key)),
                    }
                    div { class: "tw:flex-none tw:pb-1.5",
                        ViewSwitch {
                            mode: mode(),
                            on_select: move |next: HomeViewMode| {
                                mode.set(next);
                                next.save();
                            },
                        }
                    }
                }
            }
            if let Some(home) = home.as_ref() {
                // A board's backup, handed over as a file when core
                // prepares one. Invisible; mounted wherever the page is.
                BackupDownloadWatcher { download: home.devices.backup_download.clone() }
                for section in visible {
                    match section {
                        UiHomeSection::OnlineBoards => rsx! {
                            OnlineBoards {
                                key: "{section:?}",
                                home: home.clone(),
                                mode: mode(),
                                on_action: on_action_or_none,
                            }
                        },
                        UiHomeSection::ConnectBoard => rsx! {
                            ConnectBoardSection {
                                key: "{section:?}",
                                usb_available: home.devices.usb_available,
                                transport_available: home.devices.transport_available,
                                wifi_connect: home.devices.wifi_address_connect.clone(),
                                welcome: home.sections.connect.welcome,
                                ble_reach: connect_pins.ble_reach,
                                page_url: connect_pins.page_url.clone(),
                                wifi_typed: connect_pins.wifi_typed.clone(),
                                pick_open: connect_pins.pick_open,
                                network_open: connect_pins.network_open,
                                on_action: on_action_or_none,
                            }
                        },
                        UiHomeSection::OfflineBoards => rsx! {
                            OfflineBoards {
                                key: "{section:?}",
                                home: home.clone(),
                                mode: mode(),
                                on_action: on_action_or_none,
                            }
                        },
                        // A first visit has no board to unlock.
                        UiHomeSection::UnlockingYourBoards if !newcomer => rsx! {
                            KeysFold { key: "{section:?}" }
                        },
                        // The projects sections arrive with their own
                        // phase; the catalog draws below.
                        _ => rsx! {},
                    }
                }
            }
            // The catalog, grouped by kind (catalog content tree D17): real
            // pieces first, then patterns — core derives the sections and
            // their words, this page only lays them out and filters them by
            // the tab. ALL of it (G1 ruling 2026-08-29: "scrolling is better
            // than navigating"). Real, running content one click deep —
            // viewing is stateless (D2). Rendered dispatcher-less too
            // (stories, host mounts): the cards are compiled-in content and
            // clicks just no-op there.
            ExampleGroups {
                groups,
                opening,
                busy,
                heading: ExampleHeading::Rule,
                first_id: Some(HOME_EXAMPLES_ID),
                on_action: on_action_or_none,
            }
            // The page ends on purpose: a quiet footer of reference doors
            // (spike ruling 2026-08-30) instead of the grid just stopping.
            footer { class: "tw:mt-2 tw:flex tw:flex-wrap tw:items-center tw:justify-center tw:gap-x-6 tw:gap-y-2 tw:border-t tw:border-border-muted tw:pt-4 tw:pb-1 tw:text-xs",
                a { class: FOOTER_LINK_CLASS, href: "/docs", "Docs" }
                a { class: FOOTER_LINK_CLASS, href: "/boards", "Supported boards" }
                a {
                    class: FOOTER_LINK_CLASS,
                    href: crate::app::docs::docs_links::what_is_a_shader::HREF,
                    "What's a shader?"
                }
            }
        }
    }
}

/// Footer reference links — the canonical neutral link look (accent
/// reckoning D1: links hold no hue at rest).
const FOOTER_LINK_CLASS: &str = "tw:font-semibold tw:text-muted-foreground tw:no-underline tw:hover:text-strong-foreground tw:hover:underline ux-focus-ring";

/// The one quiet line for a `/p/` link's fate (tone classes appended).
const SHARED_LINE_CLASS: &str =
    "tw:m-0 tw:max-w-md tw:rounded-md tw:border tw:px-4 tw:py-2.5 tw:text-xs tw:leading-snug";
