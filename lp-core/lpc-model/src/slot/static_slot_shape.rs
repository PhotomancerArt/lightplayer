//! Borrowed descriptors for Rust-authored static slot shapes.
//!
//! These descriptors mirror [`crate::SlotShape`] without owning heap-backed
//! collections or strings. Generated static catalogs can store them in read-only
//! memory and expose them through [`crate::SlotShapeView`].

use crate::{
    LpType, ModelEnumVariant, ModelStructMember, OrderedF32, ProductKind, SlotEnumEncoding,
    SlotEnumOption, SlotFieldShape, SlotMapKeyShape, SlotMeta, SlotName, SlotPolicy, SlotSemantics,
    SlotShape, SlotShapeId, SlotValueShape, SlotVariantShape, ValueEditorHint,
};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec::Vec;

/// Borrowed static shape of a slot tree.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StaticSlotShapeDescriptor {
    Ref {
        id: SlotShapeId,
    },
    Unit {
        meta: StaticSlotMeta,
    },
    Value {
        shape: StaticSlotValueShape,
    },
    Record {
        meta: StaticSlotMeta,
        fields: &'static [StaticSlotFieldShape],
    },
    Map {
        meta: StaticSlotMeta,
        key: SlotMapKeyShape,
        value: &'static StaticSlotShapeDescriptor,
    },
    Enum {
        meta: StaticSlotMeta,
        encoding: StaticSlotEnumEncoding,
        variants: &'static [StaticSlotVariantShape],
    },
    Option {
        meta: StaticSlotMeta,
        some: &'static StaticSlotShapeDescriptor,
    },
    Custom {
        meta: StaticSlotMeta,
        codec: SlotShapeId,
        shape: &'static StaticSlotShapeDescriptor,
        refs: &'static [SlotShapeId],
    },
}

impl StaticSlotShapeDescriptor {
    /// Convert one static descriptor tree to the owned shape representation.
    ///
    /// This is intended for tests and explicit dev/debug streaming. Runtime
    /// lookup should prefer borrowed traversal through [`crate::SlotShapeView`].
    pub fn to_owned_shape(self) -> SlotShape {
        match self {
            Self::Ref { id } => SlotShape::Ref { id },
            Self::Unit { meta } => SlotShape::Unit {
                meta: meta.to_owned_meta(),
            },
            Self::Value { shape } => SlotShape::Value {
                shape: shape.to_owned_value_shape(),
            },
            Self::Record { meta, fields } => SlotShape::Record {
                meta: meta.to_owned_meta(),
                fields: fields.iter().map(|field| field.to_owned_field()).collect(),
            },
            Self::Map { meta, key, value } => SlotShape::Map {
                meta: meta.to_owned_meta(),
                key,
                value: Box::new(value.to_owned_shape()),
            },
            Self::Enum {
                meta,
                encoding,
                variants,
            } => SlotShape::Enum {
                meta: meta.to_owned_meta(),
                encoding: encoding.to_owned_encoding(),
                variants: variants
                    .iter()
                    .map(|variant| variant.to_owned_variant())
                    .collect(),
            },
            Self::Option { meta, some } => SlotShape::Option {
                meta: meta.to_owned_meta(),
                some: Box::new(some.to_owned_shape()),
            },
            Self::Custom {
                meta,
                codec,
                shape,
                refs,
            } => SlotShape::Custom {
                meta: meta.to_owned_meta(),
                codec,
                shape: Box::new(shape.to_owned_shape()),
                refs: refs.to_vec(),
            },
        }
    }

    pub fn referenced_shape_ids(self) -> Vec<SlotShapeId> {
        let mut refs = Vec::new();
        self.collect_referenced_shape_ids(&mut refs);
        refs
    }

    fn collect_referenced_shape_ids(self, refs: &mut Vec<SlotShapeId>) {
        match self {
            Self::Ref { id } => refs.push(id),
            Self::Unit { .. } | Self::Value { .. } => {}
            Self::Record { fields, .. } => {
                for field in fields {
                    field.shape.collect_referenced_shape_ids(refs);
                }
            }
            Self::Map { value, .. } => value.collect_referenced_shape_ids(refs),
            Self::Enum { variants, .. } => {
                for variant in variants {
                    variant.shape.collect_referenced_shape_ids(refs);
                }
            }
            Self::Option { some, .. } => some.collect_referenced_shape_ids(refs),
            Self::Custom {
                shape,
                refs: custom_refs,
                ..
            } => {
                shape.collect_referenced_shape_ids(refs);
                refs.extend(custom_refs.iter().copied());
            }
        }
    }
}

/// Borrowed slot presentation metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct StaticSlotMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<&'static str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<&'static str>,
}

impl StaticSlotMeta {
    pub const EMPTY: Self = Self {
        label: None,
        description: None,
    };

    pub const fn empty() -> Self {
        Self::EMPTY
    }

    pub fn to_owned_meta(self) -> SlotMeta {
        SlotMeta {
            label: self.label.map(ToString::to_string),
            description: self.description.map(ToString::to_string),
        }
    }
}

/// Borrowed shape of one complete value payload.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct StaticSlotValueShape {
    pub id: SlotShapeId,
    pub ty: StaticLpType,
    #[serde(default)]
    pub meta: StaticSlotMeta,
    #[serde(default)]
    pub editor: StaticValueEditorHint,
}

impl StaticSlotValueShape {
    pub const fn new(id: SlotShapeId, ty: StaticLpType) -> Self {
        Self {
            id,
            ty,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Plain,
        }
    }

    pub fn to_owned_value_shape(self) -> SlotValueShape {
        SlotValueShape {
            id: self.id,
            ty: self.ty.to_owned_type(),
            meta: self.meta.to_owned_meta(),
            editor: self.editor.to_owned_editor_hint(),
        }
    }
}

/// Borrowed structural value type for static slot value shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticLpType {
    Any,
    String,
    I32,
    U32,
    F32,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    BVec2,
    BVec3,
    BVec4,
    Mat2x2,
    Mat3x3,
    Mat4x4,
    Array(&'static StaticLpType, usize),
    List(&'static StaticLpType),
    Struct {
        name: Option<&'static str>,
        fields: &'static [StaticModelStructMember],
    },
    Enum {
        name: Option<&'static str>,
        variants: &'static [StaticModelEnumVariant],
    },
    Resource,
    Product(ProductKind),
}

impl StaticLpType {
    pub fn to_owned_type(self) -> LpType {
        match self {
            Self::Any => LpType::Any,
            Self::String => LpType::String,
            Self::I32 => LpType::I32,
            Self::U32 => LpType::U32,
            Self::F32 => LpType::F32,
            Self::Bool => LpType::Bool,
            Self::Vec2 => LpType::Vec2,
            Self::Vec3 => LpType::Vec3,
            Self::Vec4 => LpType::Vec4,
            Self::IVec2 => LpType::IVec2,
            Self::IVec3 => LpType::IVec3,
            Self::IVec4 => LpType::IVec4,
            Self::UVec2 => LpType::UVec2,
            Self::UVec3 => LpType::UVec3,
            Self::UVec4 => LpType::UVec4,
            Self::BVec2 => LpType::BVec2,
            Self::BVec3 => LpType::BVec3,
            Self::BVec4 => LpType::BVec4,
            Self::Mat2x2 => LpType::Mat2x2,
            Self::Mat3x3 => LpType::Mat3x3,
            Self::Mat4x4 => LpType::Mat4x4,
            Self::Array(item, len) => LpType::Array(Box::new(item.to_owned_type()), len),
            Self::List(item) => LpType::List(Box::new(item.to_owned_type())),
            Self::Struct { name, fields } => LpType::Struct {
                name: name.map(ToString::to_string),
                fields: fields.iter().map(|field| field.to_owned_member()).collect(),
            },
            Self::Enum { name, variants } => LpType::Enum {
                name: name.map(ToString::to_string),
                variants: variants
                    .iter()
                    .map(|variant| variant.to_owned_variant())
                    .collect(),
            },
            Self::Resource => LpType::Resource,
            Self::Product(kind) => LpType::Product(kind),
        }
    }
}

/// One borrowed field in a static [`StaticLpType::Struct`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StaticModelStructMember {
    pub name: &'static str,
    pub ty: StaticLpType,
}

impl StaticModelStructMember {
    pub fn to_owned_member(self) -> ModelStructMember {
        ModelStructMember {
            name: self.name.to_string(),
            ty: self.ty.to_owned_type(),
        }
    }
}

/// One borrowed variant in a static [`StaticLpType::Enum`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StaticModelEnumVariant {
    pub name: &'static str,
    pub payload: Option<StaticLpType>,
}

impl StaticModelEnumVariant {
    pub fn to_owned_variant(self) -> ModelEnumVariant {
        ModelEnumVariant {
            name: self.name.to_string(),
            payload: self.payload.map(StaticLpType::to_owned_type),
        }
    }
}

/// Borrowed editor hint for static value shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StaticValueEditorHint {
    #[default]
    Plain,
    NodeRef,
    Path,
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<OrderedF32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<OrderedF32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<OrderedF32>,
    },
    Slider {
        min: OrderedF32,
        max: OrderedF32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<OrderedF32>,
    },
    Xy,
    Dimensions,
    Affine2d,
    Resource,
    RuntimeBufferResource,
    VisualProduct,
    ControlProduct,
    Dropdown {
        options: &'static [StaticSlotEnumOption],
    },
}

impl StaticValueEditorHint {
    pub fn to_owned_editor_hint(self) -> ValueEditorHint {
        match self {
            Self::Plain => ValueEditorHint::Plain,
            Self::NodeRef => ValueEditorHint::NodeRef,
            Self::Path => ValueEditorHint::Path,
            Self::Number { min, max, step } => ValueEditorHint::Number { min, max, step },
            Self::Slider { min, max, step } => ValueEditorHint::Slider { min, max, step },
            Self::Xy => ValueEditorHint::Xy,
            Self::Dimensions => ValueEditorHint::Dimensions,
            Self::Affine2d => ValueEditorHint::Affine2d,
            Self::Resource => ValueEditorHint::Resource,
            Self::RuntimeBufferResource => ValueEditorHint::RuntimeBufferResource,
            Self::VisualProduct => ValueEditorHint::VisualProduct,
            Self::ControlProduct => ValueEditorHint::ControlProduct,
            Self::Dropdown { options } => ValueEditorHint::Dropdown {
                options: options
                    .iter()
                    .map(|option| option.to_owned_option())
                    .collect(),
            },
        }
    }
}

/// Borrowed dropdown choice for static value shape editor hints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StaticSlotEnumOption {
    pub value: &'static str,
    pub label: &'static str,
}

impl StaticSlotEnumOption {
    pub fn to_owned_option(self) -> SlotEnumOption {
        SlotEnumOption {
            value: self.value.to_string(),
            label: self.label.to_string(),
        }
    }
}

/// Borrowed enum syntax for a static enum slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StaticSlotEnumEncoding {
    Tagged { field: &'static str },
    External,
}

impl StaticSlotEnumEncoding {
    pub const fn tagged_kind() -> Self {
        Self::Tagged { field: "kind" }
    }

    pub fn to_owned_encoding(self) -> SlotEnumEncoding {
        match self {
            Self::Tagged { field } => SlotEnumEncoding::Tagged {
                field: SlotName::parse(field).expect("valid static enum tag field"),
            },
            Self::External => SlotEnumEncoding::External,
        }
    }
}

impl Default for StaticSlotEnumEncoding {
    fn default() -> Self {
        Self::tagged_kind()
    }
}

/// One borrowed field inside a static record shape.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct StaticSlotFieldShape {
    pub name: &'static str,
    pub shape: &'static StaticSlotShapeDescriptor,
    #[serde(default, skip_serializing_if = "SlotSemantics::is_default")]
    pub semantics: SlotSemantics,
    #[serde(default, skip_serializing_if = "SlotPolicy::is_default")]
    pub policy: SlotPolicy,
}

impl StaticSlotFieldShape {
    pub fn to_owned_field(self) -> SlotFieldShape {
        SlotFieldShape::with_semantics_and_policy(
            self.name,
            self.shape.to_owned_shape(),
            self.semantics,
            self.policy,
        )
        .expect("valid static slot field name")
    }
}

/// One borrowed variant inside a static enum shape.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct StaticSlotVariantShape {
    pub name: &'static str,
    pub shape: &'static StaticSlotShapeDescriptor,
}

impl StaticSlotVariantShape {
    pub fn to_owned_variant(self) -> SlotVariantShape {
        SlotVariantShape::new(self.name, self.shape.to_owned_shape())
            .expect("valid static slot variant name")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SlotDirection, SlotMerge};

    static BOOL_SHAPE: StaticSlotShapeDescriptor = StaticSlotShapeDescriptor::Value {
        shape: StaticSlotValueShape::new(SlotShapeId::new(1), StaticLpType::Bool),
    };
    static RECORD_FIELDS: [StaticSlotFieldShape; 1] = [StaticSlotFieldShape {
        name: "enabled",
        shape: &BOOL_SHAPE,
        semantics: SlotSemantics::new(SlotDirection::Local, SlotMerge::Latest),
        policy: SlotPolicy::writable_persisted(),
    }];
    static RECORD_SHAPE: StaticSlotShapeDescriptor = StaticSlotShapeDescriptor::Record {
        meta: StaticSlotMeta::EMPTY,
        fields: &RECORD_FIELDS,
    };

    #[test]
    fn static_record_descriptor_converts_to_owned_shape() {
        let owned = RECORD_SHAPE.to_owned_shape();

        let SlotShape::Record { fields, .. } = owned else {
            panic!("expected record shape");
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name.as_str(), "enabled");
        assert!(matches!(fields[0].shape, SlotShape::Value { .. }));
    }

    #[test]
    fn static_descriptor_serializes_like_owned_shape() {
        let static_json = serde_json::to_string(&RECORD_SHAPE).unwrap();
        let owned_json = serde_json::to_string(&RECORD_SHAPE.to_owned_shape()).unwrap();

        assert_eq!(static_json, owned_json);
    }

    #[test]
    fn static_refs_are_collected() {
        static REF_SHAPE: StaticSlotShapeDescriptor = StaticSlotShapeDescriptor::Ref {
            id: SlotShapeId::new(99),
        };
        static OPTION_SHAPE: StaticSlotShapeDescriptor = StaticSlotShapeDescriptor::Option {
            meta: StaticSlotMeta::EMPTY,
            some: &REF_SHAPE,
        };

        assert_eq!(OPTION_SHAPE.referenced_shape_ids(), [SlotShapeId::new(99)]);
    }
}
