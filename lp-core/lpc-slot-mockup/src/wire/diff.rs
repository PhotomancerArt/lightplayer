use lpc_model::{
    FrameId, SlotAccess, SlotData, SlotDataAccess, SlotName, SlotPath, SlotShapeId, SlotShapeNode,
    SlotShapeRegistry, Versioned,
};

use super::path::slot_name_for_key;
use super::snapshot::snapshot;
use super::types::{SlotChange, SlotPatch};

pub fn collect_diff(
    root_name: &str,
    root: &dyn SlotAccess,
    registry: &SlotShapeRegistry,
    since: FrameId,
) -> Vec<SlotPatch> {
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
    patches: &mut Vec<SlotPatch>,
) {
    let shape = registry.get(shape_id).expect("shape");
    match (shape, data) {
        (SlotShapeNode::Value { .. }, SlotDataAccess::Value(value)) => {
            if value.changed_frame() > since {
                patches.push(SlotPatch {
                    root: root_name.to_string(),
                    path,
                    change: SlotChange::Replace(SlotData::Value(Versioned::new(
                        value.changed_frame(),
                        value.value(),
                    ))),
                });
            }
        }
        (SlotShapeNode::Record { fields, .. }, SlotDataAccess::Record(record)) => {
            if record.fields_changed_frame() > since {
                patches.push(SlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: SlotChange::Replace(snapshot(shape_id, data, registry)),
                });
            }
            for (index, field) in fields.iter().enumerate() {
                if let Some(child) = record.field(index) {
                    collect_diff_inner(
                        root_name,
                        path.child(field.name.clone()),
                        field.shape.id(),
                        child,
                        registry,
                        since,
                        patches,
                    );
                }
            }
        }
        (SlotShapeNode::Map { value, .. }, SlotDataAccess::Map(map)) => {
            if map.keys_changed_frame() > since {
                patches.push(SlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: SlotChange::Replace(snapshot(shape_id, data, registry)),
                });
            }
            for key in map.keys() {
                if let Some(child) = map.get(&key) {
                    collect_diff_inner(
                        root_name,
                        path.child(slot_name_for_key(&key)),
                        value.id(),
                        child,
                        registry,
                        since,
                        patches,
                    );
                }
            }
        }
        (SlotShapeNode::Enum { variants, .. }, SlotDataAccess::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == en.variant())
                .expect("variant");
            if en.variant_changed_frame() > since {
                patches.push(SlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: SlotChange::Replace(snapshot(shape_id, data, registry)),
                });
            }
            collect_diff_inner(
                root_name,
                path.child(SlotName::parse(en.variant()).unwrap()),
                variant.shape.id(),
                en.data(),
                registry,
                since,
                patches,
            );
        }
        (SlotShapeNode::Option { some, .. }, SlotDataAccess::Option(option)) => {
            if option.presence_changed_frame() > since {
                patches.push(SlotPatch {
                    root: root_name.to_string(),
                    path: path.clone(),
                    change: SlotChange::Replace(snapshot(shape_id, data, registry)),
                });
            }
            if let Some(child) = option.data() {
                collect_diff_inner(
                    root_name,
                    path.child(SlotName::parse("some").unwrap()),
                    some.id(),
                    child,
                    registry,
                    since,
                    patches,
                );
            }
        }
        _ => panic!("shape/data mismatch"),
    }
}
