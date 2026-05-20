use crate::{
    Revision, SlotDataAccess, SlotDataMutAccess, SlotEnumAccess, SlotEnumDefaultVariant,
    SlotEnumMutAccess, SlotEnumShape, SlotMapValueAccess, SlotMapValueMutAccess, SlotMutationError,
    SlotShape, SlotShapeRegistry, SlotVariantShape, WithRevision, current_revision,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{FieldSlot, FieldSlotMut};

/// Revision-tracked slot container for structured enum slots.
///
/// `EnumSlot<T>` owns the revision for "which variant is active". Payload
/// fields inside `T` remain normal slot fields and carry their own revisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumSlot<T> {
    inner: WithRevision<T>,
}

impl<T> EnumSlot<T> {
    pub fn new(value: T) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(revision: Revision, value: T) -> Self {
        Self {
            inner: WithRevision::new(revision, value),
        }
    }

    pub fn variant_revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &T {
        self.inner.value()
    }

    pub fn value_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }

    pub fn into_inner(self) -> T {
        self.inner.into_value()
    }

    pub fn set_with_version(&mut self, revision: Revision, value: T) {
        self.inner.set(revision, value);
    }
}

impl<T: Default> Default for EnumSlot<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Serialize> Serialize for EnumSlot<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for EnumSlot<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(T::deserialize(deserializer)?))
    }
}

#[cfg(feature = "schema-gen")]
impl<T: schemars::JsonSchema> schemars::JsonSchema for EnumSlot<T> {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        <T as schemars::JsonSchema>::schema_name()
    }

    fn schema_id() -> alloc::borrow::Cow<'static, str> {
        <T as schemars::JsonSchema>::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <T as schemars::JsonSchema>::json_schema(generator)
    }
}

/// Revision-free data contract for structured enum slot payloads.
pub trait SlottedEnum {
    fn variant(&self) -> &str;
    fn data(&self) -> SlotDataAccess<'_>;
}

/// Mutable revision-free data contract for structured enum slot payloads.
pub trait SlottedEnumMut: SlottedEnum {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_>;

    fn set_variant_default(&mut self, variant: &str) -> Result<(), SlotMutationError>;

    fn set_variant_default_with_shape(
        &mut self,
        variant: &str,
        registry: &SlotShapeRegistry,
        variants: &[SlotVariantShape],
    ) -> Result<(), SlotMutationError> {
        let _ = (registry, variants);
        self.set_variant_default(variant)
    }
}

impl<T> SlotEnumShape for EnumSlot<T>
where
    T: SlotEnumShape,
{
    fn slot_enum_shape() -> SlotShape {
        T::slot_enum_shape()
    }
}

impl<T> SlotEnumAccess for EnumSlot<T>
where
    T: SlottedEnum,
{
    fn variant_revision(&self) -> Revision {
        self.variant_revision()
    }

    fn variant(&self) -> &str {
        self.inner.value().variant()
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self.inner.value().data() {
            SlotDataAccess::Unit(_) => SlotDataAccess::Unit(self.variant_revision()),
            data => data,
        }
    }
}

impl<T> SlotEnumMutAccess for EnumSlot<T>
where
    T: SlottedEnumMut,
{
    fn variant_revision(&self) -> Revision {
        self.variant_revision()
    }

    fn variant(&self) -> &str {
        self.inner.value().variant()
    }

    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        if matches!(self.inner.get().data(), SlotDataAccess::Unit(_)) {
            SlotDataMutAccess::Unit(self.inner.changed_at_mut())
        } else {
            self.inner.get_mut().data_mut()
        }
    }
}

impl<T> SlotEnumDefaultVariant for EnumSlot<T>
where
    T: SlottedEnumMut,
{
    fn set_variant_default(
        &mut self,
        revision: Revision,
        variant: &str,
    ) -> Result<(), SlotMutationError> {
        self.inner.get_mut().set_variant_default(variant)?;
        self.inner.mark_updated(revision);
        Ok(())
    }

    fn set_variant_default_with_shape(
        &mut self,
        revision: Revision,
        variant: &str,
        registry: &SlotShapeRegistry,
        variants: &[SlotVariantShape],
    ) -> Result<(), SlotMutationError> {
        self.inner
            .get_mut()
            .set_variant_default_with_shape(variant, registry, variants)?;
        self.inner.mark_updated(revision);
        Ok(())
    }
}

impl<T> SlotMapValueAccess for EnumSlot<T>
where
    T: SlottedEnum,
{
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
    }
}

impl<T> SlotMapValueMutAccess for EnumSlot<T>
where
    T: SlottedEnumMut,
{
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Enum(self)
    }
}

impl<T> FieldSlot for EnumSlot<T>
where
    T: SlotEnumShape + SlottedEnum,
{
    fn slot_field_shape() -> SlotShape {
        T::slot_enum_shape()
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
    }
}

impl<T> FieldSlotMut for EnumSlot<T>
where
    T: SlottedEnumMut,
{
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Enum(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SlotMeta, SlotName};
    use alloc::{format, vec};

    #[test]
    fn enum_slot_owns_active_variant_revision() {
        let slot = EnumSlot::with_version(Revision::new(7), TestMode::First(Revision::default()));

        assert_eq!(SlotEnumAccess::variant_revision(&slot), Revision::new(7));
        assert_eq!(SlotEnumAccess::variant(&slot), "first");
        assert_eq!(slot.value(), &TestMode::First(Revision::default()));
    }

    #[test]
    fn enum_slot_switches_default_variant_and_stamps_revision() {
        let mut slot = EnumSlot::new(TestMode::First(Revision::default()));

        SlotEnumDefaultVariant::set_variant_default(&mut slot, Revision::new(42), "second")
            .unwrap();

        assert_eq!(slot.value(), &TestMode::Second(Revision::default()));
        assert_eq!(slot.variant_revision(), Revision::new(42));
        assert!(
            SlotEnumDefaultVariant::set_variant_default(&mut slot, Revision::new(43), "missing")
                .is_err()
        );
    }

    #[test]
    fn enum_slot_exposes_enum_shape_and_data() {
        let slot = EnumSlot::new(TestMode::First(Revision::default()));

        assert!(matches!(
            EnumSlot::<TestMode>::slot_field_shape(),
            SlotShape::Enum { .. }
        ));
        assert!(matches!(slot.slot_field_data(), SlotDataAccess::Enum(_)));
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum TestMode {
        First(Revision),
        Second(Revision),
    }

    impl Default for TestMode {
        fn default() -> Self {
            Self::First(Revision::default())
        }
    }

    impl SlotEnumShape for TestMode {
        fn slot_enum_shape() -> SlotShape {
            SlotShape::Enum {
                meta: SlotMeta::empty(),
                encoding: crate::SlotEnumEncoding::default(),
                variants: vec![
                    SlotVariantShape {
                        name: SlotName::parse("first").unwrap(),
                        shape: SlotShape::Unit {
                            meta: SlotMeta::empty(),
                        },
                    },
                    SlotVariantShape {
                        name: SlotName::parse("second").unwrap(),
                        shape: SlotShape::Unit {
                            meta: SlotMeta::empty(),
                        },
                    },
                ],
            }
        }
    }

    impl SlottedEnum for TestMode {
        fn variant(&self) -> &str {
            match self {
                Self::First(_) => "first",
                Self::Second(_) => "second",
            }
        }

        fn data(&self) -> SlotDataAccess<'_> {
            match self {
                Self::First(revision) | Self::Second(revision) => SlotDataAccess::Unit(*revision),
            }
        }
    }

    impl SlottedEnumMut for TestMode {
        fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
            match self {
                Self::First(revision) | Self::Second(revision) => SlotDataMutAccess::Unit(revision),
            }
        }

        fn set_variant_default(&mut self, variant: &str) -> Result<(), SlotMutationError> {
            *self = match variant {
                "first" => Self::First(Revision::default()),
                "second" => Self::Second(Revision::default()),
                other => {
                    return Err(SlotMutationError::unknown_variant(format!(
                        "unknown variant {other}"
                    )));
                }
            };
            Ok(())
        }
    }
}
