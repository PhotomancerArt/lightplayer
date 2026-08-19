//! Stories for the shared site chrome (chrome C: split-weight families,
//! ⋯ overflow at narrow).
//!
//! The chrome is presentational (its standalone hashchange listener is
//! route-guarded and never installs under the story book), so these mount
//! it directly per section, with the version-chip preview standing in for
//! the live right-cluster children. The bar is a `@container`, so a
//! narrow FRAME is enough to trigger the collapse — the viewport can stay
//! wide.

use dioxus::prelude::*;
use lpa_studio_core::{
    DirtySummary, ProjectSyncPhase, UiChromeSession, UiChromeSessionControl, UiChromeSessionStatus,
    UiChromeSessionTarget, UiStatus,
};
use lpa_studio_web_story_macros::story;

use crate::app::layout::session_control::ChromeSessionControl;
use crate::app::layout::site_chrome::{ChromeProjectMenu, SiteChrome, SiteSection};
use crate::app::layout::version_badge::{BuildChip, VersionChipPreview};
use crate::app::project::ProjectDetailContent;
use crate::app::story_fixtures::project_editor_fixture;
use crate::base::{LogoLockup, LogoMark};

#[story(
    description = "Wide bar, one row per section: primary family (Devices, Projects) full weight by the brand; secondary family (Explore, Boards, Docs) lighter on the right; Home lights no tab — the logo is its affordance."
)]
pub(crate) fn sections_active() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2",
            for section in [
                SiteSection::Devices,
                SiteSection::Projects,
                SiteSection::Explore,
                SiteSection::Boards,
                SiteSection::Docs,
                SiteSection::Home,
            ]
            {
                {frame(1000, section, branch_chip(), false)}
            }
        }
    }
}

#[story(description = "Devices section active, dev-branch chip on the right.")]
pub(crate) fn devices_active() -> Element {
    frame(1000, SiteSection::Devices, branch_chip(), false)
}

#[story(description = "Docs section active (secondary family), deployed version chip.")]
pub(crate) fn docs_active() -> Element {
    frame(
        1000,
        SiteSection::Docs,
        BuildChip::Release("2026.08.01-2".to_string()),
        false,
    )
}

#[story(
    label = "Narrow (390px)",
    description = "Phone width: brand word hidden, secondary family folded into the ⋯ menu, dirty build icon in warning tone."
)]
pub(crate) fn narrow() -> Element {
    frame(
        390,
        SiteSection::Devices,
        BuildChip::Branch {
            name: "claude/settings-provenance-rework-b6680f".to_string(),
            dirty: true,
        },
        false,
    )
}

#[story(
    label = "Narrow, ⋯ menu open",
    description = "The ONE merged \u{22ef} menu (G3 ruling): Sections, Sessions, and Tools groups in a single popup; active section marked (Docs is current here)."
)]
pub(crate) fn narrow_menu_open() -> Element {
    rsx! {
        div { class: "tw:min-h-[220px]",
            {frame(390, SiteSection::Docs, branch_chip(), true)}
        }
    }
}

#[story(
    description = "The session strip (D15/D16) across counts and widths: one lensed-here sim; three sessions with the lens elsewhere (washed chip) and a behind device; five sessions (the overflow lives in the \u{22ef} menu); a long name ellipsizing. Wide frames show chips, the md frame folds to two, the narrow frame folds every session into the \u{22ef} menu."
)]
pub(crate) fn session_strip_states() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2",
            // one session, lensed, editor fronted (here = accent)
            {strip_frame(1000, vec![sim_session("zook-dome", true, UiChromeSessionStatus::Run)], true, false)}
            // three sessions: lens on a device but another section fronted
            // (washed), a behind device, an empty sim
            {strip_frame(1000, three_sessions(), false, false)}
            // five sessions at wide: cap 4 + "+1"
            {strip_frame(1000, five_sessions(), true, false)}
            // md width: two chips + "+3"
            {strip_frame(780, five_sessions(), true, false)}
            // narrow: the one count chip (dots + 5)
            {strip_frame(390, five_sessions(), true, false)}
            // long names ellipsize inside the chip cap
            {strip_frame(
                1000,
                vec![
                    device_session("deva1", "the-remarkably-long-device-name-from-the-hall", true, UiChromeSessionStatus::Run),
                    sim_session("2026-08-05-2053-xiao-esp32-c6", false, UiChromeSessionStatus::Attention),
                ],
                true,
                false,
            )}
        }
    }
}

#[story(
    label = "Narrow \u{22ef} with sessions",
    description = "The merged menu with a session group: five sessions in menu grammar, here-row bold, statuses marked, tools below."
)]
pub(crate) fn session_flyout_open() -> Element {
    rsx! {
        div { class: "tw:min-h-[260px]",
            {strip_frame(390, five_sessions(), true, true)}
        }
    }
}

fn sim_session(project: &str, lensed: bool, status: UiChromeSessionStatus) -> UiChromeSession {
    UiChromeSession {
        key: "sim".to_string(),
        name: "Simulator".to_string(),
        sim: true,
        transport: String::new(),
        status,
        lensed,
        target: UiChromeSessionTarget::Sim {
            project_key: Some(project.to_string()),
        },
    }
}

fn device_session(
    uid: &str,
    name: &str,
    lensed: bool,
    status: UiChromeSessionStatus,
) -> UiChromeSession {
    UiChromeSession {
        key: uid.to_string(),
        name: name.to_string(),
        sim: false,
        transport: "USB".to_string(),
        status,
        lensed,
        target: UiChromeSessionTarget::Device {
            uid: Some(uid.to_string()),
        },
    }
}

fn three_sessions() -> Vec<UiChromeSession> {
    vec![
        sim_session("plasma", false, UiChromeSessionStatus::Empty),
        device_session("devb2", "Desk C6", true, UiChromeSessionStatus::Run),
        device_session(
            "devc3",
            "Dome quad",
            false,
            UiChromeSessionStatus::Attention,
        ),
    ]
}

fn five_sessions() -> Vec<UiChromeSession> {
    vec![
        sim_session("zook-dome", true, UiChromeSessionStatus::Run),
        device_session("devb2", "Desk C6", false, UiChromeSessionStatus::Run),
        device_session(
            "devc3",
            "Dome quad",
            false,
            UiChromeSessionStatus::Attention,
        ),
        device_session("devd4", "Stair strip", false, UiChromeSessionStatus::Empty),
        device_session("deve5", "Porch sign", false, UiChromeSessionStatus::Run),
    ]
}

fn strip_frame(
    width: u32,
    sessions: Vec<UiChromeSession>,
    on_editor: bool,
    flyout_open: bool,
) -> Element {
    rsx! {
        div {
            class: "tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            style: "max-width: {width}px;",
            SiteChrome {
                section: SiteSection::Devices,
                sessions,
                on_editor,
                overflow_menu_open: flyout_open,
                VersionChipPreview { chip: branch_chip() }
            }
        }
    }
}

#[story(
    label = "⋯ menu on a project route",
    description = "The overflow menu while a project is open: a Project group leads it (spike project-share §5, ruling G4). \"Sharing & access…\" opens the same panel the Share pill does, and \"Archive project\" is a QUIET row, not a red one — archiving is reversible and nothing is deleted, so dressing it as destructive would teach people to fear the wrong control. There is no Delete forever anywhere in this menu."
)]
pub(crate) fn overflow_menu_project_group() -> Element {
    rsx! {
        // A NARROW frame, like the `narrow` story: `overflow_menu_open`
        // opens the narrow mount of the menu, and at a wide container that
        // mount is `display: none` — an open popover anchored to a hidden
        // trigger has nothing to measure.
        div {
            class: "tw:min-h-[460px] tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            style: "max-width: 390px;",
            SiteChrome {
                section: SiteSection::Session,
                overflow_menu_open: true,
                project_menu: Some(ChromeProjectMenu {
                    on_share: EventHandler::new(|()| {}),
                    on_archive: EventHandler::new(|()| {}),
                }),
                VersionChipPreview { chip: branch_chip() }
            }
        }
    }
}

#[story(
    description = "The header project chip (D8, Google-Docs pattern) across states, beside the lensed session chip: clean (quiet i), unsaved (yellow wash, pencil, amber count), failed edits (red), and the unsaved chip at a narrow bar — the chip is UNGATED, one mount at every width, so project state never disappears."
)]
pub(crate) fn project_chip_states() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2",
            {chip_frame(1000, chip_content(0, 0), false)}
            {chip_frame(1000, chip_content(3, 0), false)}
            {chip_frame(1000, chip_content(1, 2), false)}
            {chip_frame(520, chip_content(3, 0), false)}
        }
    }
}

#[story(
    label = "Project chip popover open",
    description = "The chip's detail popup — the SAME ProjectDetailSections the pane's [i] renders (identity + status word, Project settings, Share, pending edits with per-entry revert, stats) — open from the yellow unsaved chip. Unlike the ⋯ menu there is no narrow-frame caveat: the chip has ONE ungated mount, so the popover always has a visible trigger to measure."
)]
pub(crate) fn project_chip_popover_open() -> Element {
    rsx! {
        div { class: "tw:min-h-[560px]",
            {chip_frame(700, chip_content(2, 0), true)}
        }
    }
}

/// The chip stories' content: the shared editor fixture with the dirty
/// counts stamped, gathered exactly the way `web_app` builds the chip.
fn chip_content(persisted: usize, failed: usize) -> ProjectDetailContent {
    let mut editor = project_editor_fixture(ProjectSyncPhase::Ready);
    editor.dirty = DirtySummary { persisted, failed };
    ProjectDetailContent::new(&editor, UiStatus::good("Ready"))
}

fn chip_frame(width: u32, content: ProjectDetailContent, popover_open: bool) -> Element {
    rsx! {
        div {
            class: "tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            style: "max-width: {width}px;",
            SiteChrome {
                section: SiteSection::Session,
                on_editor: true,
                sessions: vec![sim_session("mini-dome", true, UiChromeSessionStatus::Run)],
                session_control: Some(ChromeSessionControl {
                    session: sim_control(),
                    project: Some(content),
                    on_action: EventHandler::new(|_| {}),
                    initially_open: popover_open,
                }),
                VersionChipPreview { chip: branch_chip() }
            }
        }
    }
}

/// The control stories' session: THE sim, naming the board it simulates
/// (ruling 8.1). P5 grows the full state matrix — this keeps the existing
/// chip stories rendering the control that replaced the chip.
fn sim_control() -> UiChromeSessionControl {
    UiChromeSessionControl {
        key: "sim".to_string(),
        sim: true,
        name: "Sim".to_string(),
        board: Some("ESP32-C6".to_string()),
        status: UiChromeSessionStatus::Run,
        busy: None,
        stat_line: Some("60 fps · 217 lamps".to_string()),
    }
}

#[story(
    description = "The brand: wide lockup, small mark-only form, and the mark at favicon/bar/hero sizes."
)]
pub(crate) fn logo_sizes() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-4 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4",
            LogoLockup {}
            LogoLockup { size: 34 }
            LogoLockup { compact: true }
            div { class: "tw:flex tw:items-center tw:gap-5 tw:text-heading",
                LogoMark { size: 16 }
                LogoMark { size: 22 }
                LogoMark { size: 56 }
            }
        }
    }
}

fn branch_chip() -> BuildChip {
    BuildChip::Branch {
        name: "top-bar-ux-ace649".to_string(),
        dirty: false,
    }
}

fn frame(width: u32, section: SiteSection, chip: BuildChip, menu_open: bool) -> Element {
    rsx! {
        div {
            class: "tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            style: "max-width: {width}px;",
            SiteChrome { section, overflow_menu_open: menu_open,
                VersionChipPreview { chip }
            }
        }
    }
}
