use lpc_model::{SlotAccess, SlotData, SlotDataAccess, SlotShape, SlotShapeId, SlotShapeRegistry};

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

pub fn print_data_root(
    shape_id: &SlotShapeId,
    data: &SlotData,
    registry: &SlotShapeRegistry,
) -> Vec<String> {
    let mut lines = Vec::new();
    print_inner(
        "<root>".to_string(),
        shape_id,
        data.access(),
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
    let shape = registry.get(shape_id).expect("shape");
    print_shape(path, shape, data, registry, lines);
}

fn print_shape(
    path: String,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
    lines: &mut Vec<String>,
) {
    match (shape, data) {
        (SlotShape::Ref { id }, data) => print_inner(path, id, data, registry, lines),
        (SlotShape::Value { .. }, SlotDataAccess::Value(value)) => {
            lines.push(format!("{path}: {:?}", value.value()));
        }
        (SlotShape::Record { fields, .. }, SlotDataAccess::Record(record)) => {
            lines.push(format!("{path}: record"));
            for (index, field) in fields.iter().enumerate() {
                if let Some(child) = record.field(index) {
                    print_shape(
                        format!("{path}.{}", field.name),
                        &field.shape,
                        child,
                        registry,
                        lines,
                    );
                }
            }
        }
        (SlotShape::Map { value, .. }, SlotDataAccess::Map(map)) => {
            lines.push(format!("{path}: map"));
            for key in map.keys() {
                if let Some(child) = map.get(&key) {
                    print_shape(
                        format!("{path}.{}", key_segment(&key)),
                        value,
                        child,
                        registry,
                        lines,
                    );
                }
            }
        }
        (SlotShape::Enum { variants, .. }, SlotDataAccess::Enum(en)) => {
            lines.push(format!("{path}: enum {}", en.variant()));
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == en.variant())
                .expect("variant");
            print_shape(
                format!("{path}.{}", en.variant()),
                &variant.shape,
                en.data(),
                registry,
                lines,
            );
        }
        (SlotShape::Option { some, .. }, SlotDataAccess::Option(option)) => {
            lines.push(format!(
                "{path}: option {}",
                if option.data().is_some() {
                    "some"
                } else {
                    "none"
                }
            ));
            if let Some(child) = option.data() {
                print_shape(format!("{path}.some"), some, child, registry, lines);
            }
        }
        _ => panic!("shape/data mismatch"),
    }
}
