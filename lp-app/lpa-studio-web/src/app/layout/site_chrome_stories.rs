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
    UiChromeSessionControl, UiChromeSessionStatus, UiHistoryKind, UiPaneAction, UiPendingEdit,
    UiPendingEditKind, UiPendingEditPhase, UiProjectHistory, UiProjectHistoryEntry, UiStatus,
};
use lpa_studio_web_story_macros::story;

use crate::app::layout::session_control::{
    ChromeSessionControl, ControlSegment, ProjectPopoverInputs, SessionChangesPanel,
};
use crate::app::layout::site_chrome::{
    ChromeModeToggle, ChromeProjectMenu, SiteChrome, SiteSection,
};
use crate::app::layout::version_badge::{BuildChip, VersionChipPreview};
use crate::app::project::ProjectDetailContent;
use crate::app::share::ProjectRelationship;
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
    description = "The overflow menu while a project is open: a Project group leads it (spike project-share §5, ruling G4). It holds exactly one row now — \"Sharing & access…\" retired with the Share pill at relationship-control P5, because the bar's PROJECT segment is the share door and a door in the bar does not also belong behind a menu. \"Archive project\" is a QUIET row, not a red one — archiving is reversible and nothing is deleted, so dressing it as destructive would teach people to fear the wrong control. There is no Delete forever anywhere in this menu."
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
                    on_archive: EventHandler::new(|()| {}),
                }),
                VersionChipPreview { chip: branch_chip() }
            }
        }
    }
}

#[story(
    description = "The three-segment session·project control (spike \u{a7}2-E) across every project state — saved / unsaved / failed / syncing — crossed with device rows: the sim naming its board, a boardless sim (bare \"Sim\", ruling Q6), and the hardware stand-in (a boardless sim until the rebuilt device model returns). The dirty wash lives on the CHANGES segment now (D8), so the project segment stays neutral and nothing is announced twice; failed here is the failed-ONLY edge (persisted=0, failed>0) — red changes bed with a red count, and no Save, because the controller only publishes actions while persisted edits are pending (see docs/debt/failed-only-asset-edit-header-blindness.md); syncing shows the busy dot with a quiet \u{2713}."
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
    label = "Faces — all five relationships, clean and dirty",
    description = "The project segment's face for every state the derivation can produce, clean over dirty. Example is the pristine transient view (the accent \"example\" pill it replaces is gone — the face is neutral by D12, and it is the one thing in the bar that says whose document this is); Private is a library project the service has not answered a roster for; Shared is the same project once it has (D12 keeps it neutral — no status-blue for published); Member and Viewing are somebody else's document, with and without write. Dirty adds the amber count on the CHANGES segment and the Save sibling; the project segment does not change in either direction, because ownership and dirtiness are different questions."
)]
pub(crate) fn control_relationship_faces() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2",
            for (face_label, relationship) in [
                ("Example", ProjectRelationship::Example),
                ("Private", ProjectRelationship::MineLocal),
                ("Shared", ProjectRelationship::MinePublished),
                ("Member", ProjectRelationship::MemberOfSomeoneElses),
                ("Viewing", ProjectRelationship::ViewingSomeoneElses),
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
    description = "The PROJECT section of the shared panel in the real chrome: the identity skeleton (Where \u{2192} Access \u{2192} action row; history moved to the changes section at D14) hanging off the Example face, with the merged outline anchored on the WHOLE shell \u{2014} the bar is the tab row (D15), and the open segment wears the selected treatment. Mounted with empty popover inputs \u{2014} no address, no roster, no ledger \u{2014} which is the honest cold-start shape; the panel's own stories cover the five states with their data."
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
    description = "The CHANGES section on a dirty hardware session — the third question, in its own home: the labeled pending edits with per-entry revert, the pending facts, an honest receipt (\"already live in this session\" — it never claims cloud state it cannot see), the controller's own Save plus the Revert-all that retired the \u{21ba} sibling, and the banked History underneath (D14 — one temporal axis; this never-saved fixture shows the honest empty line). The counts here are the same ones the closed segment shows."
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
    description = "The clean half of the changes popup: one plain line over the banked History (empty here, honestly). The segment is always present (a quiet \u{2713}), so \"nothing pending\" is a state you can click into and confirm rather than an absence you have to infer."
)]
pub(crate) fn control_changes_popup_clean() -> Element {
    rsx! {
        div { class: "tw:min-h-[260px]",
            {control_row(700, sim_control(Some("ESP32-C6")), Some(control_content(0, 0, UiStatus::good("Ready"))), Some(ControlSegment::Changes))}
        }
    }
}

#[story(
    label = "Changes panel — pending over banked history",
    description = "The merged changes panel (D14), mounted directly with a fixed clock: the pending block on top (two unsaved edits, the receipt naming the version Save will bank — v13, the same number the timeline's newest row will wear after the save), and the document's banked history underneath — version, kind, what, when; a push names its device by uid (resolving names needs the async device registry). One panel, no tab between the two ends of the ledger."
)]
pub(crate) fn control_changes_panel_history() -> Element {
    changes_panel_frame(rsx! {
        SessionChangesPanel {
            changes: dirty_content_with_history().changes(),
            relationship: ProjectRelationship::MinePublished,
            now_secs: STORY_NOW,
            on_action: EventHandler::new(|_| {}),
        }
    })
}

#[story(
    label = "Changes panel — clean, with history",
    description = "The same merged panel with nothing pending: the quiet all-saved line, then the banked timeline. Clean is not empty — the document still has a past, and this is where it reads."
)]
pub(crate) fn control_changes_panel_clean_history() -> Element {
    changes_panel_frame(rsx! {
        SessionChangesPanel {
            changes: content_with_history(0, 0, UiStatus::good("Ready")).changes(),
            relationship: ProjectRelationship::MinePublished,
            now_secs: STORY_NOW,
            on_action: EventHandler::new(|_| {}),
        }
    })
}

#[story(
    label = "Changes panel — example keeps the empty line",
    description = "A built-in example's session DOES carry history rows — the transient open seeds a provenance origin plus an initial save — but that is bookkeeping, not something the person did, so the timeline says \"no history yet\" even with rows on hand. It is the same sentence that stays true after Save a copy: the real rows begin at the first save."
)]
pub(crate) fn control_changes_panel_example() -> Element {
    changes_panel_frame(rsx! {
        SessionChangesPanel {
            changes: content_with_history(0, 0, UiStatus::good("Ready")).changes(),
            relationship: ProjectRelationship::Example,
            now_secs: STORY_NOW,
            on_action: EventHandler::new(|_| {}),
        }
    })
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
    description = "A lens route fronted (single-session policy): Boards and Docs carry the \u{2197} new-tab mark in the secondary family, because from here they open a NEW tab rather than ending the session (ruling R8-3, amended 8.1)."
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
    description = "The phone rung's ⋯ menu (crowded bar <560): the Project group leads with its one Archive row, and the Sections group carries ALL FIVE sections — Devices and Projects join the world's three, because the phone bar keeps no inline tabs at all."
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
                    project_popover: ProjectPopoverInputs::default(),
                    on_action: EventHandler::new(|_| {}),
                    initially_open: None,
                }),
                play_toggle: Some(ChromeModeToggle { href: "#play".to_string(), active: false }),
                project_menu: Some(ChromeProjectMenu {
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
                    project_popover: ProjectPopoverInputs::default(),
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
        key: "runtime-sim".to_string(),
        name: "Sim".to_string(),
        board: board.map(str::to_string),
        status: UiChromeSessionStatus::Run,
        stat_line: board.map(|_| "60 fps · 217 lamps".to_string()),
    }
}

/// The hardware-session rows went with M2 of the device-model rebuild;
/// what stands in is a boardless sim (the same lockup with no suffix).
fn hardware_control() -> UiChromeSessionControl {
    sim_control(None)
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

/// A fixed clock for the history rows, so relative times never drift
/// between captures.
const STORY_NOW: f64 = 1_800_000_000.0;

/// `dirty_content` plus a banked history — the merged changes panel's
/// full shape (D14): pending block over timeline.
fn dirty_content_with_history() -> ProjectDetailContent {
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
    editor.history = history();
    ProjectDetailContent::new(&editor, UiStatus::good("Ready"))
}

/// The shared control content with the history fixture stamped on.
fn content_with_history(persisted: usize, failed: usize, status: UiStatus) -> ProjectDetailContent {
    let mut editor = project_editor_fixture(ProjectSyncPhase::Ready);
    editor.dirty = DirtySummary { persisted, failed };
    editor.header_actions = save_revert_actions(persisted);
    editor.history = history();
    ProjectDetailContent::new(&editor, status)
}

/// A representative log: a fork origin, saves, a push, and a join —
/// every row kind the projection emits, newest first the way core hands
/// it over. (Moved here from the relationship-panel stories at D14, with
/// the History tab itself.)
fn history() -> UiProjectHistory {
    UiProjectHistory {
        entries: vec![
            entry(Some(12), UiHistoryKind::Saved, "", STORY_NOW - 20.0 * 60.0),
            entry(
                Some(12),
                UiHistoryKind::Pushed,
                "\u{2192} dev7g2k\u{2026}",
                STORY_NOW - 18.0 * 60.0,
            ),
            entry(Some(11), UiHistoryKind::Saved, "", STORY_NOW - 2.0 * 3600.0),
            entry(
                Some(10),
                UiHistoryKind::Joined,
                "kept this version \u{2014} the other was set aside",
                STORY_NOW - 5.0 * 3600.0,
            ),
            entry(
                Some(1),
                UiHistoryKind::Origin,
                "forked from prj4m1x\u{2026}",
                STORY_NOW - 3.0 * 86_400.0,
            ),
        ],
        next_version: Some(13),
    }
}

fn entry(version: Option<u64>, kind: UiHistoryKind, label: &str, at: f64) -> UiProjectHistoryEntry {
    UiProjectHistoryEntry {
        version,
        kind,
        label: label.to_string(),
        at,
    }
}

/// The detail card's box for a directly-mounted panel — the popover
/// primitive owns the chrome in the live app, so the story frame supplies
/// one (the relationship-panel stories' own technique).
fn changes_panel_frame(children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:min-w-0 tw:rounded-md tw:border tw:border-border-strong tw:bg-card-subtle tw:text-sm tw:text-muted-foreground tw:shadow-lg",
            {children}
        }
    }
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
