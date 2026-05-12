//! Right-side debug inspector for nodes, resources, and shapes.

use eframe::egui;
use lpc_model::{NodeId, ResourceRef, SlotShape, SlotShapeId};
use lpc_view::project::ProjectView;

use super::format::{format_resource_metadata, format_resource_summary};
use super::slot_render::{render_slot_root_debug, render_slot_shape_summary, root_name};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InspectorSelection {
    Node(NodeId),
    Resource(ResourceRef),
    Shape(SlotShapeId),
}

pub(crate) fn render_debug_inspector(
    ui: &mut egui::Ui,
    view: &ProjectView,
    selection: &mut Option<InspectorSelection>,
) {
    ensure_selection(view, selection);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Debug");
        ui.separator();
        render_node_tree(ui, view, selection);
        ui.separator();
        render_resource_tree(ui, view, selection);
        ui.separator();
        render_shape_tree(ui, view, selection);
        ui.separator();
        render_selected_detail(ui, view, *selection);
    });
}

fn ensure_selection(view: &ProjectView, selection: &mut Option<InspectorSelection>) {
    let valid = match selection {
        Some(InspectorSelection::Node(id)) => view.tree.nodes.contains_key(id),
        Some(InspectorSelection::Resource(resource_ref)) => {
            view.resource_cache.summary(*resource_ref).is_some()
        }
        Some(InspectorSelection::Shape(id)) => view.slots.registry.contains(id),
        None => false,
    };
    if valid {
        return;
    }
    *selection = view
        .tree
        .nodes
        .keys()
        .next()
        .copied()
        .map(InspectorSelection::Node);
}

fn render_node_tree(
    ui: &mut egui::Ui,
    view: &ProjectView,
    selection: &mut Option<InspectorSelection>,
) {
    ui.collapsing("Nodes", |ui| {
        if view.tree.nodes.is_empty() {
            ui.label("Waiting for project sync...");
            return;
        }

        let roots: Vec<NodeId> = view
            .tree
            .nodes
            .iter()
            .filter_map(|(id, entry)| entry.parent.is_none().then_some(*id))
            .collect();

        for root in roots {
            render_node_tree_entry(ui, view, root, 0, selection);
        }
    });
}

fn render_node_tree_entry(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    depth: usize,
    selection: &mut Option<InspectorSelection>,
) {
    let Some(entry) = view.tree.nodes.get(&id) else {
        return;
    };

    ui.horizontal(|ui| {
        ui.add_space((depth as f32) * 14.0);
        let label = entry.path.0.last().map_or_else(
            || entry.path.to_string(),
            |segment| segment.name.to_string(),
        );
        let selected = *selection == Some(InspectorSelection::Node(id));
        if ui
            .selectable_label(selected, format!("{label}  #{}", id.0))
            .clicked()
        {
            *selection = Some(InspectorSelection::Node(id));
        }
    });

    for child in &entry.children {
        render_node_tree_entry(ui, view, *child, depth + 1, selection);
    }
}

fn render_resource_tree(
    ui: &mut egui::Ui,
    view: &ProjectView,
    selection: &mut Option<InspectorSelection>,
) {
    ui.collapsing("Resources", |ui| {
        let mut count = 0usize;
        for summary in view.resource_cache.summaries() {
            count += 1;
            let selected = *selection == Some(InspectorSelection::Resource(summary.resource_ref));
            if ui
                .selectable_label(selected, format_resource_summary(summary))
                .clicked()
            {
                *selection = Some(InspectorSelection::Resource(summary.resource_ref));
            }
        }
        if count == 0 {
            ui.label("No resources synced.");
        }
    });
}

fn render_shape_tree(
    ui: &mut egui::Ui,
    view: &ProjectView,
    selection: &mut Option<InspectorSelection>,
) {
    ui.collapsing("Shapes", |ui| {
        let snapshot = view.slots.registry.snapshot();
        if snapshot.shapes.is_empty() {
            ui.label("No shapes synced.");
            return;
        }

        for (id, entry) in snapshot.shapes {
            let selected = *selection == Some(InspectorSelection::Shape(id));
            if ui
                .selectable_label(selected, format!("{id}  rev {}", entry.changed_at().0))
                .clicked()
            {
                *selection = Some(InspectorSelection::Shape(id));
            }
        }
    });
}

fn render_selected_detail(
    ui: &mut egui::Ui,
    view: &ProjectView,
    selection: Option<InspectorSelection>,
) {
    ui.heading("Details");
    match selection {
        Some(InspectorSelection::Node(id)) => render_node_detail(ui, view, id),
        Some(InspectorSelection::Resource(resource_ref)) => {
            render_resource_detail(ui, view, resource_ref);
        }
        Some(InspectorSelection::Shape(id)) => render_shape_detail(ui, view, id),
        None => {
            ui.label("No selection.");
        }
    }
}

fn render_node_detail(ui: &mut egui::Ui, view: &ProjectView, id: NodeId) {
    let Some(entry) = view.tree.nodes.get(&id) else {
        ui.label("Node missing.");
        return;
    };

    ui.horizontal_wrapped(|ui| {
        ui.monospace(format!("#{}", id.0));
        ui.label(format!("{:?}", entry.status));
        ui.label(format!("{:?}", entry.state));
    });
    ui.small(entry.path.to_string());
    ui.label(format!("created {}", entry.created_frame.0));
    ui.label(format!("changed {}", entry.change_frame.0));
    ui.label(format!("children changed {}", entry.children_ver.0));

    for suffix in ["def", "state"] {
        let root = root_name(id, suffix);
        let Some(data) = view.slots.roots.get(&root) else {
            continue;
        };
        let Some(shape) = view.slots.root_shapes.get(&root).copied() else {
            continue;
        };

        egui::CollapsingHeader::new(suffix)
            .default_open(suffix == "def")
            .show(ui, |ui| {
                render_slot_root_debug(ui, &view.slots.registry, shape, data);
            });
    }
}

fn render_resource_detail(ui: &mut egui::Ui, view: &ProjectView, resource_ref: ResourceRef) {
    let Some(summary) = view.resource_cache.summary(resource_ref) else {
        ui.label("Resource missing.");
        return;
    };

    ui.monospace(format!("{:?}/{}", resource_ref.domain, resource_ref.id));
    ui.label(format!("revision {}", summary.revision.0));
    ui.label(format!("availability {:?}", summary.availability));
    ui.label(format_resource_metadata(&summary.metadata));
    if let Some(bytes) = summary.byte_length_hint {
        ui.label(format!("{bytes} bytes"));
    }
    ui.add_enabled(false, egui::Button::new("payload detail"));
}

fn render_shape_detail(ui: &mut egui::Ui, view: &ProjectView, id: SlotShapeId) {
    let Some(entry) = view.slots.registry.entry(&id) else {
        ui.label("Shape missing.");
        return;
    };

    ui.monospace(format!("{id}"));
    ui.label(format!("revision {}", entry.changed_at().0));
    render_shape_tree_detail(ui, &view.slots.registry, entry.value(), 0);
}

fn render_shape_tree_detail(
    ui: &mut egui::Ui,
    registry: &lpc_model::SlotShapeRegistry,
    shape: &SlotShape,
    depth: usize,
) {
    ui.horizontal_wrapped(|ui| {
        ui.add_space((depth as f32) * 14.0);
        render_slot_shape_summary(ui, registry, shape);
    });

    match shape {
        SlotShape::Ref { id } => {
            if let Some(shape) = registry.get(id) {
                render_shape_tree_detail(ui, registry, shape, depth + 1);
            }
        }
        SlotShape::Record { fields, .. } => {
            for field in fields {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(((depth + 1) as f32) * 14.0);
                    ui.monospace(field.name.as_str());
                });
                render_shape_tree_detail(ui, registry, &field.shape, depth + 2);
            }
        }
        SlotShape::Map { value, .. } | SlotShape::Option { some: value, .. } => {
            render_shape_tree_detail(ui, registry, value, depth + 1);
        }
        SlotShape::Enum { variants, .. } => {
            for variant in variants {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(((depth + 1) as f32) * 14.0);
                    ui.monospace(variant.name.as_str());
                });
                render_shape_tree_detail(ui, registry, &variant.shape, depth + 2);
            }
        }
        SlotShape::Unit { .. } | SlotShape::Value { .. } => {}
    }
}
