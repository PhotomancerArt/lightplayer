//! The node's detail popover behind its one header affordance: the merged
//! affordance (status + subtree dirty, computed in core) is the trigger's
//! glyph + tone; the popover carries all the text — identity, the status
//! word, and the per-bucket dirty sections (their only home since the P6
//! affordance model removed the header count chips). Each dirty bucket is a
//! titled [`DetailSection`] (subtree count in the title row's meta cell)
//! listing the node's OWN pending edits with their per-entry reverts — the
//! same shared `PendingEditList` the project save panel uses; descendants'
//! edits stay in the counts and in their own nodes' popups.
//!
//! Rendered into the node pane's detail slot at the header's right edge; the
//! header layout itself is the shared `StudioPane` component.

use dioxus::prelude::*;
use lpa_studio_core::core::status::UiStatusKind;
use lpa_studio_core::{UiAction, UiNodeHeader, UiPendingEdit};

use crate::app::affordance::affordance_trigger_style;
use crate::app::project::pending_edit_section::{
    PendingEditBucket, PendingEditList, bucket_section_tint, entries_in,
};
use crate::base::{DetailPopover, DetailSection};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn NodeDetailPopover(
    header: UiNodeHeader,
    /// The editor-level pending-edit list; the popover filters it to the
    /// entries addressed to THIS node (`node_path` == the header path).
    #[props(default)]
    pending_edits: Vec<UiPendingEdit>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let dirty = header.dirty;
    let affordance = header.affordance();
    let style = affordance_trigger_style(affordance);
    let label = format!("{} details", header.title);
    let own_edits: Vec<UiPendingEdit> = pending_edits
        .into_iter()
        .filter(|edit| edit.node_path == header.path)
        .collect();
    let unsaved_entries = entries_in(&own_edits, PendingEditBucket::Persisted);
    let live_entries = entries_in(&own_edits, PendingEditBucket::Live);
    let failed_entries = entries_in(&own_edits, PendingEditBucket::Failed);
    let forward = EventHandler::new(move |action: UiAction| {
        if let Some(handler) = on_action {
            handler.call(action);
        }
    });

    rsx! {
        DetailPopover {
            icon: style.icon,
            label,
            tone: style.tone,
            active: affordance.is_announced(),
            initially_open,
            DetailSection {
                div { class: "tw:flex tw:min-w-0 tw:items-start tw:justify-between tw:gap-4 tw:py-1",
                    div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                        strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words", "{header.title}" }
                        span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground", "{header.kind}" }
                    }
                    span { class: node_status_label_class(header.status.kind), "{header.status.label}" }
                }
            }
            DetailSection {
                dl { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-1.5 tw:py-1 tw:text-xs",
                    if let Some(summary) = header.summary.as_ref() {
                        NodeDetailRow { label: "summary", value: summary.clone() }
                    }
                    if let Some(source) = header.source.as_ref() {
                        NodeDetailRow { label: "source", value: source.clone() }
                    }
                    NodeDetailRow { label: "path", value: header.path.clone() }
                }
            }
            if let Some(detail) = header.detail.as_ref() {
                DetailSection {
                    p { class: "tw:m-0 tw:py-1 tw:text-xs tw:leading-normal tw:text-muted-foreground tw:break-words", "{detail}" }
                }
            }
            if dirty.persisted > 0 {
                DetailSection {
                    title: "Unsaved (persisted)",
                    meta: dirty.persisted.to_string(),
                    tint: bucket_section_tint(PendingEditBucket::Persisted, dirty.persisted),
                    PendingEditList { entries: unsaved_entries, on_action: forward }
                }
            }
            if dirty.transient > 0 {
                DetailSection {
                    title: "Live (transient)",
                    meta: dirty.transient.to_string(),
                    tint: bucket_section_tint(PendingEditBucket::Live, dirty.transient),
                    PendingEditList { entries: live_entries, on_action: forward }
                }
            }
            if dirty.failed > 0 {
                DetailSection {
                    title: "Failed edits",
                    meta: dirty.failed.to_string(),
                    tint: bucket_section_tint(PendingEditBucket::Failed, dirty.failed),
                    PendingEditList { entries: failed_entries, on_action: forward }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeDetailRow(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[72px_minmax(0,1fr)] tw:gap-2",
            dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{label}" }
            dd { class: "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words", "{value}" }
        }
    }
}

/// Toned pill for the status word inside detail popups — the popup is where
/// status text lives now that headers and tree rows only carry affordances.
pub(crate) fn node_status_label_class(kind: UiStatusKind) -> &'static str {
    match kind {
        UiStatusKind::Neutral => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-neutral-foreground"
        }
        UiStatusKind::Working => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-working-border tw:bg-status-working-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-working-foreground"
        }
        UiStatusKind::Good => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-good-border tw:bg-status-good-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-good-foreground"
        }
        UiStatusKind::Warning => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-warning-foreground"
        }
        UiStatusKind::Error => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-error-foreground"
        }
    }
}
