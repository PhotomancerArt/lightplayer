//! Authored artifact-or-inline asset slot.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::slot::shape;
use crate::slot_codec::{
    SlotDataWriteError, SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError, SyntaxEventSource,
    ValueReader,
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
const BYTES_KEY: &str = "bytes";
const EXTENSION_KEY: &str = "extension";

/// Authored asset slot value.
///
/// An artifact reference round-trips as a bare authored path string
/// (`"shader.glsl"`); inline bodies are tables keyed by `text`/`bytes`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum AssetSlotValue {
    /// Asset body lives in another artifact.
    Artifact(ArtifactSpec),
    /// UTF-8 asset body embedded in the owning artifact.
    InlineText {
        extension: Option<String>,
        text: String,
    },
    /// Raw asset bytes embedded in the owning artifact.
    InlineBytes {
        extension: Option<String>,
        bytes: Vec<u8>,
    },
}

// Hand-written, streaming serde for the compact authored form. This is
// deliberately NOT `#[serde(untagged)]`: untagged (like internally-tagged)
// buffers the whole input into serde's `Content` tree and re-parses, which
// monomorphizes the heavy `Content` machinery into the deserialize graph — the
// exact flash cost the externally-tagged-enum work removed. A `Visitor`
// dispatches on the input shape (string vs map) in a single streaming pass.
impl serde::Serialize for AssetSlotValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Artifact(spec) => spec.serialize(serializer),
            Self::InlineText { extension, text } => {
                let mut map = serializer.serialize_map(None)?;
                if let Some(extension) = extension {
                    map.serialize_entry(EXTENSION_KEY, extension)?;
                }
                map.serialize_entry("text", text)?;
                map.end()
            }
            Self::InlineBytes { extension, bytes } => {
                let mut map = serializer.serialize_map(None)?;
                if let Some(extension) = extension {
                    map.serialize_entry(EXTENSION_KEY, extension)?;
                }
                map.serialize_entry(BYTES_KEY, bytes)?;
                map.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for AssetSlotValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AssetSlotValueVisitor;

        impl<'de> serde::de::Visitor<'de> for AssetSlotValueVisitor {
            type Value = AssetSlotValue;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("an artifact path string or an inline asset table")
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
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut extension: Option<Option<String>> = None;
                let mut text: Option<String> = None;
                let mut bytes: Option<Vec<u8>> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        EXTENSION_KEY => extension = Some(map.next_value()?),
                        "text" => text = Some(map.next_value()?),
                        BYTES_KEY => bytes = Some(map.next_value()?),
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                let extension = extension.unwrap_or(None);
                match (text, bytes) {
                    (Some(text), None) => Ok(AssetSlotValue::InlineText { extension, text }),
                    (None, Some(bytes)) => Ok(AssetSlotValue::InlineBytes { extension, bytes }),
                    (None, None) => Err(serde::de::Error::custom(
                        "inline asset table requires a `text` or `bytes` field",
                    )),
                    (Some(_), Some(_)) => Err(serde::de::Error::custom(
                        "inline asset table has both `text` and `bytes`",
                    )),
                }
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

    pub fn inline_text(extension: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            value: AssetSlotValue::InlineText {
                extension: Some(extension.into()),
                text: text.into(),
            },
            revision: current_revision(),
        }
    }

    pub fn inline_bytes(extension: Option<String>, bytes: Vec<u8>) -> Self {
        Self {
            value: AssetSlotValue::InlineBytes { extension, bytes },
            revision: current_revision(),
        }
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
            AssetSlotValue::InlineText { .. } | AssetSlotValue::InlineBytes { .. } => None,
        }
    }

    pub fn inline_text_value(&self) -> Option<(Option<&str>, &str)> {
        match &self.value {
            AssetSlotValue::InlineText { extension, text } => {
                Some((extension.as_deref(), text.as_str()))
            }
            AssetSlotValue::Artifact(_) | AssetSlotValue::InlineBytes { .. } => None,
        }
    }

    pub fn inline_bytes_value(&self) -> Option<(Option<&str>, &[u8])> {
        match &self.value {
            AssetSlotValue::InlineBytes { extension, bytes } => {
                Some((extension.as_deref(), bytes.as_slice()))
            }
            AssetSlotValue::Artifact(_) | AssetSlotValue::InlineText { .. } => None,
        }
    }

    fn snapshot_text(&self) -> String {
        match &self.value {
            AssetSlotValue::Artifact(spec) => spec.to_string(),
            AssetSlotValue::InlineText { extension, text } => match extension {
                Some(extension) => format!("inline text .{extension}: {text}"),
                None => format!("inline text: {text}"),
            },
            AssetSlotValue::InlineBytes { extension, bytes } => match extension {
                Some(extension) => format!("inline bytes .{extension}: {} bytes", bytes.len()),
                None => format!("inline bytes: {} bytes", bytes.len()),
            },
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

    pub(crate) fn write_slot_toml(&self) -> Result<toml::Value, SlotDataWriteError> {
        write_value_toml(&self.value)
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

    if first == BYTES_KEY || first == EXTENSION_KEY {
        return read_inline_bytes(object);
    }

    let mut inline_key = None;
    let mut inline_text = None;
    while let Some(mut prop) = object.next_prop()? {
        let name = prop.name().to_string();
        if name == PATH_KEY || name == LEGACY_PATH_KEY || name == BYTES_KEY || name == EXTENSION_KEY
        {
            return Err(prop.unknown_field(&name, &["inline text extension key"]));
        }
        if inline_key.is_some() {
            return Err(prop.unknown_field(&name, &[inline_key.as_deref().unwrap_or("inline")]));
        }
        inline_key = Some(name);
        inline_text = Some(prop.value().string()?);
    }

    let Some(extension) = inline_key else {
        return Err(object.missing_required_field("inline extension key"));
    };
    Ok(AssetSlotValue::InlineText {
        extension: Some(extension),
        text: inline_text.unwrap_or_default(),
    })
}

fn read_artifact_spec(value: String) -> Result<AssetSlotValue, SyntaxError> {
    ArtifactSpec::parse(&value)
        .map(AssetSlotValue::Artifact)
        .map_err(|err| SyntaxError::new("", None, format!("invalid artifact spec: {err}")))
}

fn read_inline_bytes<S>(
    mut object: crate::slot_codec::ObjectReader<'_, '_, S>,
) -> Result<AssetSlotValue, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut extension = None;
    let mut bytes = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            BYTES_KEY => bytes = Some(read_byte_array(prop.value())?),
            EXTENSION_KEY => extension = Some(prop.value().string()?),
            name => return Err(prop.unknown_field(name, &[BYTES_KEY, EXTENSION_KEY])),
        }
    }

    Ok(AssetSlotValue::InlineBytes {
        extension,
        bytes: bytes.unwrap_or_default(),
    })
}

fn read_byte_array<S>(value: ValueReader<'_, '_, S>) -> Result<Vec<u8>, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut bytes = Vec::new();
    let mut array = value.array()?;
    while let Some(item) = array.next_item()? {
        let byte = item.u32()?;
        let Ok(byte) = u8::try_from(byte) else {
            return Err(SyntaxError::new(
                "",
                None,
                "expected byte value in range 0..=255",
            ));
        };
        bytes.push(byte);
    }
    Ok(bytes)
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
        AssetSlotValue::InlineText { extension, text } => {
            let mut object = writer.object()?;
            object
                .prop(extension.as_deref().unwrap_or("text"))?
                .string(text)?;
            object.finish()
        }
        AssetSlotValue::InlineBytes { extension, bytes } => {
            write_inline_bytes_json(writer, extension.as_deref(), bytes.as_slice())
        }
    }
}

fn write_inline_bytes_json<W>(
    value: SlotValueWriter<'_, W>,
    extension: Option<&str>,
    bytes: &[u8],
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut object = value.object()?;
    if let Some(extension) = extension {
        object.prop(EXTENSION_KEY)?.string(extension)?;
    }
    let mut array = object.prop(BYTES_KEY)?.array()?;
    for byte in bytes {
        array.item()?.u32(u32::from(*byte))?;
    }
    array.finish()?;
    object.finish()
}

fn write_value_toml(value: &AssetSlotValue) -> Result<toml::Value, SlotDataWriteError> {
    match value {
        AssetSlotValue::Artifact(spec) => Ok(toml::Value::String(spec.to_string())),
        AssetSlotValue::InlineText { extension, text } => {
            let mut table = toml::map::Map::new();
            table.insert(
                extension.clone().unwrap_or_else(|| String::from("text")),
                toml::Value::String(text.clone()),
            );
            Ok(toml::Value::Table(table))
        }
        AssetSlotValue::InlineBytes { extension, bytes } => {
            let mut table = toml::map::Map::new();
            if let Some(extension) = extension {
                table.insert(
                    String::from(EXTENSION_KEY),
                    toml::Value::String(extension.clone()),
                );
            }
            table.insert(
                String::from(BYTES_KEY),
                toml::Value::Array(
                    bytes
                        .iter()
                        .map(|byte| toml::Value::Integer(i64::from(*byte)))
                        .collect(),
                ),
            );
            Ok(toml::Value::Table(table))
        }
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
    use crate::slot_codec::{SlotReader, TomlSyntaxSource, apply_reader_to_slot};

    fn read_asset(text: &str) -> AssetSlot {
        let registry = SlotShapeRegistry::default();
        let wrapper = toml::from_str::<toml::Value>(text).expect("toml");
        let value = wrapper.get("source").unwrap_or(&wrapper);
        let mut reader = SlotReader::new(TomlSyntaxSource::new(value).expect("syntax"), &registry);
        let mut slot = AssetSlot::default();
        apply_reader_to_slot(
            slot.slot_field_data_mut(),
            &AssetSlot::slot_field_shape(),
            &registry,
            reader.value(),
        )
        .expect("read asset");
        slot
    }

    #[test]
    fn parses_path_shorthand() {
        let slot = read_asset(
            r#"
source = "./shader.glsl"
"#,
        );
        assert_eq!(slot.artifact_value().unwrap().to_string(), "shader.glsl");
    }

    #[test]
    fn parses_path_table() {
        let slot = read_asset(
            r#"
path = "./shader.glsl"
"#,
        );
        assert_eq!(slot.artifact_value().unwrap().to_string(), "shader.glsl");
    }

    #[test]
    fn parses_legacy_dollar_path_table() {
        let slot = read_asset(
            r#"
"$path" = "./shader.glsl"
"#,
        );
        assert_eq!(slot.artifact_value().unwrap().to_string(), "shader.glsl");
    }

    #[test]
    fn parses_inline_glsl_table() {
        let slot = read_asset(
            r#"
glsl = "void main() {}"
"#,
        );
        let (ext, text) = slot.inline_text_value().unwrap();
        assert_eq!(ext, Some("glsl"));
        assert!(text.contains("main"));
    }

    #[test]
    fn parses_inline_svg_table() {
        let slot = read_asset(
            r#"
svg = "<svg/>"
"#,
        );
        let (ext, text) = slot.inline_text_value().unwrap();
        assert_eq!(ext, Some("svg"));
        assert_eq!(text, "<svg/>");
    }

    #[test]
    fn parses_inline_bytes_table() {
        let slot = read_asset(
            r#"
extension = "png"
bytes = [137, 80, 78, 71]
"#,
        );
        let (ext, bytes) = slot.inline_bytes_value().unwrap();
        assert_eq!(ext, Some("png"));
        assert_eq!(bytes, &[137, 80, 78, 71]);
    }

    #[test]
    fn round_trips_artifact_shorthand_toml() {
        let slot = AssetSlot::path("./a.glsl");
        let value = slot.write_slot_toml().expect("write");
        assert_eq!(value.as_str(), Some("a.glsl"));
    }
}
