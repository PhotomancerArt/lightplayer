use dioxus::prelude::*;
use lpa_studio_core::{ProjectEditorView, ProjectSyncPhase, UiAction, UiChannelChoice};

use crate::app::node::NodePane;

/// The node-body column of the project editor: one `NodePane` per synced
/// node. The sidebar column is the [`ProjectPane`](super::ProjectPane) —
/// one `StudioPane` carrying the project header and the node tree.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectNodeWorkspace(view: ProjectEditorView, on_action: EventHandler<UiAction>) -> Element {
    // Channel choices context: every bindable row's binding picker reads
    // this shared list (observed ∪ well-known, M4).
    let mut channel_choices = use_context_provider(|| Signal::new(Vec::<UiChannelChoice>::new()));
    if *channel_choices.peek() != view.channel_choices {
        channel_choices.set(view.channel_choices.clone());
    }
    // An empty node list means "still syncing" only before the first sync
    // completes; afterwards it is a real (and normal) empty project.
    let syncing = !matches!(view.sync.phase, ProjectSyncPhase::Ready);
    let nodes = view.nodes;
    let pending_edits = view.pending_edits;

    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3.5",
            if nodes.is_empty() && syncing {
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:rounded-md tw:border tw:border-border-subtle tw:bg-card-subtle tw:p-4",
                    h3 { class: "tw:m-0 tw:text-base tw:text-strong-foreground", "Syncing project…" }
                    p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground", "Node cards appear here once the project has synced." }
                }
            } else if nodes.is_empty() {
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:rounded-md tw:border tw:border-dashed tw:border-border-subtle tw:bg-card-subtle tw:p-4",
                    h3 { class: "tw:m-0 tw:text-base tw:text-strong-foreground", "This project is empty" }
                    p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground",
                        "Add your first node from the project panel — the “Add node…” row or the + in its header."
                    }
                }
            } else {
                for node in nodes {
                    NodePane {
                        key: "{node.node_id}",
                        view: node,
                        on_action,
                        pending_edits: pending_edits.clone(),
                    }
                }
            }
        }
    }
}
