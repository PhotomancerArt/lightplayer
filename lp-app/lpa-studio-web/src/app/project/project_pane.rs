//! The project pane: the whole project card as ONE `StudioPane` (UX gate
//! feedback on D4/D5 — no second header above the node tree).
//!
//! Header: the project *name* as the title (never the literal word
//! "project" — that is the kind label), a dirty/status tone wash, contextual
//! Save / Revert-to-saved icon actions supplied by the controller
//! (`ProjectEditorView.header_actions`), the always-present "+" whose press
//! opens the add-node kind picker (`ProjectEditorView.add_node_menu`, P5 —
//! intercepting the P4 add action's default create), and a `DetailPopover`
//! at the right edge whose trigger renders the pane's one core-computed
//! `UiAffordance`
//! (P6 affordance model), plus — and ONLY when the project carries active
//! debug overrides — the global "Debug active · N · Clear all" chip (D8 tier
//! a). No status chip and no count chips in the header:
//! the status word ("Ready", "Syncing", …), the per-bucket dirty counts, and
//! the project stats all live in the detail popup.
//!
//! Body: the node tree (plus any sync issue) — no heading and no pane-level
//! button strip (P6 sidebar tidy: the tree is self-evident; Refresh and
//! Disconnect remain ops without buttons). The popup is what the project
//! IS — identity, "Project settings" rows, stats. Two things are
//! deliberately NOT here: the pending edits, owned by the header control's
//! **changes** segment (relationship-control D8, with per-entry revert,
//! revert-all, and Save), and the share rows, owned by its **project**
//! segment's popover (D9 — Copy link in the action row, zip/JSON in its ⋯).
//!
//! **Embedded mode** (workbench ruling 2): the workbench's Nodes dock renders
//! this pane FLAT — no card chrome, no project-name/[i] header, just the save
//! affordances, any sync issue, and the tree on the panel's own background.
//! The dock is already titled "Nodes", so a card inside it was box-in-box; and
//! the popup that used to hang off this header lives on the header
//! session·project control instead — the relationship panel under its
//! PROJECT segment (which reaches [`ProjectDetailSections`] through its ⋯
//! menu's Details row), the pending edits under its CHANGES segment
//! (single-session policy). Every other mount keeps the card, header, and
//! popup exactly as before.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DirtySummary, ProjectController, ProjectEditorView, ProjectOp, ProjectSyncPhase,
    UiAction, UiAffordance, UiConfigSlot, UiMetric, UiPaneAction, UiPendingEdit, UiStatus,
};

use crate::app::affordance::{affordance_pane_tone, affordance_trigger_style};
use crate::app::layout::{PaneChrome, StudioPane};
use crate::app::node::node_status_label_class;
use crate::app::project::{ProjectNodeTree, ProjectSettingsSection};
use crate::base::{DetailPopover, DetailSection, PopoverPlacement};

/// Everything the project's detail popup shows, gathered from the editor view
/// plus the pane-level status — one value so the SAME sections can render in
/// two homes: the project pane's own [i] (every non-workbench mount) and the
/// header session·project control's panel (every mount — single-session
/// policy).
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectDetailContent {
    affordance: UiAffordance,
    project_name: String,
    status: UiStatus,
    dirty: DirtySummary,
    overlay_revision: i64,
    edits_in_flight: usize,
    stats: Vec<UiMetric>,
    pending_edits: Vec<UiPendingEdit>,
    root_slots: Vec<UiConfigSlot>,
    manifest: Option<lpa_studio_core::UiProjectManifest>,
    library_identity: Option<(String, String)>,
    /// The open project's document history, newest first and capped — the
    /// relationship panel's History tab (D10). It rides this value for the
    /// same reason everything else here does: one gather, so no two homes
    /// can disagree about a project.
    history: lpa_studio_core::UiProjectHistory,
    /// The controller's contextual Save / Revert-to-saved pair (present
    /// only while persisted edits are pending). The SECTIONS do not render
    /// them — the pane header and the header session·project control do —
    /// but they ride this value so every home dispatches the controller's
    /// own actions instead of minting a second save verb.
    header_actions: Vec<UiPaneAction>,
}

/// The pending-work facts and lists the header control's **changes**
/// popup renders — the projection [`ProjectDetailContent::changes`] hands
/// out. Plain public fields: this is a view value, not a widget.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectChanges {
    /// The merged affordance — the popup's "State" row wording.
    pub affordance: UiAffordance,
    /// Unsaved / failed counts: the changes segment's face reads them, and
    /// `persisted > 0` is the same dirty test `header_actions` encodes.
    pub dirty: DirtySummary,
    /// The overlay's revision number — the pending fact that used to sit in
    /// the detail sections' "Pending edits" block.
    pub overlay_revision: i64,
    /// Edits dispatched and not yet acked.
    pub edits_in_flight: usize,
    /// Every pending edit, in the DTO's stable order (bucketed for display
    /// by `pending_edit_section::entries_in`).
    pub pending_edits: Vec<UiPendingEdit>,
    /// The controller's own Save / Revert-to-saved pair — the popup
    /// dispatches THESE, never a second save verb minted locally.
    pub header_actions: Vec<UiPaneAction>,
}

impl ProjectDetailContent {
    /// The merged affordance — the header control's state glyph reads it.
    pub fn affordance(&self) -> UiAffordance {
        self.affordance
    }

    /// The project's display name — the header control's project segment
    /// text.
    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    /// Unsaved persisted edits — the header control's amber count.
    pub fn unsaved_count(&self) -> usize {
        self.dirty.persisted
    }

    /// The open project's library package, as `(uid, slug)` — `None` for
    /// the storeless demo path or a device-hosted project this library does
    /// not know. The relationship panel's ⋯ export rows and its Duplicate
    /// verb both address the package by this uid.
    pub fn library_identity(&self) -> Option<&(String, String)> {
        self.library_identity.as_ref()
    }

    /// The open project's document history — the relationship panel's
    /// History tab reads it, read-only (no restore: vision D6 is parked).
    pub fn history(&self) -> &lpa_studio_core::UiProjectHistory {
        &self.history
    }

    /// The controller's contextual header actions (Save / Revert-to-saved),
    /// empty while the project is clean — the header session·project
    /// control's trailing segments read them.
    pub fn header_actions(&self) -> &[UiPaneAction] {
        &self.header_actions
    }

    /// The **changes** half of this content: everything the header
    /// control's changes popup renders, in one value.
    ///
    /// The bar's changes segment is the concept home for pending work
    /// (relationship-control D8), so the popup — not
    /// [`ProjectDetailSections`] — lists the unsaved and failed entries and
    /// states the pending facts. This accessor exists so the popup can live
    /// beside the segment it hangs off without the sections' private fields
    /// leaking wholesale.
    pub fn changes(&self) -> ProjectChanges {
        ProjectChanges {
            affordance: self.affordance,
            dirty: self.dirty,
            overlay_revision: self.overlay_revision,
            edits_in_flight: self.edits_in_flight,
            pending_edits: self.pending_edits.clone(),
            header_actions: self.header_actions.clone(),
        }
    }

    /// Gather the popup's content from the editor view and the pane status
    /// (the same merge the pane header's affordance uses).
    pub fn new(view: &ProjectEditorView, status: UiStatus) -> Self {
        Self {
            affordance: view.affordance(status.kind),
            project_name: view.project_name.clone(),
            status,
            dirty: view.dirty,
            overlay_revision: view.sync.overlay_revision,
            edits_in_flight: view.edits_in_flight,
            stats: view.stats.clone(),
            pending_edits: view.pending_edits.clone(),
            root_slots: view.root_slots.clone(),
            manifest: view.manifest.clone(),
            library_identity: view.library_identity.clone(),
            history: view.history.clone(),
            header_actions: view.header_actions.clone(),
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectPane(
    view: ProjectEditorView,
    /// Pane-level status from the project controller ("Ready", "Syncing", …),
    /// merged into the header affordance and shown as text in the popup.
    #[props(default = UiStatus::neutral("Project"))]
    status: UiStatus,
    #[props(default = false)] running: bool,
    /// Render FLAT, with no card chrome and no header (workbench dock only —
    /// see the module docs). Every other mount leaves this off and renders
    /// exactly as it always has.
    #[props(default = false)]
    embedded: bool,
    on_action: EventHandler<UiAction>,
    /// Open the detail popup immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
    /// Open the add-node kind picker immediately (stories only).
    #[props(default = false)]
    add_picker_initially_open: bool,
) -> Element {
    let detail_content = ProjectDetailContent::new(&view, status.clone());
    let affordance = detail_content.affordance;
    let chrome = PaneChrome {
        tone: affordance_pane_tone(affordance, status.kind),
        selected: false,
        chips: Vec::new(),
    };
    let sync_issue = view.sync.issue.clone();
    let roots = view.tree.roots.clone();
    let syncing = !matches!(view.sync.phase, ProjectSyncPhase::Ready);
    let tree_add_menu = view.add_node_menu.clone();
    // Adding does not ride the header: the tree's "Add node…" row (below)
    // and the workspace button carry the picker. Header actions are the
    // contextual Save / Revert pair on the generic `PaneActionButton` path.
    let header_actions = view.header_actions.clone();
    // D8 tier (a): the project-wide debug channel, deliberately outside the
    // dirty rollup that drives `chrome`/`affordance` (D7).
    let debug_overrides = view.debug_overrides;

    if embedded {
        // Flat on the dock's background: the sync issue and the tree. No
        // card, no title row, no [i] — the dock's tab names this, and the
        // popup plus the Save/Revert pair both live on the header
        // session·project control now (R7-2: the save moment's one home).
        return rsx! {
            div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-2.5",
                if let Some(issue) = sync_issue.as_ref() {
                    div { class: "tw:grid tw:gap-1 tw:rounded-sm tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-2 tw:text-xs tw:text-status-error-foreground",
                        strong { "{issue.message}" }
                        if let Some(detail) = issue.detail.as_ref() {
                            p { class: "tw:m-0 tw:text-xs tw:text-status-error-foreground", "{detail}" }
                        }
                    }
                }
                ProjectNodeTree {
                    roots,
                    running,
                    add_node_menu: tree_add_menu,
                    syncing,
                    add_picker_initially_open,
                    on_action,
                }
            }
        };
    }

    rsx! {
        StudioPane {
            title: view.project_name.clone(),
            kind: "Project".to_string(),
            chrome,
            actions: header_actions,
            on_action,
            trailing: rsx! {
                DebugActiveChip { count: debug_overrides, on_action }
            },
            detail: rsx! {
                ProjectDetailPopover { content: detail_content, initially_open }
            },
            body: rsx! {
                div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3 tw:pt-3",
                    if let Some(issue) = sync_issue.as_ref() {
                        div { class: "tw:grid tw:gap-1 tw:rounded-sm tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-3 tw:text-sm tw:text-status-error-foreground",
                            strong { "{issue.message}" }
                            if let Some(detail) = issue.detail.as_ref() {
                                p { class: "tw:m-0 tw:text-xs tw:text-status-error-foreground", "{detail}" }
                            }
                        }
                    }
                    ProjectNodeTree {
                        roots,
                        running,
                        add_node_menu: tree_add_menu,
                        syncing,
                        add_picker_initially_open,
                        on_action,
                    }
                }
            },
        }
    }
}

/// The global **"Debug active · N · Clear all"** chip (D8 tier a): present
/// whenever ANY debug override is active anywhere in the project, absent
/// otherwise. It is the only project-level announcement debug overrides get —
/// they are not dirty (D7), so they never reach the header wash, the Save
/// affordances, or the save panel's change list.
///
/// Pressing it dispatches [`ProjectOp::ClearDebugEdits`] (label "Clear all"
/// already lives on the op's `ActionMeta`, so the chip stays pure
/// presentation); persisted edits survive untouched — this is not Revert-all.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DebugActiveChip(count: usize, on_action: EventHandler<UiAction>) -> Element {
    if count == 0 {
        return rsx! {};
    }
    let action = UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        ProjectOp::ClearDebugEdits,
    );
    let summary = action.meta().summary.clone();

    rsx! {
        div { class: "tw:flex tw:items-center tw:pr-1",
            button {
                class: "lp-debug-global-chip",
                r#type: "button",
                title: "{summary}",
                aria_label: "Clear all debug overrides",
                onclick: move |event| {
                    event.stop_propagation();
                    on_action.call(action.clone());
                },
                "Debug active · {count} · Clear all"
            }
        }
    }
}

/// The detail popup on the shared [`DetailPopover`] base — the project's
/// standing panel: project identity with the status word (its only home —
/// headers no longer carry a status chip), the root's "Project settings"
/// identity rows (the editable `name` — and the read-only
/// `format`/`uid`/`nodes` rows — live here rather than on the restored root
/// card, as purpose-built controls rather than generic slot editors; see
/// [`ProjectSettingsSection`]), and the project stats (moved here
/// from the old sidebar MetricGrid card).
///
/// Two things are NOT here. The pending-edit lists and facts belong to the
/// header control's **changes** segment (relationship-control D8), whose
/// popup renders [`ProjectDetailContent::changes`]; the share rows belong
/// to its **project** segment's popover (D9), which is also what reaches
/// these sections, through its ⋯ menu's Details row.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectDetailPopover(
    content: ProjectDetailContent,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let affordance = content.affordance;
    let style = affordance_trigger_style(affordance);
    let label = trigger_label(affordance);

    rsx! {
        DetailPopover {
            icon: style.icon,
            label: label.to_string(),
            tone: style.tone,
            placement: PopoverPlacement::BottomEnd,
            active: affordance.is_announced(),
            initially_open,
            ProjectDetailSections { content }
        }
    }
}

/// The project popup's SECTIONS, without the popover around them — the
/// re-housable unit (ruling 2). Rendered inside the project pane's own [i]
/// on every non-workbench mount, and behind the header control's project
/// popover ⋯ menu's **Details** row on every mount (single-session policy)
/// — the workbench's flat Nodes dock has no header of its own to hang a
/// popup from, so that mount is the ONLY place its project state shows.
///
/// Purely presentational now: the change lists (which carried the per-entry
/// revert dispatch) moved to the changes popup, so these sections take no
/// `on_action`.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectDetailSections(content: ProjectDetailContent) -> Element {
    let ProjectDetailContent {
        project_name,
        status,
        stats,
        root_slots,
        manifest,
        // The sharing half — the link, the two export forms, and the
        // dirty-disable rule that guards them — belongs to the project
        // popover now (relationship-control D9): Copy link is a fixed
        // action-row slot, zip/JSON are its ⋯ overflow. These sections are
        // reached THROUGH that popover, so carrying the rows here too would
        // be the same door twice.
        dirty: _,
        library_identity: _,
        // The changes half of the content — pending-edit lists, the
        // pending facts, and the Save/Revert pair — belongs to the header
        // control's CHANGES popup now (relationship-control D8); it rides
        // the value so both halves stay one projection, and the sections
        // deliberately render none of it.
        affordance: _,
        overlay_revision: _,
        edits_in_flight: _,
        pending_edits: _,
        header_actions: _,
        // Likewise the history half: the relationship panel's History tab
        // renders it, and these sections are its neighbours, not its home.
        history: _,
    } = content;
    let status_class = node_status_label_class(status.kind);

    rsx! {
        DetailSection {
            div { class: "tw:flex tw:min-w-0 tw:items-start tw:justify-between tw:gap-4 tw:py-1",
                div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                    strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words", "{project_name}" }
                    span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground", "Project" }
                }
                span { class: status_class, "{status.label}" }
            }
        }
        if !root_slots.is_empty() || manifest.is_some() {
            // The project's identity rows (container manifest, plus the
            // root's nodes count): purpose-built controls, NOT the
            // generic slot editor — see `project_settings_section`.
            DetailSection { title: "Project settings",
                ProjectSettingsSection { manifest, root_slots }
            }
        }
        if !stats.is_empty() {
            DetailSection { title: "Project stats",
                for metric in stats {
                    ProjectDetailRow { label: metric.label.clone(), value: metric.value.clone() }
                }
            }
        }
    }
}

/// One label/value row of the detail card — shared with the header
/// control's changes popup, which carries the pending facts now.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProjectDetailRow(label: String, value: String) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:flex tw:items-baseline tw:justify-between tw:gap-3 tw:text-xs tw:leading-snug",
            span { class: "tw:font-bold tw:text-subtle-foreground", "{label}" }
            span { class: "tw:font-mono tw:text-muted-foreground", "{value}" }
        }
    }
}

/// Accessible trigger label for the pane's merged affordance — shared
/// with the site header's session·project control, which opens the same
/// panel.
pub(crate) fn trigger_label(affordance: UiAffordance) -> &'static str {
    match affordance {
        UiAffordance::Info => "Project details — no unsaved changes",
        UiAffordance::Busy => "Project activity in progress",
        UiAffordance::Debug => "Project has debug overrides",
        UiAffordance::Unsaved => "Project has unsaved changes",
        UiAffordance::Error => "Project needs attention",
    }
}

/// The "State" row wording for the merged affordance — read by the header
/// control's changes popup, which is where the pending facts live now.
pub(crate) fn state_label(affordance: UiAffordance) -> &'static str {
    match affordance {
        UiAffordance::Info => "unchanged",
        UiAffordance::Busy => "in progress",
        UiAffordance::Debug => "debug overrides only",
        UiAffordance::Unsaved => "uncommitted",
        UiAffordance::Error => "needs attention",
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::core::status::UiStatusKind;
    use lpa_studio_core::{ProjectNodeTreeView, ProjectSyncSummary};

    use super::*;

    fn dirty(persisted: usize, failed: usize) -> DirtySummary {
        DirtySummary { persisted, failed }
    }

    fn editor_view(dirty: DirtySummary, edits_in_flight: usize) -> ProjectEditorView {
        let mut view = ProjectEditorView::new(
            "p",
            1,
            ProjectSyncSummary::default(),
            Vec::new(),
            ProjectNodeTreeView::new(Vec::new(), 0),
            Vec::new(),
        );
        view.dirty = dirty;
        view.edits_in_flight = edits_in_flight;
        view
    }

    #[test]
    fn trigger_follows_the_core_merge_pencil_when_uncommitted_i_otherwise() {
        // Clean + Ready: quiet "i".
        let clean = editor_view(DirtySummary::clean(), 0).affordance(UiStatusKind::Good);
        assert_eq!(clean, UiAffordance::Info);
        assert_eq!(state_label(clean), "unchanged");

        // Persisted edits: the edited pencil, even while an ack is pending
        // (Unsaved outranks Busy in the shared priority).
        let uncommitted = editor_view(dirty(1, 0), 1).affordance(UiStatusKind::Good);
        assert_eq!(uncommitted, UiAffordance::Unsaved);
        assert_eq!(state_label(uncommitted), "uncommitted");

        // In-flight only: genuine activity.
        let busy = editor_view(dirty(0, 0), 1).affordance(UiStatusKind::Good);
        assert_eq!(busy, UiAffordance::Busy);
        assert_eq!(state_label(busy), "in progress");

        // D7: a project whose only pending edits are debug overrides reads
        // clean — they never enter the summary, so the trigger stays quiet.
        let debug_only = editor_view(DirtySummary::clean(), 0).affordance(UiStatusKind::Good);
        assert_eq!(debug_only, UiAffordance::Info);
    }

    #[test]
    fn header_tone_rides_the_shared_merge_and_error_is_never_masked() {
        let tone = |dirty: DirtySummary, in_flight: usize, status: UiStatusKind| {
            affordance_pane_tone(editor_view(dirty, in_flight).affordance(status), status)
        };

        use crate::app::layout::PaneTone;
        assert_eq!(
            tone(DirtySummary::clean(), 0, UiStatusKind::Good),
            PaneTone::Good
        );
        assert_eq!(tone(dirty(1, 1), 2, UiStatusKind::Good), PaneTone::Error);
        assert_eq!(tone(dirty(2, 0), 0, UiStatusKind::Good), PaneTone::Warning);
        assert_eq!(tone(dirty(0, 1), 0, UiStatusKind::Good), PaneTone::Error);
        assert_eq!(tone(dirty(0, 0), 1, UiStatusKind::Good), PaneTone::Working);
        // An error pane status is never masked by a dirty wash.
        assert_eq!(tone(dirty(1, 0), 0, UiStatusKind::Error), PaneTone::Error);
    }
}
