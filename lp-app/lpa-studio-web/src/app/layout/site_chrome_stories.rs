//! Stories for the shared site chrome.
//!
//! The chrome is presentational (its standalone hashchange listener is
//! route-guarded and never installs under the story book), so these mount
//! it directly per section, with the version-chip preview standing in for
//! the live right-cluster children. The narrow story pins the bar at phone
//! width — the brand word hides, the chip truncates, the tabs survive.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::layout::site_chrome::{SiteChrome, SiteSection};
use crate::app::layout::version_badge::{BuildChip, VersionChipPreview};
use crate::base::{LogoLockup, LogoMark};

#[story(description = "Devices section active, dev-branch chip on the right.")]
pub(crate) fn studio_active() -> Element {
    frame(1000, SiteSection::Devices, branch_chip())
}

#[story(description = "Boards section active.")]
pub(crate) fn boards_active() -> Element {
    frame(1000, SiteSection::Boards, branch_chip())
}

#[story(description = "Docs section active, deployed version chip.")]
pub(crate) fn docs_active() -> Element {
    frame(
        1000,
        SiteSection::Docs,
        BuildChip::Release("2026.08.01-2".to_string()),
    )
}

#[story(
    label = "Narrow (390px)",
    description = "Phone width: brand word hidden, long dirty branch left-ellipsized, tabs intact."
)]
pub(crate) fn narrow() -> Element {
    frame(
        390,
        SiteSection::Devices,
        BuildChip::Branch {
            name: "claude/settings-provenance-rework-b6680f".to_string(),
            dirty: true,
        },
    )
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

fn frame(width: u32, section: SiteSection, chip: BuildChip) -> Element {
    rsx! {
        div {
            class: "tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            style: "max-width: {width}px;",
            SiteChrome { section,
                VersionChipPreview { chip }
            }
        }
    }
}
