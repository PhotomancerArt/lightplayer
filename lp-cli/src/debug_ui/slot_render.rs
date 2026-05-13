//! Shared slot rendering for node cards and debug inspector.

use eframe::egui;
use lpc_model::{
    LpValue, ProductRef, SlotData, SlotMapKey, SlotShape, SlotShapeId, SlotShapeRegistry,
    SlotValueShape,
};

use super::format::{
    format_lp_type, format_lp_value, format_product_ref, format_resource_ref,
    format_value_editor_hint,
};
use super::inspector::InspectorSelection;

pub(crate) fn render_slot_root_debug(
    ui: &mut egui::Ui,
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    data: &SlotData,
) {
    match registry.get(&shape_id) {
        Some(shape) => render_slot_shape_debug(ui, registry, shape, data, "root"),
        None => {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Missing shape {shape_id}"),
            );
            render_slot_data_fallback(ui, data);
        }
    }
}

pub(crate) fn render_slot_root_rows(
    ui: &mut egui::Ui,
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    data: &SlotData,
    selection: Option<&mut Option<InspectorSelection>>,
) {
    match registry.get(&shape_id) {
        Some(shape) => render_slot_shape_rows(ui, registry, shape, data, 0, "root", selection),
        None => {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Missing shape {shape_id}"),
            );
            render_slot_data_fallback(ui, data);
        }
    }
}

pub(crate) fn render_top_field_row(
    ui: &mut egui::Ui,
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    data: &SlotData,
    field_name: &str,
    label: &str,
    selection: Option<&mut Option<InspectorSelection>>,
) -> bool {
    let Some((shape, data)) = top_record_field(registry, shape_id, data, field_name) else {
        return false;
    };
    render_named_slot_shape_row(ui, registry, label, shape, data, 0, label, selection);
    true
}

pub(crate) fn top_record_field<'a>(
    registry: &'a SlotShapeRegistry,
    shape_id: SlotShapeId,
    data: &'a SlotData,
    field_name: &str,
) -> Option<(&'a SlotShape, &'a SlotData)> {
    let shape = resolve_shape(registry, registry.get(&shape_id)?)?;
    let SlotShape::Record { fields, .. } = shape else {
        return None;
    };
    let SlotData::Record(record) = data else {
        return None;
    };
    let (index, field) = fields
        .iter()
        .enumerate()
        .find(|(_, field)| field.name.as_str() == field_name)?;
    let child = record.fields.get(index)?;
    Some((&field.shape, child))
}

pub(crate) fn render_slot_shape_summary(
    ui: &mut egui::Ui,
    registry: &SlotShapeRegistry,
    shape: &SlotShape,
) {
    match resolve_shape(registry, shape) {
        Some(SlotShape::Ref { id }) => {
            ui.monospace(format!("ref {id}"));
        }
        Some(SlotShape::Unit { .. }) => {
            ui.monospace("unit");
        }
        Some(SlotShape::Value { shape }) => {
            ui.monospace(format_lp_type(&shape.ty));
            if shape.editor != Default::default() {
                ui.label(format!("{:?}", shape.editor));
            }
        }
        Some(SlotShape::Record { fields, .. }) => {
            ui.monospace(format!("record[{}]", fields.len()));
        }
        Some(SlotShape::Map { key, value, .. }) => {
            ui.monospace(format!("map<{key:?}>"));
            render_slot_shape_summary(ui, registry, value);
        }
        Some(SlotShape::Enum { variants, .. }) => {
            ui.monospace(format!("enum[{}]", variants.len()));
        }
        Some(SlotShape::Option { some, .. }) => {
            ui.monospace("option");
            render_slot_shape_summary(ui, registry, some);
        }
        None => {
            ui.colored_label(egui::Color32::LIGHT_RED, "missing referenced shape");
        }
    }
}

fn render_slot_shape_rows(
    ui: &mut egui::Ui,
    registry: &SlotShapeRegistry,
    shape: &SlotShape,
    data: &SlotData,
    depth: usize,
    id_path: &str,
    mut selection: Option<&mut Option<InspectorSelection>>,
) {
    let Some(shape) = resolve_shape(registry, shape) else {
        ui.colored_label(egui::Color32::LIGHT_RED, "Missing referenced shape");
        render_slot_data_fallback(ui, data);
        return;
    };

    match (shape, data) {
        (SlotShape::Record { fields, .. }, SlotData::Record(record)) => {
            for (index, field) in fields.iter().enumerate() {
                let Some(child) = record.fields.get(index) else {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!("missing field {}", field.name.as_str()),
                    );
                    continue;
                };
                render_named_slot_shape_row(
                    ui,
                    registry,
                    field.name.as_str(),
                    &field.shape,
                    child,
                    depth,
                    &format!("{id_path}.{}", field.name.as_str()),
                    selection.as_deref_mut(),
                );
            }
        }
        _ => render_named_slot_shape_row(
            ui, registry, "value", shape, data, depth, id_path, selection,
        ),
    }
}

fn render_named_slot_shape_row(
    ui: &mut egui::Ui,
    registry: &SlotShapeRegistry,
    name: &str,
    shape: &SlotShape,
    data: &SlotData,
    depth: usize,
    id_path: &str,
    mut selection: Option<&mut Option<InspectorSelection>>,
) {
    let Some(shape) = resolve_shape(registry, shape) else {
        ui.horizontal_wrapped(|ui| {
            indent(ui, depth);
            ui.monospace(name);
            ui.colored_label(egui::Color32::LIGHT_RED, "missing referenced shape");
        });
        return;
    };

    match (shape, data) {
        (SlotShape::Unit { .. }, SlotData::Unit { revision }) => {
            row(ui, depth, name, "unit", format!("rev {}", revision.0));
        }
        (SlotShape::Value { shape }, SlotData::Value(value)) => {
            render_value_row(
                ui,
                depth,
                name,
                shape,
                value.value(),
                value.changed_at().0,
                selection,
            );
        }
        (SlotShape::Record { fields, .. }, SlotData::Record(record)) => {
            egui::CollapsingHeader::new(format!("{name} ({})", fields.len()))
                .id_salt(("slot-row-record", id_path))
                .default_open(depth == 0 && name == "bindings")
                .show(ui, |ui| {
                    ui.small(format!("changed rev {}", record.fields_revision.0));
                    for (index, field) in fields.iter().enumerate() {
                        if let Some(child) = record.fields.get(index) {
                            render_named_slot_shape_row(
                                ui,
                                registry,
                                field.name.as_str(),
                                &field.shape,
                                child,
                                depth + 1,
                                &format!("{id_path}.{}", field.name.as_str()),
                                selection.as_deref_mut(),
                            );
                        }
                    }
                });
        }
        (SlotShape::Map { value, .. }, SlotData::Map(map)) => {
            egui::CollapsingHeader::new(format!("{name} ({})", map.entries.len()))
                .id_salt(("slot-row-map", id_path))
                .default_open(name == "bindings")
                .show(ui, |ui| {
                    ui.small(format!("keys changed rev {}", map.keys_revision.0));
                    for (key, child) in &map.entries {
                        let key_label = format_slot_map_key(key);
                        render_named_slot_shape_row(
                            ui,
                            registry,
                            &key_label,
                            value,
                            child,
                            depth + 1,
                            &format!("{id_path}[{key_label}]"),
                            selection.as_deref_mut(),
                        );
                    }
                });
        }
        (SlotShape::Enum { variants, .. }, SlotData::Enum(value)) => {
            let label = format!("{name} = {}", value.variant.as_str());
            egui::CollapsingHeader::new(label)
                .id_salt(("slot-row-enum", id_path))
                .default_open(false)
                .show(ui, |ui| {
                    ui.small(format!("variant changed rev {}", value.variant_revision.0));
                    if let Some(variant) = variants.iter().find(|v| v.name == value.variant) {
                        render_named_slot_shape_row(
                            ui,
                            registry,
                            value.variant.as_str(),
                            &variant.shape,
                            &value.data,
                            depth + 1,
                            &format!("{id_path}.{}", value.variant.as_str()),
                            selection.as_deref_mut(),
                        );
                    } else {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("missing variant shape {}", value.variant.as_str()),
                        );
                        render_slot_data_fallback(ui, &value.data);
                    }
                });
        }
        (SlotShape::Option { some, .. }, SlotData::Option(value)) => match &value.data {
            Some(child) => {
                egui::CollapsingHeader::new(name)
                    .id_salt(("slot-row-option", id_path))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.small(format!(
                            "presence changed rev {}",
                            value.presence_revision.0
                        ));
                        render_named_slot_shape_row(
                            ui,
                            registry,
                            "some",
                            some,
                            child,
                            depth + 1,
                            &format!("{id_path}.some"),
                            selection,
                        );
                    });
            }
            None => row(
                ui,
                depth,
                name,
                "none",
                format!("presence rev {}", value.presence_revision.0),
            ),
        },
        _ => {
            ui.horizontal_wrapped(|ui| {
                indent(ui, depth);
                ui.monospace(name);
                ui.colored_label(egui::Color32::LIGHT_RED, "shape/data mismatch");
                ui.monospace(format!("{data:?}"));
            });
        }
    }
}

fn render_value_row(
    ui: &mut egui::Ui,
    depth: usize,
    name: &str,
    shape: &SlotValueShape,
    value: &LpValue,
    revision: i64,
    mut selection: Option<&mut Option<InspectorSelection>>,
) {
    ui.horizontal_wrapped(|ui| {
        indent(ui, depth);
        ui.monospace(name);
        ui.label("=");
        match value {
            LpValue::Product(product) => {
                render_product_skeleton(ui, *product, selection.as_deref_mut());
            }
            LpValue::Resource(resource) => {
                render_resource_skeleton(ui, *resource, selection.as_deref_mut());
            }
            _ => {
                ui.monospace(format_lp_value(value));
            }
        }
        let response = ui.small(format!("rev {revision}"));
        let mut hover = format!("type {}", format_lp_type(&shape.ty));
        if let Some(editor) = format_value_editor_hint(&shape.editor) {
            hover.push_str(&format!("\neditor {editor}"));
        }
        response.on_hover_text(hover);
    });
}

pub(crate) fn render_product_skeleton(
    ui: &mut egui::Ui,
    product: ProductRef,
    selection: Option<&mut Option<InspectorSelection>>,
) {
    egui::Frame::default()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(8.0, 5.0))
        .rounding(4.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(product_kind_label(product));
                ui.monospace(format_product_ref(product));
                match product {
                    ProductRef::Visual(_) => {
                        ui.label("lazy");
                    }
                    ProductRef::Control(product) => {
                        ui.label("lazy");
                        let extent = product.preferred_extent();
                        ui.monospace(format!(
                            "{} samples",
                            extent.rows.saturating_mul(extent.samples_per_row)
                        ));
                    }
                }
                render_product_watch(ui, product, selection);
            });
        });
}

pub(crate) fn render_resource_skeleton(
    ui: &mut egui::Ui,
    resource: lpc_model::ResourceRef,
    selection: Option<&mut Option<InspectorSelection>>,
) {
    egui::Frame::default()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(8.0, 5.0))
        .rounding(4.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("resource");
                ui.monospace(format_resource_ref(resource));
                render_resource_watch(ui, resource, selection);
            });
        });
}

fn render_product_watch(
    ui: &mut egui::Ui,
    product: ProductRef,
    selection: Option<&mut Option<InspectorSelection>>,
) {
    let Some(selection) = selection else {
        return;
    };
    let Some(product) = product.as_visual() else {
        ui.add_enabled(false, egui::Checkbox::new(&mut false, "watch"));
        return;
    };
    let mut watched = *selection == Some(InspectorSelection::VisualProduct(product));
    if ui.checkbox(&mut watched, "watch").changed() {
        if watched {
            *selection = Some(InspectorSelection::VisualProduct(product));
        } else if *selection == Some(InspectorSelection::VisualProduct(product)) {
            *selection = None;
        }
    }
}

fn render_resource_watch(
    ui: &mut egui::Ui,
    resource: lpc_model::ResourceRef,
    selection: Option<&mut Option<InspectorSelection>>,
) {
    let Some(selection) = selection else {
        return;
    };
    let mut watched = *selection == Some(InspectorSelection::Resource(resource));
    if ui.checkbox(&mut watched, "watch").changed() {
        if watched {
            *selection = Some(InspectorSelection::Resource(resource));
        } else if *selection == Some(InspectorSelection::Resource(resource)) {
            *selection = None;
        }
    }
}

fn product_kind_label(product: ProductRef) -> &'static str {
    match product {
        ProductRef::Visual(_) => "visual product",
        ProductRef::Control(_) => "control product",
    }
}

fn render_slot_shape_debug(
    ui: &mut egui::Ui,
    registry: &SlotShapeRegistry,
    shape: &SlotShape,
    data: &SlotData,
    id_path: &str,
) {
    match resolve_shape(registry, shape) {
        Some(SlotShape::Unit { .. }) => {
            if let SlotData::Unit { revision } = data {
                ui.label(format!("unit  rev {}", revision.0));
            } else {
                render_shape_mismatch(ui, "unit", data);
            }
        }
        Some(SlotShape::Value { shape }) => {
            if let SlotData::Value(value) = data {
                ui.horizontal_wrapped(|ui| {
                    match value.value() {
                        LpValue::Product(product) => render_product_skeleton(ui, *product, None),
                        other => {
                            ui.monospace(format_lp_value(other));
                        }
                    }
                    ui.label(format!("rev {}", value.changed_at().0));
                    ui.label(format!("type {}", format_lp_type(&shape.ty)));
                    if shape.editor != Default::default() {
                        ui.label(format!("editor {:?}", shape.editor));
                    }
                });
            } else {
                render_shape_mismatch(ui, "value", data);
            }
        }
        Some(SlotShape::Record { fields, .. }) => {
            if let SlotData::Record(record) = data {
                ui.label(format!("fields rev {}", record.fields_revision.0));
                for (index, field) in fields.iter().enumerate() {
                    let Some(child) = record.fields.get(index) else {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("missing field {}", field.name.as_str()),
                        );
                        continue;
                    };
                    let child_path = format!("{id_path}.{}", field.name.as_str());
                    egui::CollapsingHeader::new(field.name.as_str())
                        .id_salt(("slot-debug-record-field", child_path.as_str()))
                        .default_open(false)
                        .show(ui, |ui| {
                            render_slot_shape_debug(ui, registry, &field.shape, child, &child_path);
                        });
                }
            } else {
                render_shape_mismatch(ui, "record", data);
            }
        }
        Some(SlotShape::Map { value, .. }) => {
            if let SlotData::Map(map) = data {
                ui.label(format!(
                    "{} entries  keys rev {}",
                    map.entries.len(),
                    map.keys_revision.0
                ));
                for (key, child) in &map.entries {
                    let key_label = format_slot_map_key(key);
                    let child_path = format!("{id_path}[{key_label}]");
                    egui::CollapsingHeader::new(key_label)
                        .id_salt(("slot-debug-map-key", child_path.as_str()))
                        .default_open(false)
                        .show(ui, |ui| {
                            render_slot_shape_debug(ui, registry, value, child, &child_path);
                        });
                }
            } else {
                render_shape_mismatch(ui, "map", data);
            }
        }
        Some(SlotShape::Enum { variants, .. }) => {
            if let SlotData::Enum(value) = data {
                ui.label(format!(
                    "variant {}  rev {}",
                    value.variant.as_str(),
                    value.variant_revision.0
                ));
                if let Some(variant) = variants.iter().find(|v| v.name == value.variant) {
                    let child_path = format!("{id_path}.{}", value.variant.as_str());
                    render_slot_shape_debug(ui, registry, &variant.shape, &value.data, &child_path);
                } else {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!("missing variant shape {}", value.variant.as_str()),
                    );
                    render_slot_data_fallback(ui, &value.data);
                }
            } else {
                render_shape_mismatch(ui, "enum", data);
            }
        }
        Some(SlotShape::Option { some, .. }) => {
            if let SlotData::Option(value) = data {
                ui.label(format!("presence rev {}", value.presence_revision.0));
                match &value.data {
                    Some(child) => {
                        let child_path = format!("{id_path}.some");
                        render_slot_shape_debug(ui, registry, some, child, &child_path);
                    }
                    None => {
                        ui.monospace("none");
                    }
                }
            } else {
                render_shape_mismatch(ui, "option", data);
            }
        }
        Some(SlotShape::Ref { .. }) => unreachable!("refs are resolved above"),
        None => {
            ui.colored_label(egui::Color32::LIGHT_RED, "Missing referenced shape");
            render_slot_data_fallback(ui, data);
        }
    }
}

fn resolve_shape<'a>(
    registry: &'a SlotShapeRegistry,
    shape: &'a SlotShape,
) -> Option<&'a SlotShape> {
    let mut shape = shape;
    while let SlotShape::Ref { id } = shape {
        shape = registry.get(id)?;
    }
    Some(shape)
}

fn row(ui: &mut egui::Ui, depth: usize, name: &str, value: &str, meta: String) {
    ui.horizontal_wrapped(|ui| {
        indent(ui, depth);
        ui.monospace(name);
        ui.label("=");
        ui.monospace(value);
        ui.label(meta);
    });
}

fn indent(ui: &mut egui::Ui, depth: usize) {
    ui.add_space((depth as f32) * 14.0);
}

pub(crate) fn format_slot_map_key(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(value) => value.clone(),
        SlotMapKey::I32(value) => value.to_string(),
        SlotMapKey::U32(value) => value.to_string(),
    }
}

pub(crate) fn root_name(node_id: lpc_model::NodeId, suffix: &str) -> String {
    format!("node.{}.{}", node_id.0, suffix)
}

fn render_shape_mismatch(ui: &mut egui::Ui, expected: &str, data: &SlotData) {
    ui.colored_label(
        egui::Color32::LIGHT_RED,
        format!("Shape/data mismatch: expected {expected}"),
    );
    render_slot_data_fallback(ui, data);
}

fn render_slot_data_fallback(ui: &mut egui::Ui, data: &SlotData) {
    ui.monospace(format!("{data:?}"));
}
