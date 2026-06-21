use dioxus::prelude::*;
use lpa_studio_core::StudioState;

#[component]
pub fn InventoryView(state: StudioState) -> Element {
    let inventory = state
        .project_session
        .as_ref()
        .and_then(|project| project.inventory.as_ref());
    let node_count = inventory
        .map(|inventory| inventory.nodes.len())
        .unwrap_or(0);
    let def_count = inventory.map(|inventory| inventory.defs.len()).unwrap_or(0);
    let asset_count = inventory
        .map(|inventory| inventory.assets.len())
        .unwrap_or(0);
    rsx! {
        section { class: "panel inventory-panel",
            div { class: "panel-heading",
                h2 { "Inventory" }
                span { class: "mini-count", "{node_count} nodes" }
            }
            div { class: "inventory-stats",
                div { strong { "{node_count}" } span { "Nodes" } }
                div { strong { "{def_count}" } span { "Definitions" } }
                div { strong { "{asset_count}" } span { "Assets" } }
            }
            ul { class: "inventory-list",
                if let Some(inventory) = inventory {
                    for node in inventory.nodes.iter().take(8) {
                        {
                            let label = if node.key.is_root() {
                                "root".to_string()
                            } else {
                                node.key
                                    .segments
                                    .iter()
                                    .map(|segment| segment.slot.to_string())
                                    .collect::<Vec<_>>()
                                    .join(" / ")
                            };
                            rsx! { li { "{label}" } }
                        }
                    }
                } else {
                    li { "No project inventory yet." }
                }
            }
        }
    }
}
