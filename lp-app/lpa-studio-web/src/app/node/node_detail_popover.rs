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
use lpa_studio_core::{
    ModuleExportOp, NodeCopyOp, ProjectController, ProjectNodeAddress, UiAction, UiModuleExport,
    UiModuleFace, UiNodeHeader, UiPendingEdit,
};

use crate::app::affordance::affordance_trigger_style;
use crate::app::module::ExportFindingRow;
use crate::app::project::pending_edit_section::{
    PendingEditBucket, PendingEditList, bucket_section_tint, entries_in,
};
use crate::base::{DetailPopover, DetailSection, DetailSectionTint, StudioIcon, StudioIconName};
use crate::core::inline_link_row_class;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn NodeDetailPopover(
    header: UiNodeHeader,
    /// The editor-level pending-edit list; the popover filters it to the
    /// entries addressed to THIS node (`node_path` == the header path).
    #[props(default)]
    pending_edits: Vec<UiPendingEdit>,
    /// The module face's own extras, when this node wears one: the export
    /// designation row (module authoring unit, P3) and the compact
    /// provenance line the face already derives. Both are module-only, so
    /// they arrive as an option rather than as header fields.
    #[props(default = None)]
    module: Option<UiModuleFace>,
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
    let failed_entries = entries_in(&own_edits, PendingEditBucket::Failed);
    // The header path is the node address the copy op needs; a header
    // whose path does not parse (never in production) simply offers no
    // share row rather than dispatching a malformed op.
    let copy_target = ProjectNodeAddress::parse(&header.path).ok();
    let provenance = module.as_ref().and_then(|face| face.provenance.clone());
    let export = module.as_ref().and_then(|face| face.export.clone());
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
                    // A module's authored provenance sits beside its
                    // identity, where the export section below can point at
                    // it: provenance is what an importer inherits.
                    if let Some(provenance) = provenance.clone() {
                        NodeDetailRow { label: "provenance", value: provenance }
                    }
                }
            }
            // Export designation (module authoring unit, P3): the gesture
            // lives on the module you are looking at, even though what it
            // edits is the project's manifest — so the checkbox names the
            // project.
            if let Some(export) = export.clone() {
                DetailSection { title: "Export", tint: DetailSectionTint::Export,
                    ModuleExportRow { export, on_action: forward }
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
            if dirty.failed > 0 {
                DetailSection {
                    title: "Failed edits",
                    meta: dirty.failed.to_string(),
                    tint: bucket_section_tint(PendingEditBucket::Failed, dirty.failed),
                    PendingEditList { entries: failed_entries, on_action: forward }
                }
            }
            // Sharing: copy this node (def + assets) as an `lp.node`
            // envelope. Needs the node's address to read its files, so a
            // header without a parseable path renders no share section.
            if let Some(node) = copy_target {
                DetailSection { title: "Share",
                    button {
                        class: inline_link_row_class(false),
                        r#type: "button",
                        title: "Copy this node and its assets to the clipboard.",
                        onclick: move |event| {
                            event.stop_propagation();
                            forward.call(UiAction::from_op(
                                ProjectController::NODE_ID,
                                NodeCopyOp { node: node.clone() },
                            ));
                        },
                        span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:flex-none tw:items-center tw:justify-center", aria_hidden: "true",
                            StudioIcon { name: StudioIconName::Copy, size: 14 }
                        }
                        span { class: "tw:min-w-0 tw:truncate", "Copy JSON" }
                    }
                    if dirty.persisted > 0 {
                        p { class: "tw:m-0 tw:pt-1 tw:text-[0.68rem] tw:leading-snug tw:text-subtle-foreground",
                            "Copies the last saved version — this node has unsaved edits."
                        }
                    }
                }
            }
        }
    }
}

/// The export designation row: a checkbox naming the project, the hint that
/// says what an importer actually gets, and this export's lint findings.
///
/// A row with a `disabled_reason` still renders — it explains why the box
/// cannot be ticked rather than vanishing (the add-node picker's
/// disabled-row precedent) — and dispatches nothing.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ModuleExportRow(export: UiModuleExport, on_action: EventHandler<UiAction>) -> Element {
    let disabled = export.disabled_reason.clone();
    let blocked = disabled.is_some();
    let folder = export.folder.clone();
    let designated = export.designated;
    let label_class = if blocked {
        "tw:flex tw:min-w-0 tw:cursor-not-allowed tw:items-start tw:gap-2 tw:py-0.5 tw:text-xs tw:text-subtle-foreground tw:opacity-60"
    } else {
        "tw:flex tw:min-w-0 tw:cursor-pointer tw:items-start tw:gap-2 tw:py-0.5 tw:text-xs tw:text-muted-foreground"
    };

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
            label { class: label_class,
                input {
                    class: "tw:mt-0.5 tw:flex-none tw:accent-[var(--studio-status-export-text)]",
                    r#type: "checkbox",
                    checked: designated,
                    disabled: blocked,
                    onclick: move |event| {
                        event.stop_propagation();
                        if !blocked {
                            on_action.call(UiAction::from_op(
                                ProjectController::NODE_ID,
                                ModuleExportOp {
                                    folder: folder.clone(),
                                    export: !designated,
                                },
                            ));
                        }
                    },
                }
                span { class: "tw:min-w-0",
                    "Export from "
                    strong { class: "tw:text-strong-foreground", "{export.project}" }
                }
            }
            if let Some(reason) = disabled {
                p { class: "tw:m-0 tw:text-[0.68rem] tw:leading-snug tw:text-subtle-foreground",
                    "{reason}"
                }
            } else {
                p { class: "tw:m-0 tw:text-[0.68rem] tw:leading-snug tw:text-subtle-foreground",
                    "Importers vendor "
                    span { class: "tw:font-mono", "{export.folder}/" }
                    " as their own copy. The folder must be self-contained; its bus consumers are its interface."
                }
                if export.upgrades_to_pattern {
                    p { class: "tw:m-0 tw:text-[0.68rem] tw:leading-snug tw:text-status-export-foreground",
                        "The first export makes "
                        strong { "{export.project}" }
                        " a pattern project."
                    }
                }
            }
            for finding in export.findings.iter() {
                ExportFindingRow { finding: finding.clone() }
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
        UiStatusKind::Attention => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-attention-foreground"
        }
        UiStatusKind::Error => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-error-foreground"
        }
    }
}
