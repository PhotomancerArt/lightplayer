use std::collections::BTreeMap;

use lpc_model::{
    SlotData, SlotMapKey, SlotMapKeyShape, SlotPath, SlotShape, SlotShapeId, SlotShapeRegistry,
};

use crate::wire::{FullSync, SlotChange, SlotPatch};

#[derive(Default)]
pub struct MockClient {
    pub registry: SlotShapeRegistry,
    pub root_shapes: BTreeMap<String, SlotShapeId>,
    pub roots: BTreeMap<String, SlotData>,
}

impl MockClient {
    pub fn apply_full_sync(&mut self, sync: FullSync) {
        self.registry.apply_snapshot(sync.registry);
        for (name, shape, data) in sync.roots {
            self.root_shapes.insert(name.clone(), shape);
            self.roots.insert(name, data);
        }
    }

    pub fn apply_patches(&mut self, patches: Vec<SlotPatch>) {
        for patch in patches {
            let shape_id = self
                .root_shapes
                .get(&patch.root)
                .expect("root shape")
                .clone();
            let data = self.roots.get_mut(&patch.root).expect("root data");
            apply_replace(data, &shape_id, &patch.path, patch.change, &self.registry);
        }
    }
}

fn apply_replace(
    data: &mut SlotData,
    shape_id: &SlotShapeId,
    path: &SlotPath,
    change: SlotChange,
    registry: &SlotShapeRegistry,
) {
    let shape = registry.get(shape_id).expect("shape");
    apply_replace_shape(data, shape, path, change, registry);
}

fn apply_replace_shape(
    data: &mut SlotData,
    shape: &SlotShape,
    path: &SlotPath,
    change: SlotChange,
    registry: &SlotShapeRegistry,
) {
    if let SlotShape::Ref { id } = shape {
        let shape = registry.get(id).expect("shape ref");
        apply_replace_shape(data, shape, path, change, registry);
        return;
    }

    if path.is_root() {
        match change {
            SlotChange::Replace(replacement) => *data = replacement,
        }
        return;
    }

    let (head, tail) = path.segments().split_first().expect("path");
    let tail = SlotPath::from_segments(tail.to_vec());
    match (shape, data) {
        (SlotShape::Record { fields, .. }, SlotData::Record(record)) => {
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == *head)
                .expect("record field");
            apply_replace_shape(
                &mut record.fields[index],
                &field.shape,
                &tail,
                change,
                registry,
            );
        }
        (SlotShape::Map { key, value, .. }, SlotData::Map(map)) => {
            let key = parse_map_key(head.as_str(), *key);
            apply_replace_shape(
                map.entries.get_mut(&key).expect("map key"),
                value,
                &tail,
                change,
                registry,
            );
        }
        (SlotShape::Enum { variants, .. }, SlotData::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name == *head)
                .expect("enum variant");
            apply_replace_shape(&mut en.data, &variant.shape, &tail, change, registry);
        }
        (SlotShape::Option { some, .. }, SlotData::Option(option)) => {
            assert_eq!(head.as_str(), "some");
            apply_replace_shape(
                option.data.as_mut().expect("some"),
                some,
                &tail,
                change,
                registry,
            );
        }
        (SlotShape::Value { .. }, SlotData::Value(_)) => {
            panic!("cannot walk through a value slot")
        }
        _ => panic!("shape/data mismatch"),
    }
}

fn parse_map_key(value: &str, shape: SlotMapKeyShape) -> SlotMapKey {
    match shape {
        SlotMapKeyShape::String => SlotMapKey::String(value.to_string()),
        SlotMapKeyShape::I32 => SlotMapKey::I32(value.parse().unwrap()),
        SlotMapKeyShape::U32 => SlotMapKey::U32(value.parse().unwrap()),
    }
}
