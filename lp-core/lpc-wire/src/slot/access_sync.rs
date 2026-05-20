use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use lpc_model::{
    Revision, SlotAccess, SlotData, SlotDataAccess, SlotMapDyn, SlotName, SlotOptionDyn, SlotPath,
    SlotShape, SlotShapeId, SlotShapeLookup, SlotShapeRegistry, SlotShapeView, WithRevision,
    slot_codec::SlotWriter,
    slot_sync_codec::{
        read_slot_snapshot_shape_json, write_slot_snapshot_shape_value, write_slot_snapshot_value,
    },
};

use super::{
    WireSlotChange, WireSlotData, WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot,
    WireSlotRootsSnapshot,
};

/// Build a full slot sync payload from borrowed slot roots.
pub fn build_slot_full_sync<'a>(
    registry: &SlotShapeRegistry,
    roots: impl IntoIterator<Item = (&'a str, &'a dyn SlotAccess)>,
) -> WireSlotFullSync {
    WireSlotFullSync {
        registry: registry.snapshot_with_static_catalog(),
        roots: roots
            .into_iter()
            .map(|(name, root)| {
                let shape_id = root.shape_id();
                WireSlotRootSnapshot {
                    name: name.to_string(),
                    shape: shape_id,
                    data: wire_slot_data_from_slot_access(registry, shape_id, root.data()),
                }
            })
            .collect(),
    }
}

/// Build root snapshots without including the shape registry.
pub fn build_slot_roots_snapshot<'a>(
    registry: &SlotShapeRegistry,
    roots: impl IntoIterator<Item = (&'a str, &'a dyn SlotAccess)>,
) -> WireSlotRootsSnapshot {
    WireSlotRootsSnapshot {
        roots: roots
            .into_iter()
            .map(|(name, root)| {
                let shape_id = root.shape_id();
                WireSlotRootSnapshot {
                    name: name.to_string(),
                    shape: shape_id,
                    data: wire_slot_data_from_slot_access(registry, shape_id, root.data()),
                }
            })
            .collect(),
    }
}

/// Snapshot one slot root through its registered shape.
pub fn snapshot_slot_root(
    shape_id: &SlotShapeId,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> SlotData {
    let shape = registry
        .get_shape(*shape_id)
        .expect("slot shape is registered");
    snapshot_slot_shape(shape, data, registry)
}

/// Snapshot one data node through a borrowed shape view.
pub fn snapshot_slot_shape(
    shape: SlotShapeView<'_>,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> SlotData {
    if let Some(id) = shape.ref_id() {
        return snapshot_slot_root(&id, data, registry);
    }

    match data {
        SlotDataAccess::Unit(frame) if shape.is_unit() => SlotData::Unit { revision: frame },
        SlotDataAccess::Value(value) if shape.value_shape().is_some() => {
            SlotData::Value(WithRevision::new(value.changed_at(), value.value()))
        }
        SlotDataAccess::Record(record) => {
            let fields_len = shape.record_fields_len().expect("slot shape/data mismatch");
            SlotData::Record(lpc_model::SlotRecord::with_revision(
                record.fields_revision(),
                (0..fields_len)
                    .map(|index| {
                        let field = shape.record_field(index).expect("record field exists");
                        snapshot_slot_shape(
                            field.shape(),
                            record.field(index).unwrap_or_else(|| {
                                panic!("record field {} exists", field.name_str())
                            }),
                            registry,
                        )
                    })
                    .collect(),
            ))
        }
        SlotDataAccess::Map(map) => {
            let value = shape.map_value().expect("slot shape/data mismatch");
            let mut entries = BTreeMap::new();
            for key in map.keys() {
                entries.insert(
                    key.clone(),
                    snapshot_slot_shape(value, map.get(&key).expect("map entry exists"), registry),
                );
            }
            SlotData::Map(SlotMapDyn::with_revision(map.keys_revision(), entries))
        }
        SlotDataAccess::Enum(en) => {
            let variant_name = SlotName::parse(en.variant()).expect("enum variant is a slot name");
            let variant = shape
                .enum_variant_by_name(&variant_name)
                .expect("enum variant exists in shape");
            SlotData::Enum(lpc_model::SlotEnum::with_version(
                en.variant_revision(),
                variant_name,
                snapshot_slot_shape(variant.shape(), en.data(), registry),
            ))
        }
        SlotDataAccess::Option(option) => {
            let some = shape.option_some().expect("slot shape/data mismatch");
            match option.data() {
                Some(data) => SlotData::Option(SlotOptionDyn::some_with_version(
                    option.presence_revision(),
                    snapshot_slot_shape(some, data, registry),
                )),
                None => {
                    SlotData::Option(SlotOptionDyn::none_with_version(option.presence_revision()))
                }
            }
        }
        SlotDataAccess::Custom(custom) if shape.custom_codec().is_some() => {
            let owned_shape = shape.to_owned_shape();
            let data = wire_slot_data_from_slot_shape(
                registry,
                &owned_shape,
                SlotDataAccess::Custom(custom),
            );
            read_slot_snapshot_shape_json(registry, &owned_shape, data.get())
                .expect("custom slot sync snapshot decodes")
        }
        _ => panic!("slot shape/data mismatch"),
    }
}

/// Collect changed slot patches for one root.
pub fn collect_slot_diff(
    root_name: &str,
    root: &dyn SlotAccess,
    registry: &SlotShapeRegistry,
    since: Revision,
) -> Vec<WireSlotPatch> {
    let mut patches = Vec::new();
    collect_diff_inner(
        root_name,
        SlotPath::root(),
        &root.shape_id(),
        root.data(),
        registry,
        since,
        &mut patches,
    );
    patches
}

fn collect_diff_inner(
    root_name: &str,
    path: SlotPath,
    shape_id: &SlotShapeId,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
    since: Revision,
    patches: &mut Vec<WireSlotPatch>,
) {
    let shape = registry
        .get_shape(*shape_id)
        .expect("slot shape is registered");
    collect_diff_shape(root_name, path, shape, data, registry, since, patches);
}

fn collect_diff_shape(
    root_name: &str,
    path: SlotPath,
    shape: SlotShapeView<'_>,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
    since: Revision,
    patches: &mut Vec<WireSlotPatch>,
) {
    if let Some(id) = shape.ref_id() {
        collect_diff_inner(root_name, path, &id, data, registry, since, patches);
        return;
    }

    match data {
        SlotDataAccess::Unit(frame) if shape.is_unit() => {
            if frame > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path,
                    change: WireSlotChange::Replace(wire_slot_data_from_slot_shape_view(
                        registry,
                        shape,
                        SlotDataAccess::Unit(frame),
                    )),
                });
            }
        }
        SlotDataAccess::Value(value) if shape.value_shape().is_some() => {
            if value.changed_at() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path,
                    change: WireSlotChange::Replace(wire_slot_data_from_slot_shape_view(
                        registry,
                        shape,
                        SlotDataAccess::Value(value),
                    )),
                });
            }
        }
        SlotDataAccess::Record(record) => {
            let fields_len = shape.record_fields_len().expect("slot shape/data mismatch");
            if record.fields_revision() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: WireSlotChange::Replace(wire_slot_data_from_slot_shape_view(
                        registry, shape, data,
                    )),
                });
            }
            for index in 0..fields_len {
                let field = shape.record_field(index).expect("record field exists");
                if let Some(child) = record.field(index) {
                    collect_diff_shape(
                        root_name,
                        path.child(SlotName::parse(field.name_str()).expect("valid slot name")),
                        field.shape(),
                        child,
                        registry,
                        since,
                        patches,
                    );
                }
            }
        }
        SlotDataAccess::Map(map) => {
            let value_shape = shape.map_value().expect("slot shape/data mismatch");
            if map.keys_revision() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: WireSlotChange::Replace(wire_slot_data_from_slot_shape_view(
                        registry, shape, data,
                    )),
                });
            }
            for key in map.keys() {
                if let Some(child) = map.get(&key) {
                    collect_diff_shape(
                        root_name,
                        path.child_key(key.clone()),
                        value_shape,
                        child,
                        registry,
                        since,
                        patches,
                    );
                }
            }
        }
        SlotDataAccess::Enum(en) => {
            let variant_name = SlotName::parse(en.variant()).expect("enum variant is a slot name");
            let variant = shape
                .enum_variant_by_name(&variant_name)
                .expect("enum variant exists in shape");
            if en.variant_revision() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: WireSlotChange::Replace(wire_slot_data_from_slot_shape_view(
                        registry, shape, data,
                    )),
                });
            }
            collect_diff_shape(
                root_name,
                path.child(variant_name),
                variant.shape(),
                en.data(),
                registry,
                since,
                patches,
            );
        }
        SlotDataAccess::Option(option) => {
            let some = shape.option_some().expect("slot shape/data mismatch");
            if option.presence_revision() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: WireSlotChange::Replace(wire_slot_data_from_slot_shape_view(
                        registry, shape, data,
                    )),
                });
            }
            if let Some(child) = option.data() {
                collect_diff_shape(
                    root_name,
                    path.child(SlotName::parse("some").expect("valid option slot name")),
                    some,
                    child,
                    registry,
                    since,
                    patches,
                );
            }
        }
        SlotDataAccess::Custom(custom) if shape.custom_codec().is_some() => {
            if custom.custom_revision() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path,
                    change: WireSlotChange::Replace(wire_slot_data_from_slot_shape_view(
                        registry, shape, data,
                    )),
                });
            }
        }
        _ => panic!("slot shape/data mismatch"),
    }
}

pub fn wire_slot_data_from_slot_access(
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    data: SlotDataAccess<'_>,
) -> WireSlotData {
    let mut writer = SlotWriter::new(Vec::new());
    write_slot_snapshot_value(registry, shape_id, data, writer.value())
        .expect("slot sync snapshot writes to vec");
    raw_wire_slot_data(writer.into_inner())
}

fn wire_slot_data_from_slot_shape(
    registry: &SlotShapeRegistry,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
) -> WireSlotData {
    let mut writer = SlotWriter::new(Vec::new());
    write_slot_snapshot_shape_value(registry, shape, data, writer.value())
        .expect("slot sync snapshot writes to vec");
    raw_wire_slot_data(writer.into_inner())
}

fn wire_slot_data_from_slot_shape_view(
    registry: &SlotShapeRegistry,
    shape: SlotShapeView<'_>,
    data: SlotDataAccess<'_>,
) -> WireSlotData {
    match shape {
        SlotShapeView::Static(_) => {
            let owned_shape = shape.to_owned_shape();
            wire_slot_data_from_slot_shape(registry, &owned_shape, data)
        }
        SlotShapeView::Dynamic(shape) => wire_slot_data_from_slot_shape(registry, shape, data),
    }
}

fn raw_wire_slot_data(bytes: Vec<u8>) -> WireSlotData {
    WireSlotData::from_json_string(String::from_utf8(bytes).expect("slot sync JSON is UTF-8"))
        .expect("slot sync codec writes valid JSON")
}
