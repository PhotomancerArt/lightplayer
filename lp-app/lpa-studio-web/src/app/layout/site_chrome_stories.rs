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
use lpa_studio_web_story_macros::story;

use crate::app::layout::site_chrome::{ChromeProjectMenu, SiteChrome, SiteSection};
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
