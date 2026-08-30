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
    ControllerId, DirtySummary, ProjectController, ProjectNodeAddress, ProjectOp,
    ProjectSlotAddress, ProjectSlotRoot, ProjectSyncPhase, SlotEditOp, SlotPath, UiAction,
    UiChromeSessionControl, UiChromeSessionStatus, UiPaneAction, UiPendingEdit, UiPendingEditKind,
    UiPendingEditPhase, UiStatus,
};
use lpa_studio_web_story_macros::story;

use crate::app::layout::session_control::ChromeSessionControl;
use crate::app::layout::site_chrome::{
    ChromeModeToggle, ChromeProjectMenu, SiteChrome, SiteSection,
};
use crate::app::layout::version_badge::{BuildChip, VersionChipPreview};
use crate::app::project::ProjectDetailContent;
use crate::app::story_fixtures::project_editor_fixture;
use crate::base::{LogoLockup, LogoMark};

#[story(
    description = "Wide bar, one row per section: primary family (Devices, Projects) full weight by the brand; secondary family (Boards, Docs) lighter on the right; Home lights no tab — the logo is its affordance."
)]
pub(crate) fn sections_active() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2",
            for section in [
                SiteSection::Devices,
                SiteSection::Projects,
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
    description = "The header session·project control (spike concept B) across every project state — saved / unsaved / failed / syncing — crossed with device kind: the sim naming its board, a boardless sim (bare \"Sim\", ruling Q6), and hardware (the device name IS the board, no suffix). Unsaved wears the amber wash with Save/↺ standing beside the lockup as SIBLING buttons (G1 round-2: inspect and act are different surfaces); failed here is the failed-ONLY edge (persisted=0, failed>0) — red wash, no count pill, no Save/↺, because the header only offers actions while persisted edits are pending (see docs/debt/failed-only-asset-edit-header-blindness.md); syncing shows the busy dot with nothing dirty."
)]
pub(crate) fn control_states() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2",
            for (kind_label, control) in [
                ("Sim · board", sim_control(Some("ESP32-C6"))),
                ("Sim · bare", sim_control(None)),
                ("Hardware", hardware_control()),
            ]
            {
                div { class: "tw:grid tw:gap-1",
                    span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-wide tw:text-dim-foreground",
                        "{kind_label}"
                    }
                    {control_row(1000, control.clone(), Some(control_content(0, 0, UiStatus::good("Ready"))), false)}
                    {control_row(1000, control.clone(), Some(control_content(3, 0, UiStatus::good("Ready"))), false)}
                    {control_row(1000, control.clone(), Some(control_content(0, 2, UiStatus::error("Sync issue"))), false)}
                    {control_row(1000, control, Some(control_content(0, 0, UiStatus::working("Syncing"))), false)}
                }
            }
        }
    }
}

#[story(
    label = "Panel open — sim with board",
    description = "The panel open on a clean sim session: the device zone (kind glyph, name, run word, \"simulating ESP32-C6\" stat line) over the project zone's sections, then the \"this tab is the session\" hint. No switcher (R8-1 ruling) — there is nothing to switch to."
)]
pub(crate) fn control_panel_open_sim() -> Element {
    rsx! {
        div { class: "tw:min-h-[560px]",
            {control_row(700, sim_control(Some("ESP32-C6")), Some(control_content(0, 0, UiStatus::good("Ready"))), true)}
        }
    }
}

#[story(
    label = "Panel open — dirty (hardware, unsaved)",
    description = "The panel open on a dirty HARDWARE session — the header control's unsaved-list edge case and the hardware-unsaved edge state at once: the project zone's \"Unsaved (persisted)\" section lists the pending edits with per-entry revert, matching the amber count the closed lockup shows."
)]
pub(crate) fn control_panel_open_dirty() -> Element {
    rsx! {
        div { class: "tw:min-h-[680px]",
            {control_row(700, hardware_control(), Some(dirty_content()), true)}
        }
    }
}

#[story(
    label = "Fold — md (820px, device name folds)",
    description = "Below the 900px cut the device segment keeps only its kind glyph and status dot — the name and board suffix (which ride together) drop first because the glyph+dot pair is the two facts that survive a squeeze; Save/↺ still fit at this width."
)]
pub(crate) fn control_fold_md() -> Element {
    control_row(
        820,
        sim_control(Some("ESP32-C6")),
        Some(control_content(3, 0, UiStatus::good("Ready"))),
        false,
    )
}

#[story(
    label = "Fold — sm (600px, no ↺)",
    description = "Below the 680px cut ↺ retreats into the panel's per-entry reverts — the destructive half of the pair is the one to lose first (Save stays, it is the safe click). The device name is long since gone at this width too."
)]
pub(crate) fn control_fold_sm() -> Element {
    control_row(
        600,
        sim_control(Some("ESP32-C6")),
        Some(control_content(3, 0, UiStatus::good("Ready"))),
        false,
    )
}

#[story(
    label = "Studio mode — Docs/Boards \u{2197}",
    description = "A lens route fronted (single-session policy): Boards and Docs carry the \u{2197} new-tab mark in the secondary family, because from here they open a NEW tab rather than ending the session (ruling R8-3, amended 8.1)."
)]
pub(crate) fn studio_mode_bar() -> Element {
    control_row(
        1000,
        sim_control(Some("ESP32-C6")),
        Some(control_content(0, 0, UiStatus::good("Ready"))),
        false,
    )
}

#[story(
    label = "Edge — connected, no project",
    description = "A connected session with nothing loaded (spike \u{a7}5): the project segment reads an honest \"no project\" in italics — no invented name, no state glyph to read a state off — and the device dot is hollow (D16 connected-empty)."
)]
pub(crate) fn control_connected_empty() -> Element {
    control_row(700, hardware_empty_control(), None, false)
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
                // The shared P5 fixtures: the board-naming sim with one
                // unsaved persisted edit, so Save/↺ are aboard.
                session_control: Some(ChromeSessionControl {
                    session: sim_control(Some("ESP32-C6")),
                    project: Some(control_content(1, 0, UiStatus::good("Ready"))),
                    on_action: EventHandler::new(|_| {}),
                    initially_open: false,
                    example: false,
                }),
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

/// One control frame: `SectionSession` (studio mode) at a fixed width, so
/// the folds trigger off the FRAME rather than the story viewport — the
/// same technique `frame`/`chip_frame` used for the retired session strip
/// and project-chip stories.
fn control_row(
    width: u32,
    session: UiChromeSessionControl,
    project: Option<ProjectDetailContent>,
    initially_open: bool,
) -> Element {
    rsx! {
        div {
            class: "tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            style: "max-width: {width}px;",
            SiteChrome {
                section: SiteSection::Session,
                session_control: Some(ChromeSessionControl {
                    session,
                    project,
                    on_action: EventHandler::new(|_| {}),
                    initially_open,
                    example: false,
                }),
                VersionChipPreview { chip: branch_chip() }
            }
        }
    }
}

/// THE sim session, naming the board it simulates (ruling 8.1) — or bare
/// "Sim" when the project names no board (ruling Q6).
fn sim_control(board: Option<&str>) -> UiChromeSessionControl {
    UiChromeSessionControl {
        key: "sim".to_string(),
        sim: true,
        name: "Sim".to_string(),
        board: board.map(str::to_string),
        status: UiChromeSessionStatus::Run,
        busy: None,
        stat_line: board.map(|_| "60 fps · 217 lamps".to_string()),
    }
}

/// A connected, running hardware session — the device's own name IS the
/// board, so it never wears a suffix (only the sim does, ruling 8.1).
fn hardware_control() -> UiChromeSessionControl {
    UiChromeSessionControl {
        key: "dev_c6f0".to_string(),
        sim: false,
        name: "Garage dome".to_string(),
        board: None,
        status: UiChromeSessionStatus::Run,
        busy: None,
        stat_line: Some("USB · 217 lamps".to_string()),
    }
}

/// A connected hardware session with nothing loaded — the honest-empty
/// project edge (spike §5): hollow dot, no project segment content beyond
/// "no project".
fn hardware_empty_control() -> UiChromeSessionControl {
    UiChromeSessionControl {
        status: UiChromeSessionStatus::Empty,
        stat_line: None,
        ..hardware_control()
    }
}

/// The control stories' project content: the shared editor fixture with the
/// dirty counts and the matching header actions stamped — the SAME gate the
/// controller's `project_header_actions` applies (persisted > 0, never
/// failed alone), so a failed-only row here renders exactly the header's
/// real blind spot
/// (`docs/debt/failed-only-asset-edit-header-blindness.md`).
fn control_content(persisted: usize, failed: usize, status: UiStatus) -> ProjectDetailContent {
    let mut editor = project_editor_fixture(ProjectSyncPhase::Ready);
    editor.dirty = DirtySummary { persisted, failed };
    editor.header_actions = save_revert_actions(persisted);
    ProjectDetailContent::new(&editor, status)
}

/// The dirty-list-open story's content: two persisted edits, both listed
/// (not just counted) so the popover's "Unsaved (persisted)" section shows
/// real rows with per-entry revert, matching the closed lockup's count.
fn dirty_content() -> ProjectDetailContent {
    let mut editor = project_editor_fixture(ProjectSyncPhase::Ready);
    editor.dirty = DirtySummary {
        persisted: 2,
        failed: 0,
    };
    editor.header_actions = save_revert_actions(2);
    editor.pending_edits = vec![
        pending_edit("Orbit shader", "brightness", "0.82"),
        pending_edit("Sunrise palette", "entries[dusk]", "#ff7a3d"),
    ];
    ProjectDetailContent::new(&editor, UiStatus::good("Ready"))
}

/// Save / Revert-to-saved, exactly as the controller's `project_header_actions`
/// mints them — present only while persisted edits are pending, never for a
/// failed-only project (the header blindness this control inherited).
fn save_revert_actions(persisted: usize) -> Vec<UiPaneAction> {
    if persisted == 0 {
        return Vec::new();
    }
    vec![
        UiPaneAction::new("save", project_action(ProjectOp::SaveOverlay)),
        UiPaneAction::new(
            "revert",
            project_action(ProjectOp::RevertAllEdits).with_label("Revert to saved"),
        ),
    ]
}

/// One change-list entry with the same per-entry revert action the project
/// controller produces (mirrors `project_pane_stories::pending_edit`).
fn pending_edit(node_label: &str, path: &str, value_display: &str) -> UiPendingEdit {
    let address = ProjectSlotAddress::new(
        ProjectNodeAddress::parse("/demo.module/orbit.shader").expect("valid story node address"),
        ProjectSlotRoot::def(),
        SlotPath::parse(path).expect("valid story slot path"),
    );
    let node_path = address.node.to_string();
    UiPendingEdit {
        node_label: node_label.to_string(),
        node_path,
        slot_path_display: path.to_string(),
        kind: UiPendingEditKind::Assign {
            value_display: value_display.to_string(),
        },
        old_value: None,
        phase: UiPendingEditPhase::Persisted,
        revert: Some(UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            SlotEditOp::Revert { address },
        )),
    }
}

/// An action dispatched to the project controller itself — the same helper
/// `ProjectController::project_header_actions` uses internally.
fn project_action(op: ProjectOp) -> UiAction {
    UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op)
}
