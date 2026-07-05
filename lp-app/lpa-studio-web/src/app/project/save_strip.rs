//! Header save strip for the project pending-edit overlay.
//!
//! Compact strip shown while a project editor is connected: a persisted
//! change count, **Save** (`ProjectOp::SaveOverlay`), **Revert to saved**
//! (`ProjectOp::RevertAllEdits`), and a state icon summarizing the overlay —
//! unchanged / uncommitted / in progress — that opens a small placeholder
//! popup (overlay revision + per-kind counts). The popup deliberately stays
//! minimal; the full "what changed" panel is M3.

use dioxus::prelude::*;
use lpa_studio_core::{ControllerId, ProjectController, ProjectDirtyCounts, ProjectOp, UiAction};

use crate::base::{IconMenuButton, IconMenuTone, PopoverPlacement, StudioIconName};

/// Overall overlay state conveyed by the strip's state icon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SaveStripState {
    /// No pending persisted edits; nothing for Save to write.
    Unchanged,
    /// Persisted edits are pending in the overlay, not yet saved.
    Uncommitted,
    /// An edit or save operation is awaiting its server acknowledgement.
    InProgress,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectSaveStrip(
    dirty: ProjectDirtyCounts,
    overlay_revision: i64,
    edits_in_flight: usize,
    on_action: EventHandler<UiAction>,
    /// Open the detail popup immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
) -> Element {
    let state = save_strip_state(&dirty, edits_in_flight);
    let save_disabled = dirty.persisted == 0;
    let revert_disabled = dirty.is_clean();
    let save_action = project_action(ProjectOp::SaveOverlay);
    let revert_action = project_action(ProjectOp::RevertAllEdits).with_label("Revert to saved");

    rsx! {
        div { class: "tw:flex tw:items-center tw:gap-2",
            SaveStripStateIcon {
                state,
                dirty,
                overlay_revision,
                edits_in_flight,
                initially_open,
            }
            if dirty.persisted > 0 {
                span {
                    class: "tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:text-status-warning-foreground",
                    title: "Pending persisted edits Save will write",
                    "{dirty.persisted} unsaved"
                }
            }
            SaveStripButton {
                label: "Save",
                title: "Write pending persisted edits back to the project files",
                primary: true,
                disabled: save_disabled,
                action: save_action,
                on_action,
            }
            SaveStripButton {
                label: "Revert to saved",
                title: "Discard every pending edit on this project",
                primary: false,
                disabled: revert_disabled,
                action: revert_action,
                on_action,
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SaveStripButton(
    label: &'static str,
    title: &'static str,
    primary: bool,
    disabled: bool,
    action: UiAction,
    on_action: EventHandler<UiAction>,
) -> Element {
    let class = if primary {
        "tw:inline-flex tw:min-h-7 tw:items-center tw:gap-1.5 tw:rounded-sm tw:border tw:border-accent-border tw:bg-accent tw:px-2.5 tw:text-xs tw:font-bold tw:leading-none tw:text-accent-foreground tw:hover:bg-accent-hover tw:disabled:cursor-not-allowed tw:disabled:opacity-50"
    } else {
        "tw:inline-flex tw:min-h-7 tw:items-center tw:gap-1.5 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-2.5 tw:text-xs tw:font-bold tw:leading-none tw:text-soft-foreground tw:hover:bg-card-raised-strong tw:disabled:cursor-not-allowed tw:disabled:opacity-50"
    };
    rsx! {
        button {
            class,
            r#type: "button",
            disabled,
            title,
            onclick: move |_| on_action.call(action.clone()),
            "{label}"
        }
    }
}

/// State icon plus the placeholder detail popup (overlay revision and
/// per-kind pending-edit counts).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SaveStripStateIcon(
    state: SaveStripState,
    dirty: ProjectDirtyCounts,
    overlay_revision: i64,
    edits_in_flight: usize,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let (icon, tone, label) = match state {
        SaveStripState::Unchanged => (
            StudioIconName::StepComplete,
            IconMenuTone::Quiet,
            "No unsaved project changes",
        ),
        SaveStripState::Uncommitted => (
            StudioIconName::Edited,
            IconMenuTone::Warning,
            "Project has unsaved changes",
        ),
        SaveStripState::InProgress => (
            StudioIconName::StatusRunning,
            IconMenuTone::Working,
            "Edit in progress",
        ),
    };

    rsx! {
        IconMenuButton {
            icon,
            label: label.to_string(),
            tone,
            placement: PopoverPlacement::BottomEnd,
            active: state != SaveStripState::Unchanged,
            initially_open,
            div { class: "tw:grid tw:gap-1 tw:px-3 tw:py-2",
                h3 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Pending edits" }
                SaveStripDetailRow { label: "State", value: state_label(state).to_string() }
                SaveStripDetailRow { label: "Overlay revision", value: overlay_revision.to_string() }
                SaveStripDetailRow { label: "Unsaved (persisted)", value: dirty.persisted.to_string() }
                SaveStripDetailRow { label: "Live (transient)", value: dirty.transient.to_string() }
                if edits_in_flight > 0 {
                    SaveStripDetailRow { label: "Awaiting ack", value: edits_in_flight.to_string() }
                }
                p { class: "tw:m-0 tw:pt-1 tw:text-[0.68rem] tw:leading-snug tw:text-subtle-foreground",
                    "Live controls apply to the running project and are never written by Save."
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SaveStripDetailRow(label: &'static str, value: String) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:flex tw:items-baseline tw:justify-between tw:gap-3 tw:text-xs tw:leading-snug",
            span { class: "tw:font-bold tw:text-subtle-foreground", "{label}" }
            span { class: "tw:font-mono tw:text-muted-foreground", "{value}" }
        }
    }
}

fn save_strip_state(dirty: &ProjectDirtyCounts, edits_in_flight: usize) -> SaveStripState {
    if edits_in_flight > 0 {
        SaveStripState::InProgress
    } else if dirty.persisted > 0 {
        SaveStripState::Uncommitted
    } else {
        SaveStripState::Unchanged
    }
}

fn state_label(state: SaveStripState) -> &'static str {
    match state {
        SaveStripState::Unchanged => "unchanged",
        SaveStripState::Uncommitted => "uncommitted",
        SaveStripState::InProgress => "in progress",
    }
}

fn project_action(op: ProjectOp) -> UiAction {
    UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op)
}
