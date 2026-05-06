use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec::Vec;
use lpc_model::{
    FrameId, SlotAccess, SlotData, SlotDataAccess, SlotMapDyn, SlotName, SlotOptionDyn, SlotPath,
    SlotShape, SlotShapeId, SlotShapeRegistry, Versioned,
};

use super::{WireSlotChange, WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot};

/// Build a full slot sync payload from borrowed slot roots.
pub fn build_slot_full_sync<'a>(
    registry: &SlotShapeRegistry,
    roots: impl IntoIterator<Item = (&'a str, &'a dyn SlotAccess)>,
) -> WireSlotFullSync {
    WireSlotFullSync {
        registry: registry.snapshot(),
        roots: roots
            .into_iter()
            .map(|(name, root)| {
                let shape_id = root.shape_id();
                WireSlotRootSnapshot {
                    name: name.to_string(),
                    shape: shape_id,
                    data: snapshot_slot_root(&shape_id, root.data(), registry),
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
    let shape = registry.get(shape_id).expect("slot shape is registered");
    snapshot_slot_shape(shape, data, registry)
}

/// Snapshot one data node through a concrete shape.
pub fn snapshot_slot_shape(
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> SlotData {
    match (shape, data) {
        (SlotShape::Ref { id }, data) => snapshot_slot_root(id, data, registry),
        (SlotShape::Unit { .. }, SlotDataAccess::Unit(frame)) => SlotData::Unit {
            changed_frame: frame,
        },
        (SlotShape::Value { .. }, SlotDataAccess::Value(value)) => {
            SlotData::Value(Versioned::new(value.changed_frame(), value.value()))
        }
        (SlotShape::Record { fields, .. }, SlotDataAccess::Record(record)) => {
            SlotData::Record(lpc_model::SlotRecord::with_version(
                record.fields_changed_frame(),
                fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        snapshot_slot_shape(
                            &field.shape,
                            record.field(index).expect("record field exists"),
                            registry,
                        )
                    })
                    .collect(),
            ))
        }
        (SlotShape::Map { value, .. }, SlotDataAccess::Map(map)) => {
            let mut entries = BTreeMap::new();
            for key in map.keys() {
                entries.insert(
                    key.clone(),
                    snapshot_slot_shape(value, map.get(&key).expect("map entry exists"), registry),
                );
            }
            SlotData::Map(SlotMapDyn::with_version(map.keys_changed_frame(), entries))
        }
        (SlotShape::Enum { variants, .. }, SlotDataAccess::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == en.variant())
                .expect("enum variant exists in shape");
            SlotData::Enum(lpc_model::SlotEnum::with_version(
                en.variant_changed_frame(),
                SlotName::parse(en.variant()).expect("enum variant is a slot name"),
                snapshot_slot_shape(&variant.shape, en.data(), registry),
            ))
        }
        (SlotShape::Option { some, .. }, SlotDataAccess::Option(option)) => match option.data() {
            Some(data) => SlotData::Option(SlotOptionDyn::some_with_version(
                option.presence_changed_frame(),
                snapshot_slot_shape(some, data, registry),
            )),
            None => SlotData::Option(SlotOptionDyn::none_with_version(
                option.presence_changed_frame(),
            )),
        },
        _ => panic!("slot shape/data mismatch"),
    }
}

/// Collect changed slot patches for one root.
pub fn collect_slot_diff(
    root_name: &str,
    root: &dyn SlotAccess,
    registry: &SlotShapeRegistry,
    since: FrameId,
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
    since: FrameId,
    patches: &mut Vec<WireSlotPatch>,
) {
    let shape = registry.get(shape_id).expect("slot shape is registered");
    collect_diff_shape(root_name, path, shape, data, registry, since, patches);
}

fn collect_diff_shape(
    root_name: &str,
    path: SlotPath,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
    since: FrameId,
    patches: &mut Vec<WireSlotPatch>,
) {
    match (shape, data) {
        (SlotShape::Ref { id }, data) => {
            collect_diff_inner(root_name, path, id, data, registry, since, patches);
        }
        (SlotShape::Unit { .. }, SlotDataAccess::Unit(frame)) => {
            if frame > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path,
                    change: WireSlotChange::Replace(SlotData::Unit {
                        changed_frame: frame,
                    }),
                });
            }
        }
        (SlotShape::Value { .. }, SlotDataAccess::Value(value)) => {
            if value.changed_frame() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path,
                    change: WireSlotChange::Replace(SlotData::Value(Versioned::new(
                        value.changed_frame(),
                        value.value(),
                    ))),
                });
            }
        }
        (SlotShape::Record { fields, .. }, SlotDataAccess::Record(record)) => {
            if record.fields_changed_frame() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: WireSlotChange::Replace(snapshot_slot_shape(shape, data, registry)),
                });
            }
            for (index, field) in fields.iter().enumerate() {
                if let Some(child) = record.field(index) {
                    collect_diff_shape(
                        root_name,
                        path.child(field.name.clone()),
                        &field.shape,
                        child,
                        registry,
                        since,
                        patches,
                    );
                }
            }
        }
        (SlotShape::Map { value, .. }, SlotDataAccess::Map(map)) => {
            if map.keys_changed_frame() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: WireSlotChange::Replace(snapshot_slot_shape(shape, data, registry)),
                });
            }
            for key in map.keys() {
                if let Some(child) = map.get(&key) {
                    collect_diff_shape(
                        root_name,
                        path.child_key(key.clone()),
                        value,
                        child,
                        registry,
                        since,
                        patches,
                    );
                }
            }
        }
        (SlotShape::Enum { variants, .. }, SlotDataAccess::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == en.variant())
                .expect("enum variant exists in shape");
            if en.variant_changed_frame() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: WireSlotChange::Replace(snapshot_slot_shape(shape, data, registry)),
                });
            }
            collect_diff_shape(
                root_name,
                path.child(SlotName::parse(en.variant()).expect("enum variant is a slot name")),
                &variant.shape,
                en.data(),
                registry,
                since,
                patches,
            );
        }
        (SlotShape::Option { some, .. }, SlotDataAccess::Option(option)) => {
            if option.presence_changed_frame() > since {
                patches.push(WireSlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: WireSlotChange::Replace(snapshot_slot_shape(shape, data, registry)),
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
        _ => panic!("slot shape/data mismatch"),
    }
}
