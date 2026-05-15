use crate::{
    LpValue, Revision, SlotAccess, SlotData, SlotEnum, SlotMapDyn, SlotMapKey, SlotOptionDyn,
    SlotRecord, SlotValue, ValueRootError, WithRevision,
};
use alloc::format;
use alloc::string::{String, ToString};

/// Runtime object that exposes mutable slot-addressable data.
pub trait SlotMutAccess: SlotAccess {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_>;
}

/// Field-level mutable slot access used by derive inference.
pub trait FieldSlotMut {
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_>;
}

/// Mutable access to one slot-data node.
pub enum SlotDataMutAccess<'a> {
    Unit(&'a mut Revision),
    Value(&'a mut dyn SlotValueMutAccess),
    Record(&'a mut dyn SlotRecordMutAccess),
    Map(&'a mut dyn MapSlotMutAccess),
    Enum(&'a mut dyn SlotEnumDefaultVariant),
    Option(&'a mut dyn SlotOptionMutAccess),
}

/// Mutable access to an atomic slot value.
pub trait SlotValueMutAccess {
    fn changed_at(&self) -> Revision;

    fn set_lp_value(&mut self, revision: Revision, value: LpValue)
    -> Result<(), SlotMutationError>;
}

/// Mutable access to a record slot.
pub trait SlotRecordMutAccess {
    fn fields_revision(&self) -> Revision {
        Revision::default()
    }

    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>>;
}

/// Mutable access to a stable-key map slot.
pub trait MapSlotMutAccess {
    fn keys_revision(&self) -> Revision;
    fn get_mut(&mut self, key: &SlotMapKey) -> Option<SlotDataMutAccess<'_>>;
}

/// Mutable access to an enum slot with one active variant.
pub trait SlotEnumMutAccess {
    fn variant_revision(&self) -> Revision;
    fn variant(&self) -> &str;
    fn data_mut(&mut self) -> SlotDataMutAccess<'_>;
}

/// Mutable enum access that can switch variants by constructing default payloads.
pub trait SlotEnumDefaultVariant: SlotEnumMutAccess {
    fn set_variant_default(
        &mut self,
        revision: Revision,
        variant: &str,
    ) -> Result<(), SlotMutationError>;
}

/// Mutable access to an optional slot.
pub trait SlotOptionMutAccess {
    fn presence_revision(&self) -> Revision;
    fn data_mut(&mut self) -> Option<SlotDataMutAccess<'_>>;
}

/// A map value that can be exposed through mutable slot traversal.
pub trait SlotMapValueMutAccess {
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_>;
}

/// Error returned while applying a slot mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotMutationError {
    WrongType { message: String },
    UnknownVariant { message: String },
    UnknownPath { message: String },
    UnsupportedTarget { message: String },
}

impl SlotMutationError {
    pub fn wrong_type(message: impl Into<String>) -> Self {
        Self::WrongType {
            message: message.into(),
        }
    }

    pub fn unknown_path(message: impl Into<String>) -> Self {
        Self::UnknownPath {
            message: message.into(),
        }
    }

    pub fn unknown_variant(message: impl Into<String>) -> Self {
        Self::UnknownVariant {
            message: message.into(),
        }
    }

    pub fn unsupported_target(message: impl Into<String>) -> Self {
        Self::UnsupportedTarget {
            message: message.into(),
        }
    }
}

impl core::fmt::Display for SlotMutationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WrongType { message }
            | Self::UnknownVariant { message }
            | Self::UnknownPath { message }
            | Self::UnsupportedTarget { message } => f.write_str(message),
        }
    }
}

impl core::error::Error for SlotMutationError {}

impl From<ValueRootError> for SlotMutationError {
    fn from(error: ValueRootError) -> Self {
        Self::wrong_type(error.to_string())
    }
}

impl SlotData {
    pub fn access_mut(&mut self) -> SlotDataMutAccess<'_> {
        match self {
            Self::Unit { revision } => SlotDataMutAccess::Unit(revision),
            Self::Value(value) => SlotDataMutAccess::Value(value),
            Self::Record(record) => SlotDataMutAccess::Record(record),
            Self::Map(map) => SlotDataMutAccess::Map(map),
            Self::Enum(en) => SlotDataMutAccess::Enum(en),
            Self::Option(option) => SlotDataMutAccess::Option(option),
        }
    }
}

impl SlotValueMutAccess for WithRevision<LpValue> {
    fn changed_at(&self) -> Revision {
        self.changed_at()
    }

    fn set_lp_value(
        &mut self,
        revision: Revision,
        value: LpValue,
    ) -> Result<(), SlotMutationError> {
        self.set(revision, value);
        Ok(())
    }
}

impl<T> SlotValueMutAccess for super::ValueSlot<T>
where
    T: SlotValue,
{
    fn changed_at(&self) -> Revision {
        self.revision()
    }

    fn set_lp_value(
        &mut self,
        revision: Revision,
        value: LpValue,
    ) -> Result<(), SlotMutationError> {
        self.set_with_version(revision, T::from_lp_value(&value)?);
        Ok(())
    }
}

impl<T> FieldSlotMut for super::ValueSlot<T>
where
    T: SlotValue,
{
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Value(self)
    }
}

impl SlotRecordMutAccess for SlotRecord {
    fn fields_revision(&self) -> Revision {
        self.fields_revision
    }

    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> {
        self.fields.get_mut(index).map(SlotData::access_mut)
    }
}

impl MapSlotMutAccess for SlotMapDyn {
    fn keys_revision(&self) -> Revision {
        self.keys_revision
    }

    fn get_mut(&mut self, key: &SlotMapKey) -> Option<SlotDataMutAccess<'_>> {
        self.entries.get_mut(key).map(SlotData::access_mut)
    }
}

impl SlotEnumMutAccess for SlotEnum {
    fn variant_revision(&self) -> Revision {
        self.variant_revision
    }

    fn variant(&self) -> &str {
        self.variant.as_str()
    }

    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        self.data.access_mut()
    }
}

impl SlotEnumDefaultVariant for SlotEnum {
    fn set_variant_default(
        &mut self,
        _revision: Revision,
        variant: &str,
    ) -> Result<(), SlotMutationError> {
        Err(SlotMutationError::unsupported_target(format!(
            "dynamic SlotEnum cannot construct default variant {variant:?} without payload data"
        )))
    }
}

impl SlotOptionMutAccess for SlotOptionDyn {
    fn presence_revision(&self) -> Revision {
        self.presence_revision
    }

    fn data_mut(&mut self) -> Option<SlotDataMutAccess<'_>> {
        self.data.as_mut().map(|data| data.access_mut())
    }
}

impl<T> SlotMapValueMutAccess for T
where
    T: SlotValueMutAccess,
{
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Value(self)
    }
}

impl<K, V> MapSlotMutAccess for super::MapSlot<K, V>
where
    K: super::MapSlotKeyLike,
    V: SlotMapValueMutAccess,
{
    fn keys_revision(&self) -> Revision {
        self.keys_revision
    }

    fn get_mut(&mut self, key: &SlotMapKey) -> Option<SlotDataMutAccess<'_>> {
        let typed_key = K::from_slot_map_key(key)?;
        self.entries
            .get_mut(&typed_key)
            .map(SlotMapValueMutAccess::slot_data_mut)
    }
}

impl<K, V> FieldSlotMut for super::MapSlot<K, V>
where
    K: super::MapSlotKeyLike,
    V: super::FieldSlot + super::SlotMapValueAccess + SlotMapValueMutAccess,
{
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Map(self)
    }
}

impl<T> SlotOptionMutAccess for super::OptionSlot<T>
where
    T: SlotMapValueMutAccess,
{
    fn presence_revision(&self) -> Revision {
        self.presence_revision
    }

    fn data_mut(&mut self) -> Option<SlotDataMutAccess<'_>> {
        self.data.as_mut().map(SlotMapValueMutAccess::slot_data_mut)
    }
}

impl<T> FieldSlotMut for super::OptionSlot<T>
where
    T: super::FieldSlot + super::SlotMapValueAccess + SlotMapValueMutAccess,
{
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Option(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MapSlot, ValueSlot};
    use alloc::collections::BTreeMap;

    #[test]
    fn slot_mut_value_sets_lp_value() {
        let mut value = ValueSlot::with_version(Revision::new(1), 1.0_f32);

        SlotValueMutAccess::set_lp_value(&mut value, Revision::new(2), LpValue::F32(2.5)).unwrap();

        assert_eq!(value.revision(), Revision::new(2));
        assert_eq!(value.value(), &2.5);
    }

    #[test]
    fn slot_mut_value_rejects_wrong_type() {
        let mut value = ValueSlot::with_version(Revision::new(1), 1.0_f32);
        let error =
            SlotValueMutAccess::set_lp_value(&mut value, Revision::new(2), LpValue::Bool(true))
                .unwrap_err();

        assert!(matches!(error, SlotMutationError::WrongType { .. }));
        assert_eq!(value.value(), &1.0);
        assert_eq!(value.revision(), Revision::new(1));
    }

    #[test]
    fn slot_mut_map_accesses_existing_key() {
        let mut map = MapSlot::new(BTreeMap::from([(
            String::from("speed"),
            ValueSlot::with_version(Revision::new(1), 3.0_f32),
        )]));

        let Some(SlotDataMutAccess::Value(value)) =
            MapSlotMutAccess::get_mut(&mut map, &SlotMapKey::String(String::from("speed")))
        else {
            panic!("expected value slot");
        };
        value
            .set_lp_value(Revision::new(2), LpValue::F32(4.0))
            .unwrap();

        assert_eq!(map.entries["speed"].value(), &4.0);
        assert_eq!(map.entries["speed"].revision(), Revision::new(2));
    }

    #[test]
    fn slot_mut_option_accesses_some_only() {
        let mut option = super::super::OptionSlot::some(ValueSlot::new(true));

        assert!(SlotOptionMutAccess::data_mut(&mut option).is_some());

        option.set_none_with_version(Revision::new(3));

        assert!(SlotOptionMutAccess::data_mut(&mut option).is_none());
    }
}
