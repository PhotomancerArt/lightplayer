//! Shared slot rendering for node cards and debug inspector.

use eframe::egui;
use lpc_model::{
    LpValue, SlotData, SlotMapKey, SlotShape, SlotShapeId, SlotShapeRegistry, SlotValueShape,
};

use super::format::{format_lp_type, format_lp_value, format_product_ref, format_resource_ref};

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
) {
    match registry.get(&shape_id) {
        Some(shape) => render_slot_shape_rows(ui, registry, shape, data, 0, "root"),
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
) -> bool {
    let Some((shape, data)) = top_record_field(registry, shape_id, data, field_name) else {
        return false;
    };
    render_named_slot_shape_row(ui, registry, label, shape, data, 0, label);
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
) {
    let Some(shape) = resolve_shape(registry, shape) else {
        ui.colored_label(egui::Color32::LIGHT_RED, "Missing referenced shape");
        render_slot_data_fallback(ui, data);
        return;
    };

    match (shape, data) {
        (SlotShape::Record { fields, .. }, SlotData::Record(record)) => {
            ui.label(format!("fields rev {}", record.fields_revision.0));
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
                );
            }
        }
        _ => render_named_slot_shape_row(ui, registry, "value", shape, data, depth, id_path),
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
            render_value_row(ui, depth, name, shape, value.value(), value.changed_at().0);
        }
        (SlotShape::Record { fields, .. }, SlotData::Record(record)) => {
            egui::CollapsingHeader::new(format!("{name}  record[{}]", fields.len()))
                .id_salt(("slot-row-record", id_path))
                .default_open(depth == 0 && name == "bindings")
                .show(ui, |ui| {
                    ui.label(format!("fields rev {}", record.fields_revision.0));
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
                            );
                        }
                    }
                });
        }
        (SlotShape::Map { value, .. }, SlotData::Map(map)) => {
            egui::CollapsingHeader::new(format!(
                "{name}  map[{}]  keys rev {}",
                map.entries.len(),
                map.keys_revision.0
            ))
            .id_salt(("slot-row-map", id_path))
            .default_open(name == "bindings")
            .show(ui, |ui| {
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
                    );
                }
            });
        }
        (SlotShape::Enum { variants, .. }, SlotData::Enum(value)) => {
            let label = format!(
                "{name}  variant {}  rev {}",
                value.variant.as_str(),
                value.variant_revision.0
            );
            egui::CollapsingHeader::new(label)
                .id_salt(("slot-row-enum", id_path))
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(variant) = variants.iter().find(|v| v.name == value.variant) {
                        render_named_slot_shape_row(
                            ui,
                            registry,
                            value.variant.as_str(),
                            &variant.shape,
                            &value.data,
                            depth + 1,
                            &format!("{id_path}.{}", value.variant.as_str()),
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
                egui::CollapsingHeader::new(format!(
                    "{name}  some  rev {}",
                    value.presence_revision.0
                ))
                .id_salt(("slot-row-option", id_path))
                .default_open(false)
                .show(ui, |ui| {
                    render_named_slot_shape_row(
                        ui,
                        registry,
                        "some",
                        some,
                        child,
                        depth + 1,
                        &format!("{id_path}.some"),
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
) {
    ui.horizontal_wrapped(|ui| {
        indent(ui, depth);
        ui.monospace(name);
        ui.label("=");
        match value {
            LpValue::Product(product) => {
                ui.strong(format_product_ref(*product));
                ui.add_enabled(false, egui::Button::new("probe"));
            }
            LpValue::Resource(resource) => {
                ui.strong(format_resource_ref(*resource));
                ui.add_enabled(false, egui::Button::new("details"));
            }
            _ => {
                ui.monospace(format_lp_value(value));
            }
        }
        ui.label(format!("rev {revision}"));
        ui.label(format_lp_type(&shape.ty));
        if shape.editor != Default::default() {
            ui.label(format!("{:?}", shape.editor));
        }
    });
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
                    ui.monospace(format_lp_value(value.value()));
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
