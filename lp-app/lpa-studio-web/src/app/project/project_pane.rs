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
//! Disconnect remain ops without buttons). The popup is the save panel
//! (M3 P5) plus the root's "Project settings" rows (P6 flat root): the
//! per-bucket sections list the labeled pending edits with per-entry revert.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DirtySummary, ProjectController, ProjectEditorView, ProjectOp, ProjectSyncPhase,
    UiAction, UiAffordance, UiConfigSlot, UiMetric, UiPendingEdit, UiStatus,
};

use crate::app::affordance::{affordance_pane_tone, affordance_trigger_style};
use crate::app::layout::{PaneChrome, StudioPane};
use crate::app::node::node_status_label_class;
use crate::app::project::pending_edit_section::{
    PendingEditBucket, PendingEditList, bucket_section_tint, entries_in,
};
use crate::app::project::{ProjectNodeTree, ProjectSettingsSection, ProjectShareSection};
use crate::base::{DetailPopover, DetailSection, PopoverPlacement};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectPane(
    view: ProjectEditorView,
    /// Pane-level status from the project controller ("Ready", "Syncing", …),
    /// merged into the header affordance and shown as text in the popup.
    #[props(default = UiStatus::neutral("Project"))]
    status: UiStatus,
    #[props(default = false)] running: bool,
    on_action: EventHandler<UiAction>,
    /// Open the detail popup immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
    /// Open the add-node kind picker immediately (stories only).
    #[props(default = false)]
    add_picker_initially_open: bool,
) -> Element {
    let dirty = view.dirty;
    let edits_in_flight = view.edits_in_flight;
    let affordance = view.affordance(status.kind);
    let chrome = PaneChrome {
        tone: affordance_pane_tone(affordance, status.kind),
        accent: false,
        chips: Vec::new(),
    };
    let overlay_revision = view.sync.overlay_revision;
    let sync_issue = view.sync.issue.clone();
    let stats = view.stats.clone();
    let roots = view.tree.roots.clone();
    let syncing = !matches!(view.sync.phase, ProjectSyncPhase::Ready);
    let tree_add_menu = view.add_node_menu.clone();
    // Adding does not ride the header: the tree's "Add node…" row (below)
    // and the workspace button carry the picker. Header actions are the
    // contextual Save / Revert pair on the generic `PaneActionButton` path.
    let header_actions = view.header_actions.clone();
    let project_name = view.project_name.clone();
    let pending_edits = view.pending_edits.clone();
    let root_slots = view.root_slots.clone();
    let manifest = view.manifest.clone();
    let library_identity = view.library_identity.clone();
    // D8 tier (a): the project-wide debug channel, deliberately outside the
    // dirty rollup that drives `chrome`/`affordance` (D7).
    let debug_overrides = view.debug_overrides;

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
                ProjectDetailPopover {
                    affordance,
                    project_name,
                    status,
                    dirty,
                    overlay_revision,
                    edits_in_flight,
                    stats,
                    pending_edits,
                    root_slots,
                    manifest,
                    library_identity,
                    on_action,
                    initially_open,
                }
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
fn DebugActiveChip(count: usize, on_action: EventHandler<UiAction>) -> Element {
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

/// The detail popup on the shared [`DetailPopover`] base — the save panel:
/// project identity with the status word (its only home — headers no longer
/// carry a status chip), the root's "Project settings" identity rows (P6
/// flat root: the workspace renders no root card, so the editable `name` —
/// and the read-only `format`/`uid`/`nodes` rows — live here, as
/// purpose-built controls rather than generic slot editors; see
/// [`ProjectSettingsSection`]), the pending-edit state,
/// overlay revision, the per-bucket [`DetailSection`]s (unsaved / failed —
/// there is no debug bucket, D7) as titled change lists with per-entry revert (a populated bucket
/// wears its affordance tint on the title; the count rides the title row's
/// meta cell), and the project stats (moved here from the old sidebar
/// MetricGrid card).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectDetailPopover(
    affordance: UiAffordance,
    project_name: String,
    status: UiStatus,
    dirty: DirtySummary,
    overlay_revision: i64,
    edits_in_flight: usize,
    stats: Vec<UiMetric>,
    pending_edits: Vec<UiPendingEdit>,
    #[props(default)] root_slots: Vec<UiConfigSlot>,
    /// The container-manifest identity, when a library package backs it.
    #[props(default)]
    manifest: Option<lpa_studio_core::UiProjectManifest>,
    /// The open project's library identity, when a package backs it.
    #[props(default)]
    library_identity: Option<(String, String)>,
    on_action: EventHandler<UiAction>,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let style = affordance_trigger_style(affordance);
    let label = trigger_label(affordance);
    let status_class = node_status_label_class(status.kind);
    let unsaved_entries = entries_in(&pending_edits, PendingEditBucket::Persisted);
    let failed_entries = entries_in(&pending_edits, PendingEditBucket::Failed);

    rsx! {
        DetailPopover {
            icon: style.icon,
            label: label.to_string(),
            tone: style.tone,
            placement: PopoverPlacement::BottomEnd,
            active: affordance.is_announced(),
            initially_open,
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
            // Share is its own section: settings are what the project IS,
            // share is what you do with it. A project with no library
            // package behind it (the storeless demo path, or a
            // device-hosted project this library does not know) renders
            // none — the affordances read the library snapshot.
            if let Some((uid, slug)) = library_identity.clone() {
                DetailSection { title: "Share",
                    ProjectShareSection { uid, slug, unsaved: dirty.persisted }
                }
            }
            DetailSection { title: "Pending edits",
                ProjectDetailRow { label: "State", value: state_label(affordance).to_string() }
                ProjectDetailRow { label: "Overlay revision", value: overlay_revision.to_string() }
                if edits_in_flight > 0 {
                    ProjectDetailRow { label: "Awaiting ack", value: edits_in_flight.to_string() }
                }
            }
            DetailSection {
                title: "Unsaved (persisted)",
                meta: dirty.persisted.to_string(),
                tint: bucket_section_tint(PendingEditBucket::Persisted, dirty.persisted),
                PendingEditList { entries: unsaved_entries, on_action }
            }
            if dirty.failed > 0 || !failed_entries.is_empty() {
                DetailSection {
                    title: "Failed edits",
                    meta: dirty.failed.to_string(),
                    tint: bucket_section_tint(PendingEditBucket::Failed, dirty.failed),
                    PendingEditList { entries: failed_entries, on_action }
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
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectDetailRow(label: String, value: String) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:flex tw:items-baseline tw:justify-between tw:gap-3 tw:text-xs tw:leading-snug",
            span { class: "tw:font-bold tw:text-subtle-foreground", "{label}" }
            span { class: "tw:font-mono tw:text-muted-foreground", "{value}" }
        }
    }
}

/// Accessible trigger label for the pane's merged affordance.
fn trigger_label(affordance: UiAffordance) -> &'static str {
    match affordance {
        UiAffordance::Info => "Project details — no unsaved changes",
        UiAffordance::Busy => "Project activity in progress",
        UiAffordance::Debug => "Project has debug overrides",
        UiAffordance::Unsaved => "Project has unsaved changes",
        UiAffordance::Error => "Project needs attention",
    }
}

/// The popup's "State" row wording for the merged affordance.
fn state_label(affordance: UiAffordance) -> &'static str {
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
