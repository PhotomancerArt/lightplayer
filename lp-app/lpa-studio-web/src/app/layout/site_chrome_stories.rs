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
    ControllerId, DirtySummary, ProjectController, ProjectEditorView, ProjectNodeTreeView,
    ProjectOp, ProjectSyncPhase, UiAction, UiChromeSessionControl, UiChromeSessionStatus,
    UiPaneAction, UiStatus,
};
use lpa_studio_web_story_macros::story;

use crate::app::layout::session_control::ChromeSessionControl;
use crate::app::layout::site_chrome::{
    ChromeModeToggle, ChromeProjectMenu, SiteChrome, SiteSection,
};
use crate::app::layout::version_badge::{BuildChip, VersionChipPreview};
use crate::app::project::ProjectDetailContent;
use crate::app::story_fixtures::{project_editor_summary, project_synced_metrics};
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
    description = "The ONE merged \u{22ef} menu (G3 ruling): Sections and Tools groups in a single popup; active section marked (Docs is current here). Plain route, so the primary tabs stay inline and out of the menu."
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

#[story(
    label = "Lens bar — the narrow ladder",
    description = "The crowded bar (session control + Save/↺ + mode toggles aboard) folds EARLIER than the plain one — the cut is where things stop fitting, and this bar stops fitting ~220px sooner. Top to bottom: ≥900 everything; <900 the world nav retreats to ⋯, Patch/Play and Share go icon-only, the version chip hides, the device name folds; <680 the brand word yields with the ↺; <560 the phone bar — Devices/Projects become ⋯ rows, Patch a menu row, and the project name is the one flexible truncator. Nothing overlaps or wraps at any rung."
)]
pub(crate) fn lens_bar_ladder() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2",
            for width in [1040u32, 840, 640, 390] {
                {lens_frame(width, false)}
            }
        }
    }
}

#[story(
    label = "Lens bar phone ⋯ menu",
    description = "The phone rung's ⋯ menu (crowded bar <560): the Project group leads, the folded Patch toggle rides in as a mode row, and the Sections group carries ALL FIVE sections — Devices and Projects join the world's three, because the phone bar keeps no inline tabs at all."
)]
pub(crate) fn lens_bar_phone_menu_open() -> Element {
    rsx! {
        div { class: "tw:min-h-[560px]",
            {lens_frame(390, true)}
        }
    }
}

/// One crowded-bar frame: the session·project control (dirty — Save and ↺
/// materialized), both mode toggles, the project ⋯ group, and the version
/// chip behind the same fold the shell gives it.
fn lens_frame(width: u32, menu_open: bool) -> Element {
    rsx! {
        div {
            class: "tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            style: "max-width: {width}px;",
            SiteChrome {
                section: SiteSection::Session,
                overflow_menu_open: menu_open,
                session_control: Some(story_session_control()),
                patch_toggle: Some(ChromeModeToggle { href: "#patch".to_string(), active: false }),
                play_toggle: Some(ChromeModeToggle { href: "#play".to_string(), active: false }),
                project_menu: Some(ChromeProjectMenu {
                    on_share: EventHandler::new(|()| {}),
                    on_archive: EventHandler::new(|()| {}),
                }),
                // The version chip behind the crowded bar's <900 fold —
                // the same wrapper the shell mounts it in.
                span { class: "tw:hidden tw:@min-[900px]:flex",
                    VersionChipPreview { chip: branch_chip() }
                }
            }
        }
    }
}

/// THE session with a dirty project on it: the sim (board suffix per
/// ruling 8.1) running "Mini Dome" with one unsaved arrange edit, so the
/// control shows the full anatomy — glyph, dot, name·board, amber project
/// segment, count pill, and the Save/↺ pair beside it.
fn story_session_control() -> ChromeSessionControl {
    let session = UiChromeSessionControl {
        key: "story-sim".to_string(),
        sim: true,
        name: "Sim".to_string(),
        board: Some("ESP32-C6".to_string()),
        status: UiChromeSessionStatus::Run,
        busy: None,
        stat_line: Some("60 fps · 177 lamps".to_string()),
    };
    let action = |icon: &str, op: ProjectOp| {
        UiPaneAction::new(
            icon,
            UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op),
        )
    };
    let view = ProjectEditorView::new(
        "mini-dome",
        1,
        project_editor_summary(ProjectSyncPhase::Ready),
        project_synced_metrics(),
        ProjectNodeTreeView::new(Vec::new(), 0),
        Vec::new(),
    )
    .with_project_name("Mini Dome")
    .with_dirty(DirtySummary {
        persisted: 1,
        failed: 0,
    })
    .with_header_actions(vec![
        action("save", ProjectOp::SaveOverlay),
        action("revert", ProjectOp::RevertAllEdits),
    ]);
    ChromeSessionControl {
        session,
        project: Some(ProjectDetailContent::new(&view, UiStatus::good("Project"))),
        on_action: EventHandler::new(|_| {}),
        initially_open: false,
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
