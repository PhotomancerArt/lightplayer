use crate::{LpValue, Revision, SlotMapKeyShape, SlotShape, WithRevision, current_revision};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{MapAccess, Visitor},
    ser::SerializeMap,
};

use super::{
    FieldSlot, FieldSlotMut, MapSlotAccess, MapSlotAccessMut, SlotDataAccess, SlotDataAccessMut,
    SlotMapKey, SlotOptionAccess, SlotOptionAccessMut, SlotValue, SlotValueAccess, SlotValueMut,
    ToLpValue,
};

/// A typed revision-tracked slot leaf for Rust-authored structs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSlot<T> {
    inner: WithRevision<T>,
}

impl<T> ValueSlot<T> {
    pub fn new(value: T) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(revision: Revision, value: T) -> Self {
        Self {
            inner: WithRevision::new(revision, value),
        }
    }

    pub fn set(&mut self, value: T) {
        self.set_with_version(current_revision(), value);
    }

    pub fn set_with_version(&mut self, revision: Revision, value: T) {
        self.inner.set(revision, value);
    }

    pub fn revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &T {
        self.inner.value()
    }
}

impl<T> From<WithRevision<T>> for ValueSlot<T> {
    fn from(inner: WithRevision<T>) -> Self {
        Self { inner }
    }
}

impl<T: Default> Default for ValueSlot<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Serialize> Serialize for ValueSlot<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

#[cfg(feature = "schema-gen")]
impl<T: schemars::JsonSchema> schemars::JsonSchema for ValueSlot<T> {
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

impl<'de, T: Deserialize<'de>> Deserialize<'de> for ValueSlot<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(T::deserialize(deserializer)?))
    }
}

impl<T: ToLpValue> SlotValueAccess for ValueSlot<T> {
    fn changed_at(&self) -> Revision {
        self.inner.changed_at()
    }

    fn value(&self) -> LpValue {
        self.inner.value().to_lp_value()
    }
}

impl<T: crate::FromLpValue> SlotValueMut for ValueSlot<T> {
    fn set_lp_value(
        &mut self,
        revision: Revision,
        value: LpValue,
    ) -> Result<(), crate::ValueRootError> {
        self.set_with_version(revision, T::from_lp_value(&value)?);
        Ok(())
    }
}

impl<T: SlotValue> FieldSlot for ValueSlot<T> {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(T::value_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl<T: SlotValue> FieldSlotMut for ValueSlot<T>
where
    Self: SlotValueMut,
{
    fn slot_field_data_mut(&mut self) -> SlotDataAccessMut<'_> {
        SlotDataAccessMut::Value(self)
    }
}

/// Conversion between typed map keys and generic slot map keys.
pub trait MapSlotKeyLike: Clone + Ord {
    fn key_shape() -> SlotMapKeyShape;
    fn to_authored_key(&self) -> String;
    fn from_authored_key(key: &str) -> Result<Self, String>;
    fn to_slot_map_key(&self) -> SlotMapKey;
    fn from_slot_map_key(key: &SlotMapKey) -> Option<Self>;
}

/// Typed map container for Rust-authored keyed data.
///
/// The key set has its own version because adding or removing entries is a
/// structural change independent from changes inside an entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapSlot<K, V> {
    pub keys_revision: Revision,
    pub entries: BTreeMap<K, V>,
}

impl<K: Ord, V> MapSlot<K, V> {
    pub fn new(entries: BTreeMap<K, V>) -> Self {
        Self::with_version(current_revision(), entries)
    }

    pub fn with_version(keys_revision: Revision, entries: BTreeMap<K, V>) -> Self {
        Self {
            keys_revision,
            entries,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.insert_with_version(current_revision(), key, value)
    }

    pub fn insert_with_version(&mut self, revision: Revision, key: K, value: V) -> Option<V> {
        self.keys_revision = revision;
        self.entries.insert(key, value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.remove_with_version(current_revision(), key)
    }

    pub fn remove_with_version(&mut self, revision: Revision, key: &K) -> Option<V> {
        let removed = self.entries.remove(key);
        if removed.is_some() {
            self.keys_revision = revision;
        }
        removed
    }
}

impl<K: Ord, V> Default for MapSlot<K, V> {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

impl<K, V> Serialize for MapSlot<K, V>
where
    K: MapSlotKeyLike,
    V: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(&key.to_authored_key(), value)?;
        }
        map.end()
    }
}

#[cfg(feature = "schema-gen")]
impl<K, V> schemars::JsonSchema for MapSlot<K, V>
where
    V: schemars::JsonSchema,
{
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        <BTreeMap<String, V> as schemars::JsonSchema>::schema_name()
    }

    fn schema_id() -> alloc::borrow::Cow<'static, str> {
        <BTreeMap<String, V> as schemars::JsonSchema>::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <BTreeMap<String, V> as schemars::JsonSchema>::json_schema(generator)
    }
}

impl<'de, K, V> Deserialize<'de> for MapSlot<K, V>
where
    K: MapSlotKeyLike,
    V: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MapSlotVisitor<K, V> {
            marker: core::marker::PhantomData<(K, V)>,
        }

        impl<'de, K, V> Visitor<'de> for MapSlotVisitor<K, V>
        where
            K: MapSlotKeyLike,
            V: Deserialize<'de>,
        {
            type Value = MapSlot<K, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a slot map")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut entries = BTreeMap::new();
                while let Some((key, value)) = access.next_entry::<String, V>()? {
                    let key = K::from_authored_key(&key).map_err(serde::de::Error::custom)?;
                    entries.insert(key, value);
                }
                Ok(MapSlot::new(entries))
            }
        }

        deserializer.deserialize_map(MapSlotVisitor {
            marker: core::marker::PhantomData,
        })
    }
}

impl<K, V> MapSlotAccess for MapSlot<K, V>
where
    K: MapSlotKeyLike,
    V: SlotMapValueAccess,
{
    fn keys_revision(&self) -> Revision {
        self.keys_revision
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

impl<K, V> MapSlotAccessMut for MapSlot<K, V>
where
    K: MapSlotKeyLike,
    V: SlotMapValueAccessMut,
{
    fn get_mut(&mut self, key: &SlotMapKey) -> Option<SlotDataAccessMut<'_>> {
        let typed_key = K::from_slot_map_key(key)?;
        self.entries
            .get_mut(&typed_key)
            .map(SlotMapValueAccessMut::slot_data_mut)
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

impl<K, V> FieldSlotMut for MapSlot<K, V>
where
    K: MapSlotKeyLike,
    V: FieldSlot + SlotMapValueAccess + SlotMapValueAccessMut,
{
    fn slot_field_data_mut(&mut self) -> SlotDataAccessMut<'_> {
        SlotDataAccessMut::Map(self)
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

/// A map value that can be mutated through slot traversal.
pub trait SlotMapValueAccessMut {
    fn slot_data_mut(&mut self) -> SlotDataAccessMut<'_>;
}

impl<T: SlotValueMut> SlotMapValueAccessMut for T {
    fn slot_data_mut(&mut self) -> SlotDataAccessMut<'_> {
        SlotDataAccessMut::Value(self)
    }
}

/// Typed option container for Rust-authored optional records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionSlot<T> {
    pub presence_revision: Revision,
    pub data: Option<T>,
}

impl<T> OptionSlot<T> {
    pub fn none() -> Self {
        Self::none_with_version(current_revision())
    }

    pub fn some(data: T) -> Self {
        Self::some_with_version(current_revision(), data)
    }

    pub fn none_with_version(revision: Revision) -> Self {
        Self {
            presence_revision: revision,
            data: None,
        }
    }

    pub fn some_with_version(revision: Revision, data: T) -> Self {
        Self {
            presence_revision: revision,
            data: Some(data),
        }
    }

    pub fn set_some(&mut self, data: T) {
        self.set_some_with_version(current_revision(), data);
    }

    pub fn set_some_with_version(&mut self, revision: Revision, data: T) {
        self.presence_revision = revision;
        self.data = Some(data);
    }

    pub fn set_none(&mut self) {
        self.set_none_with_version(current_revision());
    }

    pub fn set_none_with_version(&mut self, revision: Revision) {
        self.presence_revision = revision;
        self.data = None;
    }

    pub fn is_none(&self) -> bool {
        self.data.is_none()
    }
}

impl<T> Default for OptionSlot<T> {
    fn default() -> Self {
        Self::none()
    }
}

impl<T: Serialize> Serialize for OptionSlot<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.data.serialize(serializer)
    }
}

#[cfg(feature = "schema-gen")]
impl<T: schemars::JsonSchema> schemars::JsonSchema for OptionSlot<T> {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        <Option<T> as schemars::JsonSchema>::schema_name()
    }

    fn schema_id() -> alloc::borrow::Cow<'static, str> {
        <Option<T> as schemars::JsonSchema>::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <Option<T> as schemars::JsonSchema>::json_schema(generator)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for OptionSlot<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self {
            presence_revision: current_revision(),
            data: Option::<T>::deserialize(deserializer)?,
        })
    }
}

impl<T: SlotMapValueAccess> SlotOptionAccess for OptionSlot<T> {
    fn presence_revision(&self) -> Revision {
        self.presence_revision
    }

    fn data(&self) -> Option<SlotDataAccess<'_>> {
        self.data.as_ref().map(SlotMapValueAccess::slot_data)
    }
}

impl<T: SlotMapValueAccessMut> SlotOptionAccessMut for OptionSlot<T> {
    fn data_mut(&mut self) -> Option<SlotDataAccessMut<'_>> {
        self.data.as_mut().map(SlotMapValueAccessMut::slot_data_mut)
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

impl<T> FieldSlotMut for OptionSlot<T>
where
    T: FieldSlot + SlotMapValueAccess + SlotMapValueAccessMut,
{
    fn slot_field_data_mut(&mut self) -> SlotDataAccessMut<'_> {
        SlotDataAccessMut::Option(self)
    }
}

impl MapSlotKeyLike for String {
    fn key_shape() -> SlotMapKeyShape {
        SlotMapKeyShape::String
    }

    fn to_authored_key(&self) -> String {
        self.clone()
    }

    fn from_authored_key(key: &str) -> Result<Self, String> {
        Ok(String::from(key))
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

    fn to_authored_key(&self) -> String {
        self.to_string()
    }

    fn from_authored_key(key: &str) -> Result<Self, String> {
        key.parse()
            .map_err(|_| format!("expected i32 map key, got {key:?}"))
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

    fn to_authored_key(&self) -> String {
        self.to_string()
    }

    fn from_authored_key(key: &str) -> Result<Self, String> {
        key.parse()
            .map_err(|_| format!("expected u32 map key, got {key:?}"))
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
    use crate::{SlotDataAccess, current_revision};
    use alloc::vec;

    #[test]
    fn typed_slot_value_exposes_lp_value() {
        let value = ValueSlot::with_version(Revision::new(7), String::from("shader.glsl"));

        assert_eq!(value.revision(), Revision::new(7));
        assert_eq!(
            SlotValueAccess::value(&value),
            LpValue::String(String::from("shader.glsl"))
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
            Revision::new(3),
            String::from("a"),
            Entry(ValueSlot::with_version(Revision::new(3), 1)),
        );

        assert_eq!(map.keys_revision(), Revision::new(3));
        assert_eq!(map.keys(), vec![SlotMapKey::String(String::from("a"))]);
    }

    #[test]
    fn value_slot_serializes_as_authored_value_and_stamps_deserialize_version() {
        let value = ValueSlot::with_version(Revision::new(4), String::from("shader.glsl"));

        assert_eq!(serde_json::to_string(&value).unwrap(), r#""shader.glsl""#);

        let expected_version = current_revision();
        let decoded: ValueSlot<String> = serde_json::from_str(r#""main.glsl""#).unwrap();

        assert_eq!(decoded.value(), "main.glsl");
        assert_eq!(decoded.revision(), expected_version);
    }

    #[test]
    fn map_slot_serializes_as_authored_map_and_stamps_key_version() {
        let mut entries = BTreeMap::new();
        entries.insert(
            String::from("speed"),
            ValueSlot::with_version(Revision::new(2), 7_u32),
        );
        let map = MapSlot::with_version(Revision::new(3), entries);

        assert_eq!(serde_json::to_string(&map).unwrap(), r#"{"speed":7}"#);

        let expected_version = current_revision();
        let decoded: MapSlot<String, ValueSlot<u32>> =
            serde_json::from_str(r#"{"phase":3}"#).unwrap();

        assert_eq!(decoded.keys_revision(), expected_version);
        assert_eq!(decoded.entries["phase"].value(), &3);
        assert_eq!(decoded.entries["phase"].revision(), expected_version);
    }

    #[test]
    fn map_slot_round_trips_numeric_authored_keys() {
        let mut entries = BTreeMap::new();
        entries.insert(7_u32, ValueSlot::new(String::from("seven")));
        let map = MapSlot::new(entries);

        let encoded = toml::to_string(&map).unwrap();
        assert!(encoded.contains("7 = \"seven\""));

        let decoded: MapSlot<u32, ValueSlot<String>> = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded.entries[&7].value(), "seven");
    }

    #[test]
    fn option_slot_serializes_as_authored_option_and_stamps_presence_version() {
        let some = OptionSlot::some_with_version(Revision::new(3), ValueSlot::new(5_u32));
        let none = OptionSlot::<ValueSlot<u32>>::none_with_version(Revision::new(4));

        assert_eq!(serde_json::to_string(&some).unwrap(), "5");
        assert_eq!(serde_json::to_string(&none).unwrap(), "null");

        let expected_version = current_revision();
        let decoded: OptionSlot<ValueSlot<u32>> = serde_json::from_str("6").unwrap();

        assert_eq!(decoded.presence_revision(), expected_version);
        assert_eq!(decoded.data.as_ref().unwrap().value(), &6);
        assert_eq!(decoded.data.as_ref().unwrap().revision(), expected_version);
    }
}
