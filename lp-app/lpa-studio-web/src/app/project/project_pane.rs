//! The project pane: the whole project card as ONE `StudioPane` (UX gate
//! feedback on D4/D5 — no second header above the node tree).
//!
//! Header: the project *name* as the title (never the literal word
//! "project" — that is the kind label), the controller's pane status as a
//! compact chip ("Ready", "Syncing", …), a dirty/status tone wash,
//! contextual Save / Revert-to-saved icon actions supplied by the controller
//! (`ProjectEditorView.header_actions`), and a `DetailPopover` at the right
//! edge. Dirty counts ("N unsaved" / "N live") are deliberately NOT header
//! chips — too cramped; they live in the detail popup, together with the
//! project stats that used to be a sidebar card.
//!
//! Body: the node tree (plus any sync issue and the pane-level
//! Refresh/Disconnect actions). The popup stays at M2's revision + counts
//! detail level; the full "what changed" panel is M3.

use dioxus::prelude::*;
use lpa_studio_core::core::status::UiStatusKind;
use lpa_studio_core::{DirtySummary, ProjectEditorView, UiAction, UiMetric, UiStatus};

use crate::app::layout::{PaneChip, PaneChrome, PaneTone, StudioPane};
use crate::app::node::status_pane_tone;
use crate::app::project::ProjectNodeTree;
use crate::base::{
    DetailPopover, DetailSectionTint, IconMenuTone, PopoverPlacement, StudioIconName,
    detail_popover_section_class,
};
use crate::core::ActionStrip;

/// Overall overlay state summarized by the header's detail trigger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectPaneState {
    /// No pending persisted edits; nothing for Save to write.
    Unchanged,
    /// Persisted edits are pending in the overlay, not yet saved.
    Uncommitted,
    /// An edit or save operation is awaiting its server acknowledgement.
    InProgress,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectPane(
    view: ProjectEditorView,
    /// Pane-level status from the project controller ("Ready", "Syncing", …),
    /// shown as the header's compact state chip.
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
    let state = project_pane_state(&dirty, edits_in_flight);
    let chrome = PaneChrome {
        tone: project_pane_tone(status.kind, &dirty, edits_in_flight),
        accent: false,
        chips: vec![project_status_chip(&status)],
    };
    let overlay_revision = view.sync.overlay_revision;
    let sync_issue = view.sync.issue.clone();
    let stats = view.stats.clone();
    let roots = view.tree.roots.clone();
    let header_actions = view.header_actions.clone();

    rsx! {
        StudioPane {
            title: view.project_name.clone(),
            kind: "Project".to_string(),
            chrome,
            actions: header_actions,
            on_action,
            detail: rsx! {
                ProjectDetailPopover {
                    state,
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

/// The detail popup on the shared [`DetailPopover`] base: overlay revision,
/// awaiting-ack, the per-bucket dirty counts with their status tints, and the
/// project stats (moved here from the old sidebar MetricGrid card).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectDetailPopover(
    state: ProjectPaneState,
    dirty: DirtySummary,
    overlay_revision: i64,
    edits_in_flight: usize,
    stats: Vec<UiMetric>,
    #[props(default = false)] initially_open: bool,
) -> Element {
    // Trigger icon discipline (UX gate): the default trigger is the "i"
    // info glyph matching slot rows; only the genuinely attention-needing
    // uncommitted state keeps its edited glyph. Status tones stay.
    let (icon, tone, label) = match state {
        ProjectPaneState::Unchanged => (
            StudioIconName::InfoBare,
            IconMenuTone::Quiet,
            "Project details — no unsaved changes",
        ),
        ProjectPaneState::Uncommitted => (
            StudioIconName::Edited,
            IconMenuTone::Warning,
            "Project has unsaved changes",
        ),
        ProjectPaneState::InProgress => (
            StudioIconName::InfoBare,
            IconMenuTone::Working,
            "Edit in progress",
        ),
    };

    rsx! {
        DetailPopover {
            icon,
            label: label.to_string(),
            tone,
            placement: PopoverPlacement::BottomEnd,
            active: state != ProjectPaneState::Unchanged,
            initially_open,
            section { class: "tw:grid tw:gap-1 tw:px-3 tw:py-2",
                h3 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Pending edits" }
                ProjectDetailRow { label: "State", value: state_label(state).to_string() }
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

fn project_pane_state(dirty: &DirtySummary, edits_in_flight: usize) -> ProjectPaneState {
    if edits_in_flight > 0 {
        ProjectPaneState::InProgress
    } else if dirty.persisted > 0 {
        ProjectPaneState::Uncommitted
    } else {
        ProjectPaneState::Unchanged
    }
}

fn state_label(state: ProjectPaneState) -> &'static str {
    match state {
        ProjectPaneState::Unchanged => "unchanged",
        ProjectPaneState::Uncommitted => "uncommitted",
        ProjectPaneState::InProgress => "in progress",
    }
}

/// Header tone: failed edits wash red (and an error status is never masked),
/// an in-flight op washes working, otherwise the dominant dirty bucket
/// (unsaved > live, per D6); a clean idle project keeps its status tone.
fn project_pane_tone(
    status: UiStatusKind,
    dirty: &DirtySummary,
    edits_in_flight: usize,
) -> PaneTone {
    if dirty.failed > 0 || status == UiStatusKind::Error {
        PaneTone::Error
    } else if edits_in_flight > 0 {
        PaneTone::Working
    } else if dirty.persisted > 0 {
        PaneTone::Warning
    } else if dirty.transient > 0 {
        PaneTone::Live
    } else {
        status_pane_tone(status)
    }
}

/// The header's one compact state chip: the controller's pane status
/// ("Ready", "Syncing", …), toned like node status chips. Dirty counts stay
/// out of the header (detail popup); dirty state shows as the header tone.
fn project_status_chip(status: &UiStatus) -> PaneChip {
    PaneChip {
        tone: status_pane_tone(status.kind),
        text: status.label.clone(),
        title: format!("Project status: {}", status.label),
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
    use super::*;

    fn dirty(persisted: usize, transient: usize, failed: usize) -> DirtySummary {
        DirtySummary {
            persisted,
            transient,
            failed,
        }
    }

    #[test]
    fn state_tracks_in_flight_then_persisted() {
        assert_eq!(
            project_pane_state(&dirty(0, 0, 0), 0),
            ProjectPaneState::Unchanged
        );
        assert_eq!(
            project_pane_state(&dirty(0, 2, 0), 0),
            ProjectPaneState::Unchanged
        );
        assert_eq!(
            project_pane_state(&dirty(1, 0, 0), 0),
            ProjectPaneState::Uncommitted
        );
        assert_eq!(
            project_pane_state(&dirty(1, 0, 0), 1),
            ProjectPaneState::InProgress
        );
    }

    #[test]
    fn clean_idle_project_keeps_its_status_tone_and_status_chip() {
        assert_eq!(
            project_pane_tone(UiStatusKind::Good, &DirtySummary::clean(), 0),
            PaneTone::Good
        );
        assert_eq!(
            project_pane_tone(UiStatusKind::Neutral, &DirtySummary::clean(), 0),
            PaneTone::Neutral
        );

        let chip = project_status_chip(&UiStatus::good("Ready"));
        assert_eq!(chip.tone, PaneTone::Good);
        assert_eq!(chip.text, "Ready");
    }

    #[test]
    fn header_tone_prefers_failed_then_in_flight_then_unsaved_then_live() {
        assert_eq!(
            project_pane_tone(UiStatusKind::Good, &dirty(1, 0, 1), 2),
            PaneTone::Error
        );
        assert_eq!(
            project_pane_tone(UiStatusKind::Good, &dirty(1, 0, 0), 2),
            PaneTone::Working
        );
        assert_eq!(
            project_pane_tone(UiStatusKind::Good, &dirty(2, 1, 0), 0),
            PaneTone::Warning
        );
        assert_eq!(
            project_pane_tone(UiStatusKind::Good, &dirty(0, 1, 0), 0),
            PaneTone::Live
        );
        // An error pane status is never masked by a dirty wash.
        assert_eq!(
            project_pane_tone(UiStatusKind::Error, &dirty(0, 1, 0), 0),
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
