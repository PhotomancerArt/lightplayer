use dioxus::prelude::*;
use lpa_studio_core::{ProjectEditorView, ProjectSyncPhase, UiAction, UiChannelChoice};

use crate::app::node::{NodePane, PaletteCatalog, WorkspaceAddNodeButton, project_palette_choices};

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
    // Palette catalog context (M4 P4): the chooser's "This project" section
    // is DERIVED from the same node data these cards render, so a palette
    // authored on one node is one click away on every other. The shipped
    // catalog rides behind it unchanged (`catalog: None`) — copying a
    // static list into a signal on every view refresh would buy nothing.
    let mut palette_catalog = use_context_provider(|| Signal::new(PaletteCatalog::default()));
    let project_palettes = project_palette_choices(&view.nodes);
    if palette_catalog.peek().project != project_palettes {
        palette_catalog.set(PaletteCatalog {
            project: project_palettes,
            catalog: None,
        });
    }
    // An empty node list means "still syncing" only before the first sync
    // completes; afterwards it is a real (and normal) empty project.
    let syncing = !matches!(view.sync.phase, ProjectSyncPhase::Ready);
    let add_node_menu = view.add_node_menu.clone();
    let nodes = view.nodes;
    let empty = nodes.is_empty();
    let pending_edits = view.pending_edits;

    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3.5",
            if empty && syncing {
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:rounded-md tw:border tw:border-border-subtle tw:bg-card-subtle tw:p-4",
                    h3 { class: "tw:m-0 tw:text-base tw:text-strong-foreground", "Syncing project…" }
                    p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground", "Node cards appear here once the project has synced." }
                }
            } else if empty {
                div { class: "tw:grid tw:min-w-0 tw:justify-items-start tw:gap-3 tw:rounded-md tw:border tw:border-dashed tw:border-border-subtle tw:bg-card-subtle tw:p-4",
                    h3 { class: "tw:m-0 tw:text-base tw:text-strong-foreground", "This project is empty" }
                    p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground",
                        "Nodes are a project's building blocks — clocks, shaders, fixtures, outputs. Add your first:"
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
            // The workspace add affordance (review round: buttons live where
            // people look for them) — the empty card's call to action, and a
            // quiet trailing button under the node cards otherwise.
            if !syncing && let Some(menu) = add_node_menu {
                WorkspaceAddNodeButton { menu, on_action }
            }
        }
    }
}
