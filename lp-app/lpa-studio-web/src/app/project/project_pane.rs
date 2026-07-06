//! The project pane: the whole project card as ONE `StudioPane` (UX gate
//! feedback on D4/D5 — no second header above the node tree).
//!
//! Header: the project *name* as the title (never the literal word
//! "project" — that is the kind label), a dirty/status tone wash, contextual
//! Save / Revert-to-saved icon actions supplied by the controller
//! (`ProjectEditorView.header_actions`), and a `DetailPopover` at the right
//! edge whose trigger renders the pane's one core-computed `UiAffordance`
//! (P6 affordance model). No status chip and no count chips in the header:
//! the status word ("Ready", "Syncing", …), the per-bucket dirty counts, and
//! the project stats all live in the detail popup.
//!
//! Body: the node tree (plus any sync issue and the pane-level
//! Refresh/Disconnect actions). The popup stays at M2's revision + counts
//! detail level; the full "what changed" panel is M3.

use dioxus::prelude::*;
use lpa_studio_core::{
    DirtySummary, ProjectEditorView, UiAction, UiAffordance, UiMetric, UiStatus,
};

use crate::app::affordance::{affordance_pane_tone, affordance_trigger_style};
use crate::app::layout::{PaneChrome, StudioPane};
use crate::app::node::node_status_label_class;
use crate::app::project::ProjectNodeTree;
use crate::base::{
    DetailPopover, DetailSectionTint, PopoverPlacement, detail_popover_section_class,
};
use crate::core::ActionStrip;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectPane(
    view: ProjectEditorView,
    /// Pane-level status from the project controller ("Ready", "Syncing", …),
    /// merged into the header affordance and shown as text in the popup.
    #[props(default = UiStatus::neutral("Project"))]
    status: UiStatus,
    /// Pane-level actions (Refresh / Disconnect) rendered at the body's foot.
    #[props(default)]
    pane_actions: Vec<UiAction>,
    #[props(default = false)] running: bool,
    on_action: EventHandler<UiAction>,
    /// Open the detail popup immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
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
    let header_actions = view.header_actions.clone();
    let project_name = view.project_name.clone();

    rsx! {
        StudioPane {
            title: view.project_name.clone(),
            kind: "Project".to_string(),
            chrome,
            actions: header_actions,
            on_action,
            detail: rsx! {
                ProjectDetailPopover {
                    affordance,
                    project_name,
                    status,
                    dirty,
                    overlay_revision,
                    edits_in_flight,
                    stats,
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
                    h3 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Node tree" }
                    ProjectNodeTree { roots, running, on_action }
                    if !pane_actions.is_empty() {
                        ActionStrip {
                            actions: pane_actions,
                            running,
                            on_action,
                        }
                    }
                }
            },
        }
    }
}

/// The detail popup on the shared [`DetailPopover`] base: project identity
/// with the status word (its only home — headers no longer carry a status
/// chip), the pending-edit state, overlay revision, per-bucket dirty counts
/// with their status tints, and the project stats (moved here from the old
/// sidebar MetricGrid card).
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
    #[props(default = false)] initially_open: bool,
) -> Element {
    let style = affordance_trigger_style(affordance);
    let label = trigger_label(affordance);
    let status_class = node_status_label_class(status.kind);

    rsx! {
        DetailPopover {
            icon: style.icon,
            label: label.to_string(),
            tone: style.tone,
            placement: PopoverPlacement::BottomEnd,
            active: affordance.is_announced(),
            initially_open,
            section { class: detail_popover_section_class(DetailSectionTint::None),
                div { class: "tw:flex tw:min-w-0 tw:items-start tw:justify-between tw:gap-4 tw:py-1",
                    div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                        strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words", "{project_name}" }
                        span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground", "Project" }
                    }
                    span { class: status_class, "{status.label}" }
                }
            }
            section { class: "tw:grid tw:gap-1 tw:px-3 tw:py-2",
                h3 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Pending edits" }
                ProjectDetailRow { label: "State", value: state_label(affordance).to_string() }
                ProjectDetailRow { label: "Overlay revision", value: overlay_revision.to_string() }
                if edits_in_flight > 0 {
                    ProjectDetailRow { label: "Awaiting ack", value: edits_in_flight.to_string() }
                }
            }
            section { class: detail_popover_section_class(unsaved_section_tint(&dirty)),
                ProjectDetailRow { label: "Unsaved (persisted)", value: dirty.persisted.to_string() }
            }
            section { class: detail_popover_section_class(live_section_tint(&dirty)),
                ProjectDetailRow { label: "Live (transient)", value: dirty.transient.to_string() }
                p { class: "tw:m-0 tw:pt-1 tw:text-[0.68rem] tw:leading-snug tw:text-subtle-foreground",
                    "Live controls apply to the running project and are never written by Save."
                }
            }
            if dirty.failed > 0 {
                section { class: detail_popover_section_class(DetailSectionTint::Error),
                    ProjectDetailRow { label: "Failed edits", value: dirty.failed.to_string() }
                }
            }
            if !stats.is_empty() {
                section { class: detail_popover_section_class(DetailSectionTint::None),
                    h3 { class: "tw:m-0 tw:pb-0.5 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Project stats" }
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
        UiAffordance::Live => "Project has live-only edits",
        UiAffordance::Unsaved => "Project has unsaved changes",
        UiAffordance::Error => "Project needs attention",
    }
}

/// The popup's "State" row wording for the merged affordance.
fn state_label(affordance: UiAffordance) -> &'static str {
    match affordance {
        UiAffordance::Info => "unchanged",
        UiAffordance::Busy => "in progress",
        UiAffordance::Live => "live edits only",
        UiAffordance::Unsaved => "uncommitted",
        UiAffordance::Error => "needs attention",
    }
}

/// The unsaved section wears the warning (yellow) edited tint whenever
/// persisted edits are pending — the same treatment as edited slot rows.
fn unsaved_section_tint(dirty: &DirtySummary) -> DetailSectionTint {
    if dirty.persisted > 0 {
        DetailSectionTint::Warning
    } else {
        DetailSectionTint::None
    }
}

/// The live section wears the dedicated live (blue) tint whenever transient
/// controls are touched — matching live slot rows, distinct from the yellow
/// unsaved treatment.
fn live_section_tint(dirty: &DirtySummary) -> DetailSectionTint {
    if dirty.transient > 0 {
        DetailSectionTint::Live
    } else {
        DetailSectionTint::None
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::core::status::UiStatusKind;
    use lpa_studio_core::{ProjectNodeTreeView, ProjectSyncSummary};

    use super::*;

    fn dirty(persisted: usize, transient: usize, failed: usize) -> DirtySummary {
        DirtySummary {
            persisted,
            transient,
            failed,
        }
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
        let uncommitted = editor_view(dirty(1, 0, 0), 1).affordance(UiStatusKind::Good);
        assert_eq!(uncommitted, UiAffordance::Unsaved);
        assert_eq!(state_label(uncommitted), "uncommitted");

        // In-flight only: genuine activity.
        let busy = editor_view(dirty(0, 0, 0), 1).affordance(UiStatusKind::Good);
        assert_eq!(busy, UiAffordance::Busy);
        assert_eq!(state_label(busy), "in progress");

        // Live-only edits stay distinct from unsaved.
        let live = editor_view(dirty(0, 2, 0), 0).affordance(UiStatusKind::Good);
        assert_eq!(live, UiAffordance::Live);
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
        assert_eq!(tone(dirty(1, 0, 1), 2, UiStatusKind::Good), PaneTone::Error);
        assert_eq!(
            tone(dirty(2, 1, 0), 0, UiStatusKind::Good),
            PaneTone::Warning
        );
        assert_eq!(tone(dirty(0, 1, 0), 0, UiStatusKind::Good), PaneTone::Live);
        assert_eq!(
            tone(dirty(0, 0, 0), 1, UiStatusKind::Good),
            PaneTone::Working
        );
        // An error pane status is never masked by a dirty wash.
        assert_eq!(
            tone(dirty(0, 1, 0), 0, UiStatusKind::Error),
            PaneTone::Error
        );
    }

    #[test]
    fn popup_sections_tint_only_their_dirty_bucket() {
        assert_eq!(
            unsaved_section_tint(&dirty(1, 0, 0)),
            DetailSectionTint::Warning
        );
        assert_eq!(
            unsaved_section_tint(&dirty(0, 2, 0)),
            DetailSectionTint::None
        );
        assert_eq!(live_section_tint(&dirty(0, 2, 0)), DetailSectionTint::Live);
        assert_eq!(live_section_tint(&dirty(1, 0, 0)), DetailSectionTint::None);
    }
}
