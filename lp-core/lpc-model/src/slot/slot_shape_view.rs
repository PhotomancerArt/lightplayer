//! Borrowed shape views over static descriptors or dynamic owned shapes.

use crate::{
    LpType, SlotFieldShape, SlotMapKeyShape, SlotName, SlotPolicy, SlotSemantics, SlotShape,
    SlotShapeId, SlotVariantShape,
};
use alloc::vec::Vec;

use super::{
    StaticSlotFieldShape, StaticSlotShapeDescriptor, StaticSlotValueShape, StaticSlotVariantShape,
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

    pub fn policy(self) -> SlotPolicy {
        match self {
            Self::Static(field) => field.policy,
            Self::Dynamic(field) => field.policy,
        }
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
        policy: SlotPolicy::writable_persisted(),
        default_bind: None,
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
