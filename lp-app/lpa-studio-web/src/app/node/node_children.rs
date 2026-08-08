//! A card's children, as full sibling cards on the nesting rail.
//!
//! On the ROOT card of a project that exports something, the rail is
//! SECTIONED (module authoring unit, G1 R-A): the export folders' cards
//! come first under an `exports` header wearing the sage export family,
//! with the aggregate lint findings as a preamble above them, and
//! everything else follows under a bare hairline. The remainder gets no
//! word at all (G1 follow-up ruling): "rig" is a PROJECT type, and these
//! are just the nodes that stay home — the boundary is the divider, and
//! membership is already on the exported cards' chips.
//!
//! P3 put this on the root CARD as a rail of manifest names. It reads
//! better as a property of the cards themselves: the exports are right
//! there in the column, so "what does this project hand out" is answered by
//! where a card sits rather than by a list that names it somewhere else.
//!
//! A project with no exports renders exactly as it always did — one
//! ungrouped column, no headers (spike 2·ii).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiExportsGroup, UiNodeChild, UiNodeView, UiPendingEdit};

use crate::app::module::ExportFindingRow;
use crate::app::node::{NodeDirtyTint, NodePane};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeChildren(
    items: Vec<UiNodeChild>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The editor-level pending-edit list, threaded into every child pane
    /// (each pane's detail popover filters it to its own node).
    #[props(default)]
    pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
    /// Panel gestures raised by a module child's face, passed
    /// down so a child module's panel is as live as its host's.
    #[props(default = None)]
    module_panel: Option<EventHandler<crate::app::module::PanelGesture>>,
    /// The exports/rig split for THIS card's children — core-owned
    /// (`UiNodeView::exports`), carried by the root card of a project that
    /// exports something and `None` everywhere else.
    #[props(default = None)]
    exports: Option<UiExportsGroup>,
) -> Element {
    // Partition, never re-order: within each section the children keep the
    // order the column already had.
    let split = exports.filter(|group| !group.keys.is_empty()).map(|group| {
        let (exported, rig): (Vec<UiNodeChild>, Vec<UiNodeChild>) = items
            .iter()
            .cloned()
            .partition(|child| group.keys.contains(&child.detail));
        (group, exported, rig)
    });

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:border-l tw:border-border-muted tw:pl-4",
            if let Some((group, exported, rig)) = split {
                // A manifest whose names match no card renders no header —
                // an empty section is a label pointing at nothing.
                if !exported.is_empty() {
                    ChildGroupHeader { label: "exports" }
                    // The aggregate preamble: one line per finding, the
                    // same sentence the module's own popup shows.
                    if !group.findings.is_empty() {
                        div { class: "tw:grid tw:min-w-0 tw:gap-1",
                            for finding in group.findings.iter() {
                                ExportFindingRow { finding: finding.clone() }
                            }
                        }
                    }
                    for child in exported {
                        NodePane {
                            key: "{child.label}",
                            view: child_node_view(child),
                            on_action,
                            pending_edits: pending_edits.clone(),
                            dirty_tint,
                            module_panel,
                        }
                    }
                }
                if !rig.is_empty() {
                    // No label: the remainder is not a named thing, just
                    // "the rest". The hairline alone closes the exports
                    // section.
                    span {
                        class: "tw:mt-1 tw:h-px tw:min-w-0 tw:bg-border-muted",
                        aria_hidden: "true",
                    }
                    for child in rig {
                        NodePane {
                            key: "{child.label}",
                            view: child_node_view(child),
                            on_action,
                            pending_edits: pending_edits.clone(),
                            dirty_tint,
                            module_panel,
                        }
                    }
                }
            } else {
                for child in items {
                    NodePane {
                        key: "{child.label}",
                        view: child_node_view(child),
                        on_action,
                        pending_edits: pending_edits.clone(),
                        dirty_tint,
                        module_panel,
                    }
                }
            }
        }
    }
}

/// The exports section header: the card grammar's small-caps label
/// typography in the sage export family, a hairline running out to the
/// column's edge. The remainder section below it is deliberately unlabeled
/// — see the module doc.
///
/// Deliberately NOT a card and not a box — the cards below it are the
/// content; this is a divider that happens to be named.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChildGroupHeader(label: &'static str) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:pt-1",
            span { class: "tw:select-none tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.14em] tw:text-status-export-foreground",
                "{label}"
            }
            span {
                class: "tw:h-px tw:min-w-0 tw:flex-1 tw:bg-status-export-border",
                aria_hidden: "true",
            }
        }
    }
}

/// Promote an extracted child summary to a full pane view (nested cards are
/// the same card grammar) — the playlist's active child renders below its
/// playlist card through this same path (P2c item 2).
///
/// The mapping lives in core ([`UiNodeChild::into_node_view`]) so the
/// renderer and the core-side workspace scans promote a child card exactly
/// the same way.
fn child_node_view(child: UiNodeChild) -> UiNodeView {
    child.into_node_view()
}
