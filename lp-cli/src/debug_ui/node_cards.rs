//! Main node-card workspace for the temporary debug UI.

use eframe::egui;
use lpc_model::{NodeId, ResourceRef};
use lpc_view::project::ProjectView;

use super::format::format_resource_metadata;
use super::inspector::InspectorSelection;
use super::resource_preview::render_resource_payload_preview;
use super::slot_render::{
    render_resource_skeleton, render_slot_root_rows, render_top_field_row, root_name,
    top_record_field,
};

pub(crate) fn render_node_workspace(
    ui: &mut egui::Ui,
    view: &ProjectView,
    selection: &mut Option<InspectorSelection>,
) {
    if view.tree.nodes.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("Waiting for project sync...");
        });
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("node-workspace")
        .show(ui, |ui| {
            ui.heading("Nodes");
            ui.add_space(6.0);
            for id in node_order(view) {
                render_node_card(ui, view, id, selection);
                ui.add_space(8.0);
            }
        });
}

fn render_node_card(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    selection: &mut Option<InspectorSelection>,
) {
    let Some(entry) = view.tree.nodes.get(&id) else {
        return;
    };

    let selected = *selection == Some(InspectorSelection::Node(id));
    ui.push_id(("node-card", id.0), |ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let label = entry.path.0.last().map_or_else(
                    || entry.path.to_string(),
                    |segment| segment.name.to_string(),
                );
                if ui.selectable_label(selected, label).clicked() {
                    *selection = Some(InspectorSelection::Node(id));
                }
                ui.monospace(format!("#{}", id.0));
                ui.separator();
                ui.label(format!("{:?}", entry.status));
                ui.label(format!("{:?}", entry.state));
                ui.separator();
                ui.label(format!("changed {}", entry.change_frame.0));
            });
            ui.small(entry.path.to_string());
            ui.separator();

            render_connections(ui, view, id, selection);
            render_owned_resources(ui, view, id, selection);

            egui::CollapsingHeader::new("def / config")
                .id_salt(("node-card-def", id.0))
                .default_open(false)
                .show(ui, |ui| render_root_rows(ui, view, id, "def", None));

            if has_root(view, id, "state") {
                egui::CollapsingHeader::new("state")
                    .id_salt(("node-card-state", id.0))
                    .default_open(false)
                    .show(ui, |ui| render_root_rows(ui, view, id, "state", None));
            }

            if !entry.children.is_empty() {
                egui::CollapsingHeader::new(format!("children ({})", entry.children.len()))
                    .id_salt(("node-card-children", id.0))
                    .default_open(false)
                    .show(ui, |ui| {
                        for child in &entry.children {
                            if let Some(child_entry) = view.tree.nodes.get(child) {
                                ui.horizontal_wrapped(|ui| {
                                    ui.monospace(format!("#{}", child.0));
                                    ui.label(child_entry.path.to_string());
                                });
                            }
                        }
                    });
            }
        });
    });
}

fn render_connections(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    selection: &mut Option<InspectorSelection>,
) {
    let mut rendered = false;

    ui.strong("slots");
    if render_named_top_field(ui, view, id, "state", "output", "output", Some(selection)) {
        rendered = true;
    }
    if render_named_top_field(ui, view, id, "def", "input", "input", Some(selection)) {
        rendered = true;
    }
    if render_named_top_field(ui, view, id, "def", "output", "output", Some(selection)) {
        rendered = true;
    }

    if render_bindings(ui, view, id, selection) {
        rendered = true;
    }

    if !rendered {
        ui.label("No prominent input/output slots yet.");
    }
}

fn render_bindings(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    selection: &mut Option<InspectorSelection>,
) -> bool {
    let Some((shape, data)) = root_shape_and_data(view, id, "def") else {
        return false;
    };
    if top_record_field(&view.slots.registry, shape, data, "bindings").is_none() {
        return false;
    }

    ui.horizontal_wrapped(|ui| {
        ui.strong("bindings");
        ui.label("authored");
    });
    render_top_field_row(
        ui,
        &view.slots.registry,
        shape,
        data,
        "bindings",
        "bindings",
        Some(selection),
    );
    true
}

fn render_owned_resources(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    selection: &mut Option<InspectorSelection>,
) {
    let resources = owned_resources(view, id);
    if resources.is_empty() {
        return;
    }

    ui.add_space(4.0);
    ui.strong("resources");
    for resource in resources {
        let Some(summary) = view.resource_cache.summary(resource) else {
            continue;
        };
        ui.push_id(
            ("node-resource", id.0, resource.domain, resource.id),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    render_resource_skeleton(ui, resource, Some(selection));
                    ui.label(format_resource_metadata(&summary.metadata));
                    if let Some(bytes) = summary.byte_length_hint {
                        ui.small(format!("{bytes} bytes"));
                    }
                });
                if *selection == Some(InspectorSelection::Resource(resource)) {
                    ui.indent(
                        ("node-resource-preview", resource.domain, resource.id),
                        |ui| {
                            render_resource_payload_preview(ui, view, resource);
                        },
                    );
                }
            },
        );
    }
}

fn render_named_top_field(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    suffix: &str,
    field: &str,
    label: &str,
    selection: Option<&mut Option<InspectorSelection>>,
) -> bool {
    let Some((shape, data)) = root_shape_and_data(view, id, suffix) else {
        return false;
    };
    render_top_field_row(
        ui,
        &view.slots.registry,
        shape,
        data,
        field,
        label,
        selection,
    )
}

fn render_root_rows(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    suffix: &str,
    selection: Option<&mut Option<InspectorSelection>>,
) {
    let Some((shape, data)) = root_shape_and_data(view, id, suffix) else {
        ui.label(format!("No {suffix} slot root."));
        return;
    };
    render_slot_root_rows(ui, &view.slots.registry, shape, data, selection);
}

fn has_root(view: &ProjectView, id: NodeId, suffix: &str) -> bool {
    let root = root_name(id, suffix);
    view.slots.roots.contains_key(&root) && view.slots.root_shapes.contains_key(&root)
}

fn root_shape_and_data<'a>(
    view: &'a ProjectView,
    id: NodeId,
    suffix: &str,
) -> Option<(lpc_model::SlotShapeId, &'a lpc_model::SlotData)> {
    let root = root_name(id, suffix);
    Some((
        *view.slots.root_shapes.get(&root)?,
        view.slots.roots.get(&root)?,
    ))
}

fn owned_resources(view: &ProjectView, id: NodeId) -> Vec<ResourceRef> {
    view.resource_cache
        .summaries()
        .filter_map(|summary| (summary.owner == Some(id)).then_some(summary.resource_ref))
        .collect()
}

fn node_order(view: &ProjectView) -> Vec<NodeId> {
    let mut order = Vec::new();
    let roots: Vec<NodeId> = view
        .tree
        .nodes
        .iter()
        .filter_map(|(id, entry)| entry.parent.is_none().then_some(*id))
        .collect();
    for root in roots {
        collect_node_order(view, root, &mut order);
    }
    order
}

fn collect_node_order(view: &ProjectView, id: NodeId, order: &mut Vec<NodeId>) {
    if !view.tree.nodes.contains_key(&id) {
        return;
    }
    order.push(id);
    if let Some(entry) = view.tree.nodes.get(&id) {
        for child in &entry.children {
            collect_node_order(view, *child, order);
        }
    }
}
