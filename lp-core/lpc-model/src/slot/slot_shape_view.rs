//! Borrowed shape views over static descriptors or dynamic owned shapes.

use crate::{
    LpType, SlotEnumEncoding, SlotFieldShape, SlotMapKeyShape, SlotName, SlotRole, SlotSemantics,
    SlotShape, SlotShapeId, SlotVariantShape,
};
use alloc::vec::Vec;

use super::{
    StaticSlotEnumEncoding, StaticSlotFieldShape, StaticSlotShapeDescriptor, StaticSlotValueShape,
    StaticSlotVariantShape,
};

/// Borrowed view of a slot shape from either the generated static catalog or
/// the dynamic registry overlay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlotShapeView<'a> {
    Static(&'static StaticSlotShapeDescriptor),
    Dynamic(&'a SlotShape),
}

impl<'a> SlotShapeView<'a> {
    pub fn to_owned_shape(self) -> SlotShape {
        match self {
            Self::Static(shape) => shape.to_owned_shape(),
            Self::Dynamic(shape) => shape.clone(),
        }
    }

    pub fn referenced_shape_ids(self) -> Vec<SlotShapeId> {
        match self {
            Self::Static(shape) => shape.referenced_shape_ids(),
            Self::Dynamic(shape) => shape.referenced_shape_ids(),
        }
    }

    pub fn ref_id(self) -> Option<SlotShapeId> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Ref { id }) => Some(*id),
            Self::Dynamic(SlotShape::Ref { id }) => Some(*id),
            _ => None,
        }
    }

    pub fn is_unit(self) -> bool {
        matches!(
            self,
            Self::Static(StaticSlotShapeDescriptor::Unit { .. })
                | Self::Dynamic(SlotShape::Unit { .. })
        )
    }

    pub fn value_shape(self) -> Option<SlotValueShapeView<'a>> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Value { shape }) => {
                Some(SlotValueShapeView::Static(*shape))
            }
            Self::Dynamic(SlotShape::Value { shape }) => Some(SlotValueShapeView::Dynamic(shape)),
            _ => None,
        }
    }

    pub fn record_field_by_name(self, name: &SlotName) -> Option<(usize, SlotFieldShapeView<'a>)> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Record { fields, .. }) => fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == name.as_str())
                .map(|(index, field)| (index, SlotFieldShapeView::Static(field))),
            Self::Dynamic(SlotShape::Record { fields, .. }) => fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == *name)
                .map(|(index, field)| (index, SlotFieldShapeView::Dynamic(field))),
            _ => None,
        }
    }

    pub fn record_field(self, index: usize) -> Option<SlotFieldShapeView<'a>> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Record { fields, .. }) => {
                fields.get(index).map(SlotFieldShapeView::Static)
            }
            Self::Dynamic(SlotShape::Record { fields, .. }) => {
                fields.get(index).map(SlotFieldShapeView::Dynamic)
            }
            _ => None,
        }
    }

    pub fn record_fields_len(self) -> Option<usize> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Record { fields, .. }) => Some(fields.len()),
            Self::Dynamic(SlotShape::Record { fields, .. }) => Some(fields.len()),
            _ => None,
        }
    }

    pub fn map_key(self) -> Option<SlotMapKeyShape> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Map { key, .. }) => Some(*key),
            Self::Dynamic(SlotShape::Map { key, .. }) => Some(*key),
            _ => None,
        }
    }

    pub fn map_value(self) -> Option<SlotShapeView<'a>> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Map { value, .. }) => {
                Some(SlotShapeView::Static(value))
            }
            Self::Dynamic(SlotShape::Map { value, .. }) => Some(SlotShapeView::Dynamic(value)),
            _ => None,
        }
    }

    pub fn enum_variant(self, index: usize) -> Option<SlotVariantShapeView<'a>> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Enum { variants, .. }) => {
                variants.get(index).map(SlotVariantShapeView::Static)
            }
            Self::Dynamic(SlotShape::Enum { variants, .. }) => {
                variants.get(index).map(SlotVariantShapeView::Dynamic)
            }
            _ => None,
        }
    }

    pub fn is_enum(self) -> bool {
        matches!(
            self,
            Self::Static(StaticSlotShapeDescriptor::Enum { .. })
                | Self::Dynamic(SlotShape::Enum { .. })
        )
    }

    pub fn enum_variant_by_name(self, name: &SlotName) -> Option<SlotVariantShapeView<'a>> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Enum { variants, .. }) => variants
                .iter()
                .find(|variant| variant.name == name.as_str())
                .map(SlotVariantShapeView::Static),
            Self::Dynamic(SlotShape::Enum { variants, .. }) => variants
                .iter()
                .find(|variant| variant.name == *name)
                .map(SlotVariantShapeView::Dynamic),
            _ => None,
        }
    }

    pub fn option_some(self) -> Option<SlotShapeView<'a>> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Option { some, .. }) => {
                Some(SlotShapeView::Static(some))
            }
            Self::Dynamic(SlotShape::Option { some, .. }) => Some(SlotShapeView::Dynamic(some)),
            _ => None,
        }
    }

    pub fn custom_codec(self) -> Option<SlotShapeId> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Custom { codec, .. }) => Some(*codec),
            Self::Dynamic(SlotShape::Custom { codec, .. }) => Some(*codec),
            _ => None,
        }
    }

    pub fn custom_shape(self) -> Option<SlotShapeView<'a>> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Custom { shape, .. }) => {
                Some(SlotShapeView::Static(shape))
            }
            Self::Dynamic(SlotShape::Custom { shape, .. }) => Some(SlotShapeView::Dynamic(shape)),
            _ => None,
        }
    }

    /// A record field by its name as authored text (no [`SlotName`] parse),
    /// with its index.
    pub fn record_field_by_str(self, name: &str) -> Option<(usize, SlotFieldShapeView<'a>)> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Record { fields, .. }) => fields
                .iter()
                .position(|field| field.name == name)
                .map(|index| (index, SlotFieldShapeView::Static(&fields[index]))),
            Self::Dynamic(SlotShape::Record { fields, .. }) => fields
                .iter()
                .position(|field| field.name.as_str() == name)
                .map(|index| (index, SlotFieldShapeView::Dynamic(&fields[index]))),
            _ => None,
        }
    }

    /// The record's field names, in declaration order (empty for any other
    /// shape).
    pub fn record_field_names(self) -> Vec<&'a str> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Record { fields, .. }) => {
                fields.iter().map(|field| field.name).collect()
            }
            Self::Dynamic(SlotShape::Record { fields, .. }) => {
                fields.iter().map(|field| field.name.as_str()).collect()
            }
            _ => Vec::new(),
        }
    }

    /// How the enum spells its active variant, or `None` if this is not an
    /// enum shape.
    pub fn enum_encoding(self) -> Option<SlotEnumEncodingView<'a>> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Enum { encoding, .. }) => {
                Some(match encoding {
                    StaticSlotEnumEncoding::Tagged { field } => SlotEnumEncodingView::Tagged(field),
                    StaticSlotEnumEncoding::External => SlotEnumEncodingView::External,
                })
            }
            Self::Dynamic(SlotShape::Enum { encoding, .. }) => Some(match encoding {
                SlotEnumEncoding::Tagged { field } => SlotEnumEncodingView::Tagged(field.as_str()),
                SlotEnumEncoding::External => SlotEnumEncodingView::External,
            }),
            _ => None,
        }
    }

    /// An enum variant by its name as authored text (no [`SlotName`] parse).
    pub fn enum_variant_by_str(self, name: &str) -> Option<SlotVariantShapeView<'a>> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Enum { variants, .. }) => variants
                .iter()
                .find(|variant| variant.name == name)
                .map(SlotVariantShapeView::Static),
            Self::Dynamic(SlotShape::Enum { variants, .. }) => variants
                .iter()
                .find(|variant| variant.name.as_str() == name)
                .map(SlotVariantShapeView::Dynamic),
            _ => None,
        }
    }

    /// The enum's variant names, in declaration order (empty for any other
    /// shape).
    pub fn enum_variant_names(self) -> Vec<&'a str> {
        match self {
            Self::Static(StaticSlotShapeDescriptor::Enum { variants, .. }) => {
                variants.iter().map(|variant| variant.name).collect()
            }
            Self::Dynamic(SlotShape::Enum { variants, .. }) => variants
                .iter()
                .map(|variant| variant.name.as_str())
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Borrowed view of an enum shape's encoding ([`SlotEnumEncoding`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotEnumEncodingView<'a> {
    /// The active variant's name is in this discriminator field.
    Tagged(&'a str),
    /// The active variant is the enum object's single property.
    External,
}

/// Borrowed view of a value leaf shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlotValueShapeView<'a> {
    Static(StaticSlotValueShape),
    Dynamic(&'a crate::SlotValueShape),
}

impl SlotValueShapeView<'_> {
    pub fn ty_owned(self) -> LpType {
        match self {
            Self::Static(shape) => shape.ty.to_owned_type(),
            Self::Dynamic(shape) => shape.ty.clone(),
        }
    }
}

/// Borrowed view of one record field shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlotFieldShapeView<'a> {
    Static(&'static StaticSlotFieldShape),
    Dynamic(&'a SlotFieldShape),
}

impl<'a> SlotFieldShapeView<'a> {
    pub fn name(self) -> &'static str {
        match self {
            Self::Static(field) => field.name,
            Self::Dynamic(_) => {
                panic!("dynamic slot field names are not static")
            }
        }
    }

    pub fn name_str(self) -> &'a str {
        match self {
            Self::Static(field) => field.name,
            Self::Dynamic(field) => field.name.as_str(),
        }
    }

    pub fn shape(self) -> SlotShapeView<'a> {
        match self {
            Self::Static(field) => SlotShapeView::Static(field.shape),
            Self::Dynamic(field) => SlotShapeView::Dynamic(&field.shape),
        }
    }

    pub fn semantics(self) -> SlotSemantics {
        match self {
            Self::Static(field) => field.semantics,
            Self::Dynamic(field) => field.semantics,
        }
    }

    pub fn role(self) -> SlotRole {
        match self {
            Self::Static(field) => field.role,
            Self::Dynamic(field) => field.role,
        }
    }

    /// The field's declarative default binding endpoint (`bus:<channel>`),
    /// if it declares one ([`SlotFieldShape::default_bind`]).
    pub fn default_bind(self) -> Option<&'a str> {
        match self {
            Self::Static(field) => field.default_bind,
            Self::Dynamic(field) => field.default_bind.as_deref(),
        }
    }

    /// The field's panel-visibility hint for its `default_bind` wiring
    /// ([`crate::PanelHint`]).
    pub fn panel(self) -> Option<crate::PanelHint> {
        match self {
            Self::Static(field) => field.panel,
            Self::Dynamic(field) => field.panel,
        }
    }

    /// Whether a client may request mutation of this field: the role allows
    /// it and the field's own declared direction is not produced (D1 —
    /// direction implies read-only regardless of role).
    pub fn is_writable(self) -> bool {
        crate::slot::effective_writable(self.role(), self.semantics().direction)
    }
}

/// Borrowed view of one enum variant shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlotVariantShapeView<'a> {
    Static(&'static StaticSlotVariantShape),
    Dynamic(&'a SlotVariantShape),
}

impl<'a> SlotVariantShapeView<'a> {
    pub fn name_str(self) -> &'a str {
        match self {
            Self::Static(variant) => variant.name,
            Self::Dynamic(variant) => variant.name.as_str(),
        }
    }

    pub fn shape(self) -> SlotShapeView<'a> {
        match self {
            Self::Static(variant) => SlotShapeView::Static(variant.shape),
            Self::Dynamic(variant) => SlotShapeView::Dynamic(&variant.shape),
        }
    }

    pub fn to_owned_variant(self) -> SlotVariantShape {
        match self {
            Self::Static(variant) => variant.to_owned_variant(),
            Self::Dynamic(variant) => variant.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LpType, SlotDirection, SlotMerge, SlotShape, StaticLpType, StaticSlotMeta,
        StaticSlotShapeDescriptor, StaticSlotValueShape,
    };

    static BOOL_SHAPE: StaticSlotShapeDescriptor = StaticSlotShapeDescriptor::Value {
        shape: StaticSlotValueShape::new(SlotShapeId::new(1), StaticLpType::Bool),
    };
    static FIELDS: [StaticSlotFieldShape; 1] = [StaticSlotFieldShape {
        name: "enabled",
        shape: &BOOL_SHAPE,
        semantics: SlotSemantics::new(SlotDirection::Local, SlotMerge::Latest),
        role: SlotRole::Setting,
        default_bind: None,
        panel: None,
    }];
    static RECORD: StaticSlotShapeDescriptor = StaticSlotShapeDescriptor::Record {
        meta: StaticSlotMeta::EMPTY,
        fields: &FIELDS,
    };

    #[test]
    fn static_view_finds_record_field_by_name() {
        let view = SlotShapeView::Static(&RECORD);
        let name = SlotName::parse("enabled").unwrap();

        let (index, field) = view.record_field_by_name(&name).expect("field");

        assert_eq!(index, 0);
        assert_eq!(field.name_str(), "enabled");
        assert!(field.shape().value_shape().is_some());
    }

    #[test]
    fn dynamic_view_finds_record_field_by_name() {
        let shape = SlotShape::Record {
            meta: crate::SlotMeta::empty(),
            fields: alloc::vec![
                crate::SlotFieldShape::new("enabled", SlotShape::value(LpType::Bool),).unwrap()
            ],
        };
        let view = SlotShapeView::Dynamic(&shape);
        let name = SlotName::parse("enabled").unwrap();

        let (index, field) = view.record_field_by_name(&name).expect("field");

        assert_eq!(index, 0);
        assert_eq!(field.name_str(), "enabled");
        assert!(field.shape().value_shape().is_some());
    }
}
