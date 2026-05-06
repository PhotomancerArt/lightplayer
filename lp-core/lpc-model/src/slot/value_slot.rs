use crate::{FrameId, ModelValue, SlotMapKeyShape, SlotShape, Versioned, current_state_version};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::{
    FieldSlot, MapSlotAccess, SlotDataAccess, SlotLeaf, SlotMapKey, SlotOptionAccess,
    SlotValueAccess, ToModelValue,
};

/// A typed versioned slot leaf for Rust-authored structs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSlot<T> {
    inner: Versioned<T>,
}

impl<T> ValueSlot<T> {
    pub fn new(value: T) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: T) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: T) {
        self.set_with_version(current_state_version(), value);
    }

    pub fn set_with_version(&mut self, frame: FrameId, value: T) {
        self.inner.set(frame, value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &T {
        self.inner.value()
    }
}

impl<T> From<Versioned<T>> for ValueSlot<T> {
    fn from(inner: Versioned<T>) -> Self {
        Self { inner }
    }
}

impl<T: ToModelValue> SlotValueAccess for ValueSlot<T> {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> ModelValue {
        self.inner.value().to_model_value()
    }
}

impl<T: SlotLeaf> FieldSlot for ValueSlot<T> {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(T::value_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

/// Conversion between typed map keys and generic slot map keys.
pub trait MapSlotKeyLike: Clone + Ord {
    fn key_shape() -> SlotMapKeyShape;
    fn to_slot_map_key(&self) -> SlotMapKey;
    fn from_slot_map_key(key: &SlotMapKey) -> Option<Self>;
}

/// Typed map container for Rust-authored keyed data.
///
/// The key set has its own version because adding or removing entries is a
/// structural change independent from changes inside an entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapSlot<K, V> {
    pub keys_changed_frame: FrameId,
    pub entries: BTreeMap<K, V>,
}

impl<K: Ord, V> MapSlot<K, V> {
    pub fn new(entries: BTreeMap<K, V>) -> Self {
        Self::with_version(current_state_version(), entries)
    }

    pub fn with_version(keys_changed_frame: FrameId, entries: BTreeMap<K, V>) -> Self {
        Self {
            keys_changed_frame,
            entries,
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.insert_with_version(current_state_version(), key, value)
    }

    pub fn insert_with_version(&mut self, frame: FrameId, key: K, value: V) -> Option<V> {
        self.keys_changed_frame = frame;
        self.entries.insert(key, value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.remove_with_version(current_state_version(), key)
    }

    pub fn remove_with_version(&mut self, frame: FrameId, key: &K) -> Option<V> {
        let removed = self.entries.remove(key);
        if removed.is_some() {
            self.keys_changed_frame = frame;
        }
        removed
    }
}

impl<K, V> MapSlotAccess for MapSlot<K, V>
where
    K: MapSlotKeyLike,
    V: SlotMapValueAccess,
{
    fn keys_changed_frame(&self) -> FrameId {
        self.keys_changed_frame
    }

    fn keys(&self) -> Vec<SlotMapKey> {
        self.entries
            .keys()
            .map(MapSlotKeyLike::to_slot_map_key)
            .collect()
    }

    fn get(&self, key: &SlotMapKey) -> Option<SlotDataAccess<'_>> {
        let typed_key = K::from_slot_map_key(key)?;
        self.entries
            .get(&typed_key)
            .map(SlotMapValueAccess::slot_data)
    }
}

impl<K, V> FieldSlot for MapSlot<K, V>
where
    K: MapSlotKeyLike,
    V: FieldSlot + SlotMapValueAccess,
{
    fn slot_field_shape() -> SlotShape {
        SlotShape::Map {
            meta: super::SlotMeta::empty(),
            key: K::key_shape(),
            value: alloc::boxed::Box::new(V::slot_field_shape()),
        }
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Map(self)
    }
}

/// A map value that can be exposed through slot traversal.
pub trait SlotMapValueAccess {
    fn slot_data(&self) -> SlotDataAccess<'_>;
}

impl<T: SlotValueAccess> SlotMapValueAccess for T {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

/// Typed option container for Rust-authored optional records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionSlot<T> {
    pub presence_changed_frame: FrameId,
    pub data: Option<T>,
}

impl<T> OptionSlot<T> {
    pub fn none() -> Self {
        Self::none_with_version(current_state_version())
    }

    pub fn some(data: T) -> Self {
        Self::some_with_version(current_state_version(), data)
    }

    pub fn none_with_version(frame: FrameId) -> Self {
        Self {
            presence_changed_frame: frame,
            data: None,
        }
    }

    pub fn some_with_version(frame: FrameId, data: T) -> Self {
        Self {
            presence_changed_frame: frame,
            data: Some(data),
        }
    }

    pub fn set_some(&mut self, data: T) {
        self.set_some_with_version(current_state_version(), data);
    }

    pub fn set_some_with_version(&mut self, frame: FrameId, data: T) {
        self.presence_changed_frame = frame;
        self.data = Some(data);
    }

    pub fn set_none(&mut self) {
        self.set_none_with_version(current_state_version());
    }

    pub fn set_none_with_version(&mut self, frame: FrameId) {
        self.presence_changed_frame = frame;
        self.data = None;
    }
}

impl<T: SlotMapValueAccess> SlotOptionAccess for OptionSlot<T> {
    fn presence_changed_frame(&self) -> FrameId {
        self.presence_changed_frame
    }

    fn data(&self) -> Option<SlotDataAccess<'_>> {
        self.data.as_ref().map(SlotMapValueAccess::slot_data)
    }
}

impl<T> FieldSlot for OptionSlot<T>
where
    T: FieldSlot + SlotMapValueAccess,
{
    fn slot_field_shape() -> SlotShape {
        SlotShape::Option {
            meta: super::SlotMeta::empty(),
            some: alloc::boxed::Box::new(T::slot_field_shape()),
        }
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Option(self)
    }
}

impl MapSlotKeyLike for String {
    fn key_shape() -> SlotMapKeyShape {
        SlotMapKeyShape::String
    }

    fn to_slot_map_key(&self) -> SlotMapKey {
        SlotMapKey::String(self.clone())
    }

    fn from_slot_map_key(key: &SlotMapKey) -> Option<Self> {
        match key {
            SlotMapKey::String(value) => Some(value.clone()),
            _ => None,
        }
    }
}

impl MapSlotKeyLike for i32 {
    fn key_shape() -> SlotMapKeyShape {
        SlotMapKeyShape::I32
    }

    fn to_slot_map_key(&self) -> SlotMapKey {
        SlotMapKey::I32(*self)
    }

    fn from_slot_map_key(key: &SlotMapKey) -> Option<Self> {
        match key {
            SlotMapKey::I32(value) => Some(*value),
            _ => None,
        }
    }
}

impl MapSlotKeyLike for u32 {
    fn key_shape() -> SlotMapKeyShape {
        SlotMapKeyShape::U32
    }

    fn to_slot_map_key(&self) -> SlotMapKey {
        SlotMapKey::U32(*self)
    }

    fn from_slot_map_key(key: &SlotMapKey) -> Option<Self> {
        match key {
            SlotMapKey::U32(value) => Some(*value),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SlotDataAccess;
    use alloc::vec;

    #[test]
    fn typed_slot_value_exposes_model_value() {
        let value = ValueSlot::with_version(FrameId::new(7), String::from("shader.glsl"));

        assert_eq!(value.changed_frame(), FrameId::new(7));
        assert_eq!(
            SlotValueAccess::value(&value),
            ModelValue::String(String::from("shader.glsl"))
        );
    }

    #[test]
    fn typed_slot_map_tracks_key_changes() {
        struct Entry(ValueSlot<u32>);

        impl SlotMapValueAccess for Entry {
            fn slot_data(&self) -> SlotDataAccess<'_> {
                SlotDataAccess::Value(&self.0)
            }
        }

        let mut map = MapSlot::new(BTreeMap::<String, Entry>::new());
        map.insert_with_version(
            FrameId::new(3),
            String::from("a"),
            Entry(ValueSlot::with_version(FrameId::new(3), 1)),
        );

        assert_eq!(map.keys_changed_frame(), FrameId::new(3));
        assert_eq!(map.keys(), vec![SlotMapKey::String(String::from("a"))]);
    }
}
