use std::collections::BTreeMap;

use lpc_model::{
    SlotData, SlotDataAccess, SlotMapDyn, SlotName, SlotOptionDyn, SlotShape, SlotShapeId,
    SlotShapeRegistry, Versioned,
};
use lpc_wire::{WireSlotFullSync, WireSlotRootSnapshot};

use crate::engine::MockRuntime;

pub fn full_sync(runtime: &MockRuntime) -> WireSlotFullSync {
    WireSlotFullSync {
        registry: runtime.registry.snapshot(),
        roots: runtime
            .roots()
            .into_iter()
            .map(|(name, root)| {
                let shape_id = root.shape_id();
                WireSlotRootSnapshot {
                    name: name.to_string(),
                    shape: shape_id.clone(),
                    data: snapshot(&shape_id, root.data(), &runtime.registry),
                }
            })
            .collect(),
    }
}

pub(super) fn snapshot(
    shape_id: &SlotShapeId,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> SlotData {
    let shape = registry.get(shape_id).expect("shape");
    snapshot_shape(shape, data, registry)
}

pub(super) fn snapshot_shape(
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> SlotData {
    match (shape, data) {
        (SlotShape::Ref { id }, data) => snapshot(id, data, registry),
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
                        snapshot_shape(&field.shape, record.field(index).expect("field"), registry)
                    })
                    .collect(),
            ))
        }
        (SlotShape::Map { value, .. }, SlotDataAccess::Map(map)) => {
            let mut entries = BTreeMap::new();
            for key in map.keys() {
                entries.insert(
                    key.clone(),
                    snapshot_shape(value, map.get(&key).expect("map entry"), registry),
                );
            }
            SlotData::Map(SlotMapDyn::with_version(map.keys_changed_frame(), entries))
        }
        (SlotShape::Enum { variants, .. }, SlotDataAccess::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == en.variant())
                .expect("variant");
            SlotData::Enum(lpc_model::SlotEnum::with_version(
                en.variant_changed_frame(),
                SlotName::parse(en.variant()).unwrap(),
                snapshot_shape(&variant.shape, en.data(), registry),
            ))
        }
        (SlotShape::Option { some, .. }, SlotDataAccess::Option(option)) => match option.data() {
            Some(data) => SlotData::Option(SlotOptionDyn::some_with_version(
                option.presence_changed_frame(),
                snapshot_shape(some, data, registry),
            )),
            None => SlotData::Option(SlotOptionDyn::none_with_version(
                option.presence_changed_frame(),
            )),
        },
        _ => panic!("shape/data mismatch"),
    }
}
