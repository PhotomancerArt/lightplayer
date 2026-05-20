//! Main node-card workspace for the temporary debug UI.

use eframe::egui;
use lpc_model::{NodeId, ResourceRef, SlotData};
use lpc_view::project::ProjectView;
use lpc_wire::{WireEntryState, WireNodeStatus};

use super::format::format_resource_metadata;
use super::inspector::InspectorSelection;
use super::resource_preview::render_resource_payload_preview;
use super::slot_edit::{SlotEditIntent, SlotEditStatusContext};
use super::slot_render::{
    render_resource_skeleton, render_slot_root_rows, render_slot_root_rows_filtered,
    render_top_field_row, root_name, top_record_field,
};

pub(crate) fn render_node_workspace(
    ui: &mut egui::Ui,
    view: &ProjectView,
    selection: &mut Option<InspectorSelection>,
    status: Option<&SlotEditStatusContext<'_>>,
    mut edit_intents: Option<&mut Vec<SlotEditIntent>>,
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
                render_node_card(ui, view, id, selection, status, edit_intents.as_deref_mut());
                ui.add_space(8.0);
            }
        });
}

fn render_node_card(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    selection: &mut Option<InspectorSelection>,
    status: Option<&SlotEditStatusContext<'_>>,
    mut edit_intents: Option<&mut Vec<SlotEditIntent>>,
) {
    let Some(entry) = view.tree.nodes.get(&id) else {
        return;
    };

    let selected = *selection == Some(InspectorSelection::Node(id));
    ui.push_id(("node-card", id.0), |ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            render_node_header(ui, view, id, selected, selection);
            ui.separator();

            render_connections(ui, view, id, selection, status, edit_intents.as_deref_mut());
            render_owned_resources(ui, view, id, selection);

            egui::CollapsingHeader::new("config")
                .id_salt(("node-card-config", id.0))
                .default_open(true)
                .show(ui, |ui| {
                    render_root_rows_filtered(
                        ui,
                        view,
                        id,
                        "def",
                        &["bindings"],
                        None,
                        status,
                        edit_intents.as_deref_mut(),
                    );
                });

            if has_root(view, id, "state") {
                egui::CollapsingHeader::new("state")
                    .id_salt(("node-card-state", id.0))
                    .default_open(false)
                    .show(ui, |ui| {
                        render_root_rows(
                            ui,
                            view,
                            id,
                            "state",
                            None,
                            status,
                            edit_intents.as_deref_mut(),
                        );
                    });
            }

            render_bindings(ui, view, id, selection, status, edit_intents.as_deref_mut());

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

fn render_node_header(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    selected: bool,
    selection: &mut Option<InspectorSelection>,
) {
    let Some(entry) = view.tree.nodes.get(&id) else {
        return;
    };

    let name = node_label(entry);
    let kind = node_kind_label(view, id);
    let accent = node_status_color(&entry.status, &entry.state);
    ui.horizontal(|ui| {
        render_node_type_badge(ui, kind, accent);
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                let response = ui.selectable_label(selected, egui::RichText::new(name).strong());
                if response.clicked() {
                    *selection = Some(InspectorSelection::Node(id));
                }
                ui.monospace(format!("#{}", id.0));
                render_status_chip(ui, status_label(&entry.status), accent);
                render_status_chip(ui, state_label(&entry.state), state_color(&entry.state));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(kind)
                        .color(ui.visuals().weak_text_color())
                        .small(),
                );
                ui.label(
                    egui::RichText::new(entry.path.to_string())
                        .color(ui.visuals().weak_text_color())
                        .small(),
                );
                ui.label(
                    egui::RichText::new(format!("changed {}", entry.change_frame.0))
                        .color(ui.visuals().weak_text_color())
                        .small(),
                );
            });
        });
    });
}

fn render_node_type_badge(ui: &mut egui::Ui, kind: &'static str, accent: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.circle_filled(rect.center(), 17.0, accent.gamma_multiply(0.18));
    painter.circle_stroke(rect.center(), 17.0, egui::Stroke::new(1.5_f32, accent));
    let icon_color = accent;
    match kind {
        "Clock" => {
            painter.circle_stroke(rect.center(), 8.0, egui::Stroke::new(1.6_f32, icon_color));
            painter.line_segment(
                [rect.center(), rect.center() + egui::vec2(0.0, -5.0)],
                egui::Stroke::new(1.6_f32, icon_color),
            );
            painter.line_segment(
                [rect.center(), rect.center() + egui::vec2(5.0, 3.0)],
                egui::Stroke::new(1.6_f32, icon_color),
            );
        }
        "Fluid" => {
            for offset in [-5.0, 0.0, 5.0] {
                let y = rect.center().y + offset;
                painter.line_segment(
                    [
                        egui::pos2(rect.left() + 9.0, y),
                        egui::pos2(rect.left() + 29.0, y - 3.0),
                    ],
                    egui::Stroke::new(1.5_f32, icon_color),
                );
            }
        }
        "Output" => {
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 10.0, rect.center().y),
                    egui::pos2(rect.left() + 26.0, rect.center().y),
                ],
                egui::Stroke::new(2.0_f32, icon_color),
            );
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 21.0, rect.center().y - 6.0),
                    egui::pos2(rect.left() + 27.0, rect.center().y),
                ],
                egui::Stroke::new(2.0_f32, icon_color),
            );
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 21.0, rect.center().y + 6.0),
                    egui::pos2(rect.left() + 27.0, rect.center().y),
                ],
                egui::Stroke::new(2.0_f32, icon_color),
            );
        }
        "Fixture" => {
            for offset in [-7.0, 0.0, 7.0] {
                painter.circle_filled(
                    egui::pos2(rect.center().x + offset, rect.center().y),
                    3.0,
                    icon_color,
                );
            }
        }
        "Texture" => {
            let tile = 7.0;
            for x in 0..2 {
                for y in 0..2 {
                    let fill = if (x + y) % 2 == 0 {
                        icon_color
                    } else {
                        icon_color.gamma_multiply(0.32)
                    };
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(
                                rect.center().x - tile + x as f32 * tile,
                                rect.center().y - tile + y as f32 * tile,
                            ),
                            egui::vec2(tile, tile),
                        ),
                        1.0,
                        fill,
                    );
                }
            }
        }
        "Shader" | "ComputeShader" => {
            let points = [
                egui::pos2(rect.center().x, rect.top() + 10.0),
                egui::pos2(rect.right() - 10.0, rect.center().y),
                egui::pos2(rect.center().x, rect.bottom() - 10.0),
                egui::pos2(rect.left() + 10.0, rect.center().y),
            ];
            painter.add(egui::Shape::convex_polygon(
                points.to_vec(),
                icon_color.gamma_multiply(0.25),
                egui::Stroke::new(1.6_f32, icon_color),
            ));
        }
        _ => {
            painter.rect_stroke(
                rect.shrink(10.0),
                3.0,
                egui::Stroke::new(1.6_f32, icon_color),
            );
        }
    }

    let bubble = egui::Rect::from_center_size(
        rect.right_bottom() - egui::vec2(7.0, 7.0),
        egui::vec2(9.0, 9.0),
    );
    painter.circle_filled(bubble.center(), 4.5, accent);
}

fn render_status_chip(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    egui::Frame::default()
        .fill(color.gamma_multiply(0.16))
        .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.65)))
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .rounding(4.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).small().color(color));
        });
}

fn render_connections(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    selection: &mut Option<InspectorSelection>,
    status: Option<&SlotEditStatusContext<'_>>,
    mut edit_intents: Option<&mut Vec<SlotEditIntent>>,
) {
    let mut rendered = false;

    ui.strong("slots");
    if render_named_top_field(
        ui,
        view,
        id,
        "state",
        "output",
        "output",
        Some(selection),
        status,
        edit_intents.as_deref_mut(),
    ) {
        rendered = true;
    }
    if render_named_top_field(
        ui,
        view,
        id,
        "def",
        "input",
        "input",
        Some(selection),
        status,
        edit_intents.as_deref_mut(),
    ) {
        rendered = true;
    }
    if render_named_top_field(
        ui,
        view,
        id,
        "def",
        "output",
        "output",
        Some(selection),
        status,
        edit_intents.as_deref_mut(),
    ) {
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
    status: Option<&SlotEditStatusContext<'_>>,
    edit_intents: Option<&mut Vec<SlotEditIntent>>,
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
        Some(&root_name(id, "def")),
        status,
        edit_intents,
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
    status: Option<&SlotEditStatusContext<'_>>,
    edit_intents: Option<&mut Vec<SlotEditIntent>>,
) -> bool {
    let Some((shape, data)) = root_shape_and_data(view, id, suffix) else {
        return false;
    };
    let root = root_name(id, suffix);
    render_top_field_row(
        ui,
        &view.slots.registry,
        shape,
        data,
        field,
        label,
        selection,
        Some(&root),
        status,
        edit_intents,
    )
}

fn render_root_rows(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    suffix: &str,
    selection: Option<&mut Option<InspectorSelection>>,
    status: Option<&SlotEditStatusContext<'_>>,
    edit_intents: Option<&mut Vec<SlotEditIntent>>,
) {
    let Some((shape, data)) = root_shape_and_data(view, id, suffix) else {
        ui.label(format!("No {suffix} slot root."));
        return;
    };
    let root = root_name(id, suffix);
    render_slot_root_rows(
        ui,
        &view.slots.registry,
        &root,
        shape,
        data,
        selection,
        status,
        edit_intents,
    );
}

fn render_root_rows_filtered(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    suffix: &str,
    skip_top_fields: &[&str],
    selection: Option<&mut Option<InspectorSelection>>,
    status: Option<&SlotEditStatusContext<'_>>,
    edit_intents: Option<&mut Vec<SlotEditIntent>>,
) {
    let Some((shape, data)) = root_shape_and_data(view, id, suffix) else {
        ui.label(format!("No {suffix} slot root."));
        return;
    };
    let root = root_name(id, suffix);
    render_slot_root_rows_filtered(
        ui,
        &view.slots.registry,
        &root,
        shape,
        data,
        skip_top_fields,
        selection,
        status,
        edit_intents,
    );
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

fn node_label(entry: &lpc_view::tree::TreeEntryView) -> String {
    entry.path.0.last().map_or_else(
        || entry.path.to_string(),
        |segment| segment.name.to_string(),
    )
}

fn node_kind_label(view: &ProjectView, id: NodeId) -> &'static str {
    let Some((_, data)) = root_shape_and_data(view, id, "def") else {
        return "Node";
    };
    match data {
        SlotData::Enum(value) => match value.variant.as_str() {
            "Project" => "Project",
            "Clock" => "Clock",
            "Texture" => "Texture",
            "Shader" => "Shader",
            "ComputeShader" => "ComputeShader",
            "Fluid" => "Fluid",
            "Output" => "Output",
            "Fixture" => "Fixture",
            _ => "Node",
        },
        _ => "Node",
    }
}

fn status_label(status: &WireNodeStatus) -> &str {
    match status {
        WireNodeStatus::Created => "created",
        WireNodeStatus::InitError(_) => "init error",
        WireNodeStatus::Ok => "ok",
        WireNodeStatus::Warn(_) => "warn",
        WireNodeStatus::Error(_) => "error",
    }
}

fn state_label(state: &WireEntryState) -> &str {
    match state {
        WireEntryState::Pending => "pending",
        WireEntryState::Alive => "alive",
        WireEntryState::Failed { .. } => "failed",
    }
}

fn node_status_color(status: &WireNodeStatus, state: &WireEntryState) -> egui::Color32 {
    match (status, state) {
        (WireNodeStatus::Error(_) | WireNodeStatus::InitError(_), _)
        | (_, WireEntryState::Failed { .. }) => egui::Color32::from_rgb(220, 75, 72),
        (WireNodeStatus::Warn(_), _) => egui::Color32::from_rgb(214, 159, 43),
        (WireNodeStatus::Ok, WireEntryState::Alive) => egui::Color32::from_rgb(76, 174, 114),
        _ => egui::Color32::from_rgb(112, 144, 191),
    }
}

fn state_color(state: &WireEntryState) -> egui::Color32 {
    match state {
        WireEntryState::Pending => egui::Color32::from_rgb(112, 144, 191),
        WireEntryState::Alive => egui::Color32::from_rgb(76, 174, 114),
        WireEntryState::Failed { .. } => egui::Color32::from_rgb(220, 75, 72),
    }
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
