//! Authored artifact-or-inline asset slot.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::slot::shape;
use crate::slot_codec::{
    SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError, SyntaxEventSource, ValueReader,
};
use crate::{
    ArtifactSpec, FieldSlot, FieldSlotMut, LpPathBuf, LpType, LpValue, Revision, SlotCustomAccess,
    SlotCustomMutAccess, SlotDataAccess, SlotDataMutAccess, SlotMapValueMutAccess, SlotMeta,
    SlotRecordAccess, SlotRecordMutAccess, SlotShape, SlotShapeId, SlotValueAccess, SlotValueShape,
    StaticLpType, StaticSlotMeta, StaticSlotShapeDescriptor, StaticSlotValueShape, ValueEditorHint,
    current_revision,
};

pub(crate) const ASSET_SLOT_CODEC_ID: SlotShapeId =
    SlotShapeId::from_static_name("lp::slots::AssetSlotCodec");
const ASSET_SLOT_SNAPSHOT_SHAPE_ID: SlotShapeId =
    SlotShapeId::from_static_name("lp::slots::AssetSlotSnapshot");

const PATH_KEY: &str = "path";
const LEGACY_PATH_KEY: &str = "$path";

/// Authored asset slot value.
///
/// An artifact reference round-trips as a bare authored path string
/// (`"shader.glsl"`). Asset bodies always live in separate files; inline
/// bodies are not supported.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum AssetSlotValue {
    /// Asset body lives in another artifact.
    Artifact(ArtifactSpec),
}

// Hand-written, streaming serde for the compact authored form. This is
// deliberately NOT `#[serde(untagged)]`: untagged (like internally-tagged)
// buffers the whole input into serde's `Content` tree and re-parses, which
// monomorphizes the heavy `Content` machinery into the deserialize graph — the
// exact flash cost the externally-tagged-enum work removed. A `Visitor`
// dispatches on the input shape (string vs map) in a single streaming pass.
impl serde::Serialize for AssetSlotValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Artifact(spec) => spec.serialize(serializer),
        }
    }
}

impl<'de> serde::Deserialize<'de> for AssetSlotValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AssetSlotValueVisitor;

        impl<'de> serde::de::Visitor<'de> for AssetSlotValueVisitor {
            type Value = AssetSlotValue;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("an artifact path string")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                ArtifactSpec::parse(value)
                    .map(AssetSlotValue::Artifact)
                    .map_err(serde::de::Error::custom)
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                self.visit_str(&value)
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                _map: M,
            ) -> Result<Self::Value, M::Error> {
                Err(serde::de::Error::custom(
                    "inline asset bodies are not supported; reference a separate asset file",
                ))
            }
        }

        deserializer.deserialize_any(AssetSlotValueVisitor)
    }
}

/// Authored artifact-or-inline asset reference.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct AssetSlot {
    value: AssetSlotValue,
    revision: Revision,
}

// Like the other semantic slot wrappers (see `ValueSlot`), an asset slot
// serializes as its bare authored value and stamps the ambient revision on
// deserialize, rather than exposing the internal `revision` field on the wire.
impl serde::Serialize for AssetSlot {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for AssetSlot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            value: AssetSlotValue::deserialize(deserializer)?,
            revision: current_revision(),
        })
    }
}

impl Default for AssetSlot {
    fn default() -> Self {
        Self {
            value: AssetSlotValue::Artifact(ArtifactSpec::path("")),
            revision: Revision::default(),
        }
    }
}

impl AssetSlot {
    pub fn artifact(spec: ArtifactSpec) -> Self {
        Self {
            value: AssetSlotValue::Artifact(spec),
            revision: current_revision(),
        }
    }

    pub fn path(path: impl Into<LpPathBuf>) -> Self {
        Self::artifact(ArtifactSpec::path(path))
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn value(&self) -> &AssetSlotValue {
        &self.value
    }

    pub fn artifact_value(&self) -> Option<&ArtifactSpec> {
        match &self.value {
            AssetSlotValue::Artifact(spec) => Some(spec),
        }
    }

    fn snapshot_text(&self) -> String {
        match &self.value {
            AssetSlotValue::Artifact(spec) => spec.to_string(),
        }
    }

    pub(crate) fn set_value(&mut self, value: AssetSlotValue) {
        self.set_value_with_revision(current_revision(), value);
    }

    pub(crate) fn set_value_with_revision(&mut self, revision: Revision, value: AssetSlotValue) {
        self.value = value;
        self.revision = revision;
    }

    pub(crate) fn set_from_lp_value(
        &mut self,
        revision: Revision,
        value: LpValue,
    ) -> Result<(), String> {
        let LpValue::String(value) = value else {
            return Err(format!(
                "asset slot assignment expects string, got {value:?}"
            ));
        };
        let value = read_artifact_spec(value).map_err(|err| err.to_string())?;
        self.set_value_with_revision(revision, value);
        Ok(())
    }

    pub(crate) fn read_slot<S>(&mut self, value: ValueReader<'_, '_, S>) -> Result<(), SyntaxError>
    where
        S: SyntaxEventSource,
    {
        let value = read_value(value)?;
        self.set_value(value);
        Ok(())
    }

    pub(crate) fn write_slot_json<W>(
        &self,
        value: SlotValueWriter<'_, W>,
    ) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        write_value_json(&self.value, value)
    }
}

fn read_value<S>(mut value: ValueReader<'_, '_, S>) -> Result<AssetSlotValue, SyntaxError>
where
    S: SyntaxEventSource,
{
    if value.is_string_scalar()? {
        return read_artifact_spec(value.string()?);
    }

    let mut object = value.object()?;
    let Some(first) = object.peek_prop_name()? else {
        return Err(object.missing_required_field(PATH_KEY));
    };

    if first == PATH_KEY || first == LEGACY_PATH_KEY {
        let Some(mut prop) = object.next_prop()? else {
            return Err(object.missing_required_field(PATH_KEY));
        };
        let spec = prop.value().string()?;
        drop(prop);
        object.finish()?;
        return read_artifact_spec(spec);
    }

    Err(SyntaxError::new(
        "",
        None,
        "inline asset bodies are not supported; reference a separate asset file",
    ))
}

fn read_artifact_spec(value: String) -> Result<AssetSlotValue, SyntaxError> {
    ArtifactSpec::parse(&value)
        .map(AssetSlotValue::Artifact)
        .map_err(|err| SyntaxError::new("", None, format!("invalid artifact spec: {err}")))
}

fn write_value_json<W>(
    slot_value: &AssetSlotValue,
    writer: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    match slot_value {
        AssetSlotValue::Artifact(spec) => writer.string(&spec.to_string()),
    }
}

impl FieldSlot for AssetSlot {
    const STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        Some(&StaticSlotShapeDescriptor::Custom {
            meta: StaticSlotMeta::EMPTY,
            codec: ASSET_SLOT_CODEC_ID,
            shape: &StaticSlotShapeDescriptor::Value {
                shape: StaticSlotValueShape::new(
                    ASSET_SLOT_SNAPSHOT_SHAPE_ID,
                    StaticLpType::String,
                ),
            },
            refs: &[],
        });

    fn slot_field_shape() -> SlotShape {
        shape::custom(ASSET_SLOT_CODEC_ID, asset_slot_snapshot_shape(), Vec::new())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Custom(self)
    }
}

impl FieldSlotMut for AssetSlot {
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Custom(self)
    }
}

impl SlotMapValueMutAccess for AssetSlot {
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Custom(self)
    }
}

impl SlotRecordAccess for AssetSlot {
    fn fields_revision(&self) -> Revision {
        self.revision
    }

    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        if index == 0 {
            Some(SlotDataAccess::Custom(self))
        } else {
            None
        }
    }
}

impl SlotRecordMutAccess for AssetSlot {
    fn fields_revision(&self) -> Revision {
        self.revision
    }

    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> {
        if index == 0 {
            Some(SlotDataMutAccess::Custom(self))
        } else {
            None
        }
    }
}

impl SlotCustomAccess for AssetSlot {
    fn custom_codec_id(&self) -> SlotShapeId {
        ASSET_SLOT_CODEC_ID
    }

    fn custom_revision(&self) -> Revision {
        self.revision
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

impl SlotCustomMutAccess for AssetSlot {
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

fn asset_slot_snapshot_shape() -> SlotShape {
    SlotShape::Value {
        shape: SlotValueShape {
            id: ASSET_SLOT_SNAPSHOT_SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        },
    }
}

impl SlotValueAccess for AssetSlot {
    fn changed_at(&self) -> Revision {
        self.revision
    }

    fn value(&self) -> LpValue {
        LpValue::String(self.snapshot_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SlotShapeRegistry;
    use crate::slot_codec::{JsonSyntaxSource, SlotReader, SlotWriter, apply_reader_to_slot};

    fn read_asset(text: &str) -> AssetSlot {
        try_read_asset(text).expect("read asset")
    }

    fn try_read_asset(text: &str) -> Result<AssetSlot, crate::slot_codec::SyntaxError> {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(JsonSyntaxSource::new(text).expect("syntax"), &registry);
        let mut slot = AssetSlot::default();
        apply_reader_to_slot(
            slot.slot_field_data_mut(),
            &AssetSlot::slot_field_shape(),
            &registry,
            reader.value(),
        )?;
        Ok(slot)
    }

    #[test]
    fn parses_path_shorthand() {
        let slot = read_asset(r#""./shader.glsl""#);
        assert_eq!(slot.artifact_value().unwrap().to_string(), "shader.glsl");
    }

    #[test]
    fn parses_path_object() {
        let slot = read_asset(r#"{ "path": "./shader.glsl" }"#);
        assert_eq!(slot.artifact_value().unwrap().to_string(), "shader.glsl");
    }

    #[test]
    fn parses_legacy_dollar_path_object() {
        let slot = read_asset(r#"{ "$path": "./shader.glsl" }"#);
        assert_eq!(slot.artifact_value().unwrap().to_string(), "shader.glsl");
    }

    #[test]
    fn rejects_inline_text_object() {
        let err = try_read_asset(r#"{ "glsl": "void main() {}" }"#)
            .expect_err("inline text bodies are not supported");
        assert!(err.to_string().contains("inline asset"), "{err}");
    }

    #[test]
    fn rejects_inline_bytes_object() {
        let err = try_read_asset(r#"{ "extension": "png", "bytes": [137, 80, 78, 71] }"#)
            .expect_err("inline byte bodies are not supported");
        assert!(err.to_string().contains("inline asset"), "{err}");
    }

    #[test]
    fn round_trips_artifact_shorthand_json() {
        let slot = AssetSlot::path("./a.glsl");
        let mut writer = SlotWriter::new(alloc::vec::Vec::new());
        slot.write_slot_json(writer.value()).expect("write");
        let out = writer.into_inner();
        assert_eq!(core::str::from_utf8(&out).unwrap(), r#""a.glsl""#);
    }
}
