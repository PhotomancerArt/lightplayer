use std::collections::BTreeMap;

use lpc_model::{
    SlotData, SlotDataAccess, SlotMapDyn, SlotName, SlotOptionDyn, SlotShapeId, SlotShapeNode,
    SlotShapeRegistry, Versioned,
};

use crate::engine::MockRuntime;

use super::types::FullSync;

pub fn full_sync(runtime: &MockRuntime) -> FullSync {
    FullSync {
        registry: runtime.registry.snapshot(),
        roots: runtime
            .roots()
            .into_iter()
            .map(|(name, root)| {
                let shape_id = root.shape_id();
                (
                    name.to_string(),
                    shape_id.clone(),
                    snapshot(&shape_id, root.data(), &runtime.registry),
                )
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
    match (shape, data) {
        (SlotShapeNode::Value { .. }, SlotDataAccess::Value(value)) => {
            SlotData::Value(Versioned::new(value.changed_frame(), value.value()))
        }
        (SlotShapeNode::Record { fields, .. }, SlotDataAccess::Record(record)) => {
            SlotData::Record(lpc_model::SlotRecord::with_version(
                record.fields_changed_frame(),
                fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        snapshot(
                            field.shape.id(),
                            record.field(index).expect("field"),
                            registry,
                        )
                    })
                    .collect(),
            ))
        }
        (SlotShapeNode::Map { value, .. }, SlotDataAccess::Map(map)) => {
            let mut entries = BTreeMap::new();
            for key in map.keys() {
                entries.insert(
                    key.clone(),
                    snapshot(value.id(), map.get(&key).expect("map entry"), registry),
                );
            }
            SlotData::Map(SlotMapDyn::with_version(map.keys_changed_frame(), entries))
        }
        (SlotShapeNode::Enum { variants, .. }, SlotDataAccess::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == en.variant())
                .expect("variant");
            SlotData::Enum(lpc_model::SlotEnum::with_version(
                en.variant_changed_frame(),
                SlotName::parse(en.variant()).unwrap(),
                snapshot(variant.shape.id(), en.data(), registry),
            ))
        }
        (SlotShapeNode::Option { some, .. }, SlotDataAccess::Option(option)) => match option.data()
        {
            Some(data) => SlotData::Option(SlotOptionDyn::some_with_version(
                option.presence_changed_frame(),
                snapshot(some.id(), data, registry),
            )),
            None => SlotData::Option(SlotOptionDyn::none_with_version(
                option.presence_changed_frame(),
            )),
        },
        _ => panic!("shape/data mismatch"),
    }
}
