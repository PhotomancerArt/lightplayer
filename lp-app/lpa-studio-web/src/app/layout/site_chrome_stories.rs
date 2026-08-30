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

use crate::app::layout::session_control::{ChromeSessionControl, ControlSegment};
use crate::app::layout::site_chrome::{
    ChromeModeToggle, ChromeProjectMenu, SiteChrome, SiteSection,
};
use crate::app::layout::version_badge::{BuildChip, VersionChipPreview};
use crate::app::project::ProjectDetailContent;
use crate::app::share::ProjectRelationship;
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
    description = "The three-segment session·project control (spike \u{a7}2-E) across every project state — saved / unsaved / failed / syncing — crossed with device kind: the sim naming its board, a boardless sim (bare \"Sim\", ruling Q6), and hardware (the device name IS the board, no suffix). The dirty wash lives on the CHANGES segment now (D8), so the project segment stays neutral and nothing is announced twice; failed here is the failed-ONLY edge (persisted=0, failed>0) — red changes bed with a red count, and no Save, because the controller only publishes actions while persisted edits are pending (see docs/debt/failed-only-asset-edit-header-blindness.md); syncing shows the busy dot with a quiet \u{2713}."
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
                    {control_row(1000, control.clone(), Some(control_content(0, 0, UiStatus::good("Ready"))), None)}
                    {control_row(1000, control.clone(), Some(control_content(3, 0, UiStatus::good("Ready"))), None)}
                    {control_row(1000, control.clone(), Some(control_content(0, 2, UiStatus::error("Sync issue"))), None)}
                    {control_row(1000, control, Some(control_content(0, 0, UiStatus::working("Syncing"))), None)}
                }
            }
        }
    }
}

#[story(
    label = "Faces — the relationship, clean and dirty",
    description = "The project segment's face across the standings the bar can reach today, clean over dirty. Example is the pristine transient view (the accent \"example\" pill it replaces is gone — the face is neutral by D12, and it is the one thing in the bar that says whose document this is); Private is a library project with no roster answer, which is what every saved project reads as until P3 wires the fetch. Dirty adds the amber count on the CHANGES segment and the Save sibling; the project segment does not change, because ownership and dirtiness are different questions."
)]
pub(crate) fn control_relationship_faces() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2",
            for (face_label, relationship) in [
                ("Example", ProjectRelationship::Example),
                ("Private", ProjectRelationship::MineLocal),
            ]
            {
                div { class: "tw:grid tw:gap-1",
                    span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-wide tw:text-dim-foreground",
                        "{face_label}"
                    }
                    {control_row_as(1000, sim_control(Some("ESP32-C6")), Some(control_content(0, 0, UiStatus::good("Ready"))), relationship, None)}
                    {control_row_as(1000, sim_control(Some("ESP32-C6")), Some(control_content(3, 0, UiStatus::good("Ready"))), relationship, None)}
                }
            }
        }
    }
}

#[story(
    label = "Device popover open — sim with board",
    description = "The DEVICE segment's popover: what is running. Kind glyph, name, run word, the \"simulating ESP32-C6\" stat line, and the \"this tab is the session\" hint that names what leaving actually ends. No switcher (R8-1 ruling) — there is nothing to switch to. This is the declared landing zone for the desktop device panel's facts when it retires (D13)."
)]
pub(crate) fn control_device_popover_open() -> Element {
    rsx! {
        div { class: "tw:min-h-[420px]",
            {control_row(700, sim_control(Some("ESP32-C6")), Some(control_content(0, 0, UiStatus::good("Ready"))), Some(ControlSegment::Device))}
        }
    }
}

#[story(
    label = "Project popover open — the document",
    description = "The PROJECT segment's popover, still mounting today's detail sections (identity + status word, project settings, share, stats). P3 replaces this content with the fixed relationship skeleton; the segment, its face, and the popover it opens are what this phase settles."
)]
pub(crate) fn control_project_popover_open() -> Element {
    rsx! {
        div { class: "tw:min-h-[560px]",
            {control_row_as(700, sim_control(Some("ESP32-C6")), Some(control_content(0, 0, UiStatus::good("Ready"))), ProjectRelationship::Example, Some(ControlSegment::Project))}
        }
    }
}

#[story(
    label = "Changes popup open — dirty (hardware)",
    description = "The CHANGES segment's popup on a dirty hardware session — the third question, in its own home: the labeled pending edits with per-entry revert, the pending facts, an honest receipt (\"already live in this session\" — it never claims cloud state it cannot see), and the controller's own Save plus the Revert-all that retired the \u{21ba} sibling. The counts here are the same ones the closed segment shows."
)]
pub(crate) fn control_changes_popup_open() -> Element {
    rsx! {
        div { class: "tw:min-h-[560px]",
            {control_row(700, hardware_control(), Some(dirty_content()), Some(ControlSegment::Changes))}
        }
    }
}

#[story(
    label = "Changes popup open — clean",
    description = "The clean half of the changes popup: one plain line. The segment is always present (a quiet \u{2713}), so \"nothing pending\" is a state you can click into and confirm rather than an absence you have to infer."
)]
pub(crate) fn control_changes_popup_clean() -> Element {
    rsx! {
        div { class: "tw:min-h-[260px]",
            {control_row(700, sim_control(Some("ESP32-C6")), Some(control_content(0, 0, UiStatus::good("Ready"))), Some(ControlSegment::Changes))}
        }
    }
}

#[story(
    label = "Fold — md (820px, words fold)",
    description = "Below the 900px cut the device segment keeps only its kind glyph and status dot, and the relationship face drops its WORD for its glyph alone (spike \u{a7}3 vocabulary V3) — the same cut, because both are words explaining a glyph that already says it. The changes segment is untouched at this width."
)]
pub(crate) fn control_fold_md() -> Element {
    control_row_as(
        820,
        sim_control(Some("ESP32-C6")),
        Some(control_content(3, 0, UiStatus::good("Ready"))),
        ProjectRelationship::Example,
        None,
    )
}

#[story(
    label = "Fold — sm (520px, changes go count-only)",
    description = "Below the 560px cut the segment padding tightens and the changes segment spends its \u{270e} for the count — a count with no glyph still counts; a glyph with no count says nothing. Save stays (it is the safe click, and the only direct act left in the bar); the project name is the one flexible truncator."
)]
pub(crate) fn control_fold_sm() -> Element {
    control_row_as(
        520,
        sim_control(Some("ESP32-C6")),
        Some(control_content(3, 0, UiStatus::good("Ready"))),
        ProjectRelationship::Example,
        None,
    )
}

#[story(
    label = "Studio mode — Docs/Boards \u{2197}",
    description = "A lens route fronted (single-session policy): Boards and Docs carry the \u{2197} new-tab mark in the secondary family, because from here they open a NEW tab rather than ending the session (ruling R8-3, amended 8.1) — Explore stays a plain link, a real exit."
)]
pub(crate) fn studio_mode_bar() -> Element {
    control_row(
        1000,
        sim_control(Some("ESP32-C6")),
        Some(control_content(0, 0, UiStatus::good("Ready"))),
        None,
    )
}

#[story(
    label = "Edge — connected, no project",
    description = "A connected session with nothing loaded (spike \u{a7}5): the project segment reads an honest \"no project\" in italics — no invented name, no state glyph and no relationship face to read a standing off — the device dot is hollow (D16 connected-empty), and the CHANGES segment is absent entirely, because with no document open there is nothing that could be in flight."
)]
pub(crate) fn control_connected_empty() -> Element {
    control_row(700, hardware_empty_control(), None, None)
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
    description = "The crowded bar (three-segment session control + Save + mode toggles aboard) folds EARLIER than the plain one — the cut is where things stop fitting, and this bar stops fitting ~220px sooner. Top to bottom: ≥900 everything; <900 the world nav retreats to ⋯, Patch/Play and Share go icon-only, the version chip hides, the device name and the relationship word fold to their glyphs; <680 the brand word yields; <560 the phone bar — Devices/Projects become ⋯ rows, Patch a menu row, the changes segment goes count-only, and the project name is the one flexible truncator. Nothing overlaps or wraps at any rung."
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

/// One crowded-bar frame: the session·project control (dirty — the amber
/// count on the changes segment and the Save sibling materialized), both
/// mode toggles, the project ⋯ group, and the version chip behind the same
/// fold the shell gives it.
fn lens_frame(width: u32, menu_open: bool) -> Element {
    rsx! {
        div {
            class: "tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            style: "max-width: {width}px;",
            SiteChrome {
                section: SiteSection::Session,
                overflow_menu_open: menu_open,
                // The shared P5 fixtures: the board-naming sim with one
                // unsaved persisted edit, so Save is aboard.
                session_control: Some(ChromeSessionControl {
                    session: sim_control(Some("ESP32-C6")),
                    project: Some(control_content(1, 0, UiStatus::good("Ready"))),
                    relationship: ProjectRelationship::MineLocal,
                    on_action: EventHandler::new(|_| {}),
                    initially_open: None,
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
    initially_open: Option<ControlSegment>,
) -> Element {
    control_row_as(
        width,
        session,
        project,
        ProjectRelationship::MineLocal,
        initially_open,
    )
}

/// The same frame with the relationship named — the project segment's face
/// is a prop now, so a story picks the standing it wants to show.
fn control_row_as(
    width: u32,
    session: UiChromeSessionControl,
    project: Option<ProjectDetailContent>,
    relationship: ProjectRelationship,
    initially_open: Option<ControlSegment>,
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
                    relationship,
                    on_action: EventHandler::new(|_| {}),
                    initially_open,
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
/// real rows with per-entry revert, matching the closed changes segment's
/// count.
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
