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
use lpa_studio_core::{UiChromeSession, UiChromeSessionStatus, UiChromeSessionTarget};
use lpa_studio_web_story_macros::story;

use crate::app::layout::site_chrome::{SiteChrome, SiteSection};
use crate::app::layout::version_badge::{BuildChip, VersionChipPreview};
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
    description = "The folded secondary family: same items, active state marked in the menu (Docs is current here)."
)]
pub(crate) fn narrow_menu_open() -> Element {
    rsx! {
        div { class: "tw:min-h-[220px]",
            {frame(390, SiteSection::Docs, branch_chip(), true)}
        }
    }
}

#[story(
    description = "The session strip (D15/D16) across counts and widths: one lensed-here sim; three sessions with the lens elsewhere (washed chip) and a behind device; five sessions overflowing into +n; a long name ellipsizing. Wide frames show chips, the md frame folds to two + n, the narrow frame is the count chip."
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
                    device_session("dev_a1", "the-remarkably-long-device-name-from-the-hall", true, UiChromeSessionStatus::Run),
                    sim_session("2026-08-05-2053-xiao-esp32-c6", false, UiChromeSessionStatus::Attention),
                ],
                true,
                false,
            )}
        }
    }
}

#[story(
    label = "Session flyout open",
    description = "The narrow count chip's flyout: every session in menu grammar, here-row bold, statuses marked."
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
        device_session("dev_b2", "Desk C6", true, UiChromeSessionStatus::Run),
        device_session(
            "dev_c3",
            "Dome quad",
            false,
            UiChromeSessionStatus::Attention,
        ),
    ]
}

fn five_sessions() -> Vec<UiChromeSession> {
    vec![
        sim_session("zook-dome", true, UiChromeSessionStatus::Run),
        device_session("dev_b2", "Desk C6", false, UiChromeSessionStatus::Run),
        device_session(
            "dev_c3",
            "Dome quad",
            false,
            UiChromeSessionStatus::Attention,
        ),
        device_session("dev_d4", "Stair strip", false, UiChromeSessionStatus::Empty),
        device_session("dev_e5", "Porch sign", false, UiChromeSessionStatus::Run),
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
                session_flyout_open: flyout_open,
                VersionChipPreview { chip: branch_chip() }
            }
        }
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

fn frame(width: u32, section: SiteSection, chip: BuildChip, nav_menu_open: bool) -> Element {
    rsx! {
        div {
            class: "tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            style: "max-width: {width}px;",
            SiteChrome { section, nav_menu_open,
                VersionChipPreview { chip }
            }
        }
    }
}
