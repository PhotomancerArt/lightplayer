use dioxus::prelude::*;
use lpa_studio_core::{ProjectEditorView, UiAction};

use crate::app::node::NodePane;

/// The node-body column of the project editor: one `NodePane` per synced
/// node. The sidebar column is the [`ProjectPane`](super::ProjectPane) —
/// one `StudioPane` carrying the project header and the node tree.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectNodeWorkspace(view: ProjectEditorView, on_action: EventHandler<UiAction>) -> Element {
    let nodes = view.nodes;

    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3.5",
            if nodes.is_empty() {
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:rounded-md tw:border tw:border-border-subtle tw:bg-card-subtle tw:p-4",
                    h3 { class: "tw:m-0 tw:text-base tw:text-strong-foreground", "Waiting for project data" }
                    p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground", "Studio will show node bodies here once the project mirror has synced." }
                }
            } else {
                for node in nodes {
                    NodePane {
                        key: "{node.node_id}",
                        view: node,
                        on_action,
                    }
                }
            }
        }
    }
}
