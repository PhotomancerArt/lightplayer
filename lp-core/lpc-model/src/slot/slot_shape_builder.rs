//! Small constructors for authored slot shapes.
//!
//! These helpers are intended for static Rust-authored shape definitions and
//! macro-generated shape code. They keep shape declarations readable while the
//! underlying types remain explicit and serializable.

use crate::{
    ModelType, SlotFieldShape, SlotMapKeyShape, SlotMeta, SlotShape, SlotShapeId, SlotValueShape,
    SlotVariantShape,
};
use alloc::boxed::Box;
use alloc::vec::Vec;

/// Parse a static shape id.
///
/// This panics for invalid ids because these ids are authored in Rust source.
pub fn id(value: &str) -> SlotShapeId {
    SlotShapeId::parse(value).expect("valid static slot shape id")
}

/// Build a record shape with empty metadata.
pub fn record(fields: Vec<SlotFieldShape>) -> SlotShape {
    SlotShape::Record {
        meta: SlotMeta::empty(),
        fields,
    }
}

/// Build a map shape with empty metadata.
pub fn map(key: SlotMapKeyShape, value: SlotShape) -> SlotShape {
    SlotShape::Map {
        meta: SlotMeta::empty(),
        key,
        value: Box::new(value),
    }
}

/// Build an option shape with empty metadata.
pub fn option(some: SlotShape) -> SlotShape {
    SlotShape::Option {
        meta: SlotMeta::empty(),
        some: Box::new(some),
    }
}

/// Reference a registered root shape.
pub fn reference(id: SlotShapeId) -> SlotShape {
    SlotShape::reference(id)
}

/// Build one record field.
///
/// This panics for invalid names because these names are authored in Rust
/// source.
pub fn field(name: &str, shape: SlotShape) -> SlotFieldShape {
    SlotFieldShape::new(name, shape).expect("valid static slot field name")
}

/// Build one enum variant.
///
/// This panics for invalid names because these names are authored in Rust
/// source.
pub fn variant(name: &str, shape: SlotShape) -> SlotVariantShape {
    SlotVariantShape::new(name, shape).expect("valid static slot variant name")
}

/// Build a raw atomic value shape.
pub fn value(ty: ModelType) -> SlotShape {
    SlotShape::value(ty)
}

/// Build a semantic atomic value shape.
pub fn leaf(shape: SlotValueShape) -> SlotShape {
    SlotShape::leaf(shape)
}

/// Build a payload-free unit shape.
pub fn unit() -> SlotShape {
    SlotShape::unit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelType, SlotMapKeyShape};
    use alloc::vec;

    #[test]
    fn builders_create_concise_record_shapes() {
        let shape = record(vec![
            field("enabled", value(ModelType::Bool)),
            field("child", option(reference(id("example.child")))),
        ]);

        let SlotShape::Record { fields, .. } = shape else {
            panic!("record shape");
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name.as_str(), "enabled");
    }

    #[test]
    fn builders_create_map_and_enum_shapes() {
        let shape = map(
            SlotMapKeyShape::String,
            SlotShape::Enum {
                meta: SlotMeta::empty(),
                variants: vec![variant("none", unit())],
            },
        );

        let SlotShape::Map { key, value, .. } = shape else {
            panic!("map shape");
        };
        assert_eq!(key, SlotMapKeyShape::String);
        assert!(matches!(*value, SlotShape::Enum { .. }));
    }
}
