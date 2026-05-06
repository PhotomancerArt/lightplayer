use lpc_model::{SlotAccess, SlotDataAccess, SlotShapeId, SlotShapeNode, SlotShapeRegistry};

use super::path::key_segment;

pub fn print_root(root: &dyn SlotAccess, registry: &SlotShapeRegistry) -> Vec<String> {
    let mut lines = Vec::new();
    print_inner(
        "<root>".to_string(),
        &root.shape_id(),
        root.data(),
        registry,
        &mut lines,
    );
    lines
}

fn print_inner(
    path: String,
    shape_id: &SlotShapeId,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
    lines: &mut Vec<String>,
) {
    match (registry.get(shape_id).expect("shape"), data) {
        (SlotShapeNode::Value { .. }, SlotDataAccess::Value(value)) => {
            lines.push(format!("{path}: {:?}", value.value()));
        }
        (SlotShapeNode::Record { fields, .. }, SlotDataAccess::Record(record)) => {
            lines.push(format!("{path}: record"));
            for (index, field) in fields.iter().enumerate() {
                if let Some(child) = record.field(index) {
                    print_inner(
                        format!("{path}.{}", field.name),
                        field.shape.id(),
                        child,
                        registry,
                        lines,
                    );
                }
            }
        }
        (SlotShapeNode::Map { value, .. }, SlotDataAccess::Map(map)) => {
            lines.push(format!("{path}: map"));
            for key in map.keys() {
                if let Some(child) = map.get(&key) {
                    print_inner(
                        format!("{path}.{}", key_segment(&key)),
                        value.id(),
                        child,
                        registry,
                        lines,
                    );
                }
            }
        }
        (SlotShapeNode::Enum { variants, .. }, SlotDataAccess::Enum(en)) => {
            lines.push(format!("{path}: enum {}", en.variant()));
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == en.variant())
                .expect("variant");
            print_inner(
                format!("{path}.{}", en.variant()),
                variant.shape.id(),
                en.data(),
                registry,
                lines,
            );
        }
        (SlotShapeNode::Option { some, .. }, SlotDataAccess::Option(option)) => {
            lines.push(format!(
                "{path}: option {}",
                if option.data().is_some() {
                    "some"
                } else {
                    "none"
                }
            ));
            if let Some(child) = option.data() {
                print_inner(format!("{path}.some"), some.id(), child, registry, lines);
            }
        }
        _ => panic!("shape/data mismatch"),
    }
}
