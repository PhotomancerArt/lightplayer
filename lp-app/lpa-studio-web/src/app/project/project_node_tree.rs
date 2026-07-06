//! Project sidebar node tree: rows carrying the one-glyph affordance.
//!
//! Each row renders its core-computed `UiAffordance` as a small indicator —
//! only when announced (clean, healthy rows stay silent; no status words, no
//! count badges: the breakdown lives in the row tooltip, the counts in the
//! node/project detail popups). Dirty rows keep the node-header tint
//! treatment (the P3 header-only gradient over the subtle card surface); the
//! dominant dirty status color is layered through a `--studio-tree-dirty-bg`
//! CSS custom property so the selection highlight can *derive* from it: a
//! selected dirty row color-mixes the dirty color into the selection
//! background instead of flatly overriding the edited treatment.

use dioxus::prelude::*;
use lpa_studio_core::{DirtySummary, ProjectNodeStatusView, ProjectNodeTreeItem, UiAction};

use crate::app::affordance::{affordance_indicator_class, affordance_trigger_style};
use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectNodeTree(
    roots: Vec<ProjectNodeTreeItem>,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        if roots.is_empty() {
            p { class: "tw:m-0 tw:text-sm tw:text-subtle-foreground", "Project sync has not returned nodes yet." }
        } else {
            ol { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:p-0",
                for item in roots {
                    ProjectNodeTreeItemView {
                        key: "{item.node_id}",
                        item,
                        depth: 0,
                        running,
                        on_action,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectNodeTreeItemView(
    item: ProjectNodeTreeItem,
    depth: usize,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let focused = item.focused;
    let action = item.action.clone();
    let affordance = item.affordance();
    let children = item.children;
    let dirty = item.dirty;
    let class = tree_item_row_class(focused, dirty);
    let title = tree_item_title(&item.kind, &item.status, dirty);
    let indent = depth * 14;
    let label = item.label;

    rsx! {
        li {
            button {
                class,
                r#type: "button",
                disabled: running,
                style: "padding-left: {indent}px;",
                title: "{title}",
                onclick: move |_| on_action.call(action.clone()),
                span { class: "tw:inline-flex tw:h-4 tw:w-4 tw:items-center tw:justify-center tw:text-subtle-foreground",
                    StudioIcon {
                        name: StudioIconName::NodeTreeItem,
                        size: 14,
                    }
                }
                span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-sm tw:text-soft-foreground", "{label}" }
                // The one-glyph affordance indicator; silent when Info (the
                // tooltip and the popups carry the words and counts).
                if let Some(indicator_class) = affordance_indicator_class(affordance) {
                    span { class: "{indicator_class}",
                        StudioIcon {
                            name: affordance_trigger_style(affordance).icon,
                            size: 12,
                        }
                    }
                }
            }
            if !children.is_empty() {
                ol { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:p-0",
                    for child in children {
                        ProjectNodeTreeItemView {
                            key: "{child.node_id}",
                            item: child,
                            depth: depth + 1,
                            running,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// Row class for one tree item, built from two CSS background layers driven
/// by one custom property:
///
/// - dirty rows set `--studio-tree-dirty-bg` to the dominant dirty status
///   color (failed > unsaved > live, the pane-chip precedence);
/// - an unfocused dirty row paints the node-header tint: the same
///   `linear-gradient(90deg, <status bg>, transparent 62%)` over the subtle
///   card surface as the P3 header-only treatment;
/// - the focused row keeps its neutral selection border, and its highlight fill
///   color-mixes the dirty color into the selection background so selection
///   adapts to (never erases) the edited treatment; on a clean focused row
///   the variable falls back to the selection color, mixing it with itself.
fn tree_item_row_class(focused: bool, dirty: DirtySummary) -> String {
    const BASE: &str = "tw:grid tw:w-full tw:grid-cols-[18px_minmax(0,1fr)_auto] tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:px-2 tw:py-1.5 tw:text-left";
    let dirty_var = tree_item_dirty_var_class(dirty);
    if focused {
        return format!(
            "{BASE} {dirty_var} tw:border-selection-border tw:bg-[color-mix(in_oklab,var(--studio-tree-dirty-bg,var(--studio-color-selection-bg))_45%,var(--studio-color-selection-bg))]"
        );
    }
    if dirty.is_clean() {
        format!("{BASE} tw:border-transparent tw:bg-transparent tw:hover:bg-card-muted")
    } else {
        format!(
            "{BASE} {dirty_var} tw:border-transparent tw:bg-card-subtle tw:bg-[linear-gradient(90deg,var(--studio-tree-dirty-bg),transparent_62%)] tw:hover:bg-card-muted"
        )
    }
}

/// The CSS-variable layer: `--studio-tree-dirty-bg` carries the dominant
/// dirty status color for the row's background layers; clean rows leave the
/// variable unset so consumers fall back.
fn tree_item_dirty_var_class(dirty: DirtySummary) -> &'static str {
    if dirty.failed > 0 {
        "tw:[--studio-tree-dirty-bg:var(--studio-status-error-bg)]"
    } else if dirty.persisted > 0 {
        "tw:[--studio-tree-dirty-bg:var(--studio-status-warning-bg)]"
    } else if dirty.transient > 0 {
        "tw:[--studio-tree-dirty-bg:var(--studio-status-live-bg)]"
    } else {
        ""
    }
}

/// Row tooltip: the node kind and status word (banished from the visible
/// row by the affordance model), plus the per-bucket dirty breakdown the
/// deleted count badge used to carry.
fn tree_item_title(kind: &str, status: &ProjectNodeStatusView, dirty: DirtySummary) -> String {
    let mut title = format!("{kind} — {}", status.label);
    if let Some(detail) = status.detail.as_ref() {
        title.push_str(&format!(": {detail}"));
    }
    if dirty.is_clean() {
        return title;
    }
    let mut parts = Vec::new();
    if dirty.persisted > 0 {
        parts.push(format!("{} unsaved", dirty.persisted));
    }
    if dirty.transient > 0 {
        parts.push(format!("{} live", dirty.transient));
    }
    if dirty.failed > 0 {
        parts.push(format!("{} failed", dirty.failed));
    }
    format!("{title} — edits in this subtree: {}", parts.join(", "))
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
    fn clean_row_keeps_the_plain_background_and_sets_no_dirty_variable() {
        let class = tree_item_row_class(false, DirtySummary::clean());
        assert!(class.contains("tw:bg-transparent"));
        assert!(!class.contains("linear-gradient"));
        assert!(!class.contains("--studio-tree-dirty-bg:"));
    }

    #[test]
    fn dirty_row_wears_the_node_header_tint_in_the_dominant_bucket_color() {
        let unsaved = tree_item_row_class(false, dirty(2, 0, 0));
        assert!(unsaved.contains("tw:[--studio-tree-dirty-bg:var(--studio-status-warning-bg)]"));
        assert!(unsaved.contains(
            "tw:bg-[linear-gradient(90deg,var(--studio-tree-dirty-bg),transparent_62%)]"
        ));
        // The header tint sits on the subtle card surface, like pane headers.
        assert!(unsaved.contains("tw:bg-card-subtle"));

        assert!(
            tree_item_row_class(false, dirty(0, 1, 0))
                .contains("--studio-tree-dirty-bg:var(--studio-status-live-bg)")
        );
        assert!(
            tree_item_row_class(false, dirty(1, 1, 1))
                .contains("--studio-tree-dirty-bg:var(--studio-status-error-bg)")
        );
    }

    #[test]
    fn focused_dirty_row_mixes_the_dirty_color_into_the_selection_highlight() {
        let class = tree_item_row_class(true, dirty(2, 0, 0));
        assert!(class.contains("tw:border-selection-border"));
        assert!(class.contains("--studio-tree-dirty-bg:var(--studio-status-warning-bg)"));
        assert!(class.contains(
            "color-mix(in_oklab,var(--studio-tree-dirty-bg,var(--studio-color-selection-bg))_45%,var(--studio-color-selection-bg))"
        ));
        assert!(!class.contains("linear-gradient"));
    }

    #[test]
    fn focused_clean_row_falls_back_to_the_plain_selection_highlight() {
        let class = tree_item_row_class(true, DirtySummary::clean());
        assert!(class.contains("tw:border-selection-border"));
        // No variable set: the color-mix falls back to the neutral selection color.
        assert!(!class.contains("--studio-tree-dirty-bg:var"));
        assert!(class.contains("var(--studio-tree-dirty-bg,var(--studio-color-selection-bg))"));
    }

    #[test]
    fn row_title_carries_the_status_word_and_the_dirty_breakdown() {
        use lpa_studio_core::{ProjectNodeStatusTone, ProjectNodeStatusView};

        let running = ProjectNodeStatusView::new("Running", None, ProjectNodeStatusTone::Good);
        let warning = ProjectNodeStatusView::new(
            "Warning",
            Some("using fallback palette".to_string()),
            ProjectNodeStatusTone::Warning,
        );

        assert_eq!(
            tree_item_title("Shader", &running, DirtySummary::clean()),
            "Shader — Running"
        );
        assert_eq!(
            tree_item_title("Visual", &warning, DirtySummary::clean()),
            "Visual — Warning: using fallback palette"
        );
        assert_eq!(
            tree_item_title("Shader", &running, dirty(2, 1, 0)),
            "Shader — Running — edits in this subtree: 2 unsaved, 1 live"
        );
        assert_eq!(
            tree_item_title("Output", &running, dirty(0, 0, 3)),
            "Output — Running — edits in this subtree: 3 failed"
        );
    }

    #[test]
    fn rows_show_the_indicator_only_when_the_affordance_is_announced() {
        use lpa_studio_core::{
            ProjectNodeStatusTone, ProjectNodeStatusView, ProjectNodeTreeItem, UiAction,
            UiAffordance,
        };

        let item = |tone: ProjectNodeStatusTone, dirty: DirtySummary| {
            ProjectNodeTreeItem::new(
                "n1",
                "Clock",
                "Clock",
                ProjectNodeStatusView::new("Running", None, tone),
                false,
                UiAction::from_op(
                    lpa_studio_core::ControllerId::new("story.project"),
                    lpa_studio_core::ProjectEditorOp::Focus,
                ),
                Vec::new(),
            )
            .with_dirty(dirty)
        };

        // Clean + healthy: silent (no indicator element at all).
        let clean = item(ProjectNodeStatusTone::Good, DirtySummary::clean());
        assert_eq!(clean.affordance(), UiAffordance::Info);
        assert!(affordance_indicator_class(clean.affordance()).is_none());

        // Dirty and failing rows announce with the affordance glyph.
        let unsaved = item(ProjectNodeStatusTone::Good, dirty(1, 0, 0));
        assert_eq!(unsaved.affordance(), UiAffordance::Unsaved);
        let warn = item(ProjectNodeStatusTone::Warning, DirtySummary::clean());
        assert_eq!(warn.affordance(), UiAffordance::Error);
        assert!(affordance_indicator_class(warn.affordance()).is_some());
    }
}
