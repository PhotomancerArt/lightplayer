//! Authored UTF-8 file-or-inline source slot.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::slot::shape;
use crate::slot_codec::{
    SlotDataWriteError, SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError, SyntaxEventSource,
    ValueReader,
};
use crate::{
    FieldSlot, FieldSlotMut, Revision, SlotCustomAccess, SlotCustomMutAccess, SlotDataAccess,
    SlotDataMutAccess, SlotMapValueAccess, SlotMapValueMutAccess, SlotMeta, SlotRecordAccess,
    SlotRecordMutAccess, SlotShape, SlotShapeId, StaticSlotMeta, StaticSlotShapeDescriptor,
    current_revision,
};

use super::SourcePath;

pub(crate) const SOURCE_FILE_CODEC_ID: SlotShapeId =
    SlotShapeId::from_static_name("lp::slots::SourceFileCodec");

const PATH_KEY: &str = "$path";

/// Backing for an authored source file slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetSlotValue {
    Path(SourcePath),
    Inline { extension: String, text: String },
}

/// Authored file-or-inline UTF-8 source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFileSlot {
    backing: AssetSlotValue,
    revision: Revision,
}

impl Default for SourceFileSlot {
    fn default() -> Self {
        Self {
            backing: AssetSlotValue::Path(SourcePath::from("")),
            revision: Revision::default(),
        }
    }
}

impl SourceFileSlot {
    pub fn from_path(path: impl Into<SourcePath>) -> Self {
        Self {
            backing: AssetSlotValue::Path(path.into()),
            revision: current_revision(),
        }
    }

    pub fn from_inline(extension: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            backing: AssetSlotValue::Inline {
                extension: extension.into(),
                text: text.into(),
            },
            revision: current_revision(),
        }
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn backing(&self) -> &AssetSlotValue {
        &self.backing
    }

    pub fn path_value(&self) -> Option<&SourcePath> {
        match &self.backing {
            AssetSlotValue::Path(path) => Some(path),
            AssetSlotValue::Inline { .. } => None,
        }
    }

    pub fn inline_value(&self) -> Option<(&str, &str)> {
        match &self.backing {
            AssetSlotValue::Inline { extension, text } => Some((extension.as_str(), text.as_str())),
            AssetSlotValue::Path(_) => None,
        }
    }

    pub(crate) fn set_backing(&mut self, backing: AssetSlotValue) {
        self.backing = backing;
        self.revision = current_revision();
    }

    pub(crate) fn read_slot<S>(&mut self, value: ValueReader<'_, '_, S>) -> Result<(), SyntaxError>
    where
        S: SyntaxEventSource,
    {
        let backing = read_backing(value)?;
        self.set_backing(backing);
        Ok(())
    }

    pub(crate) fn write_slot_json<W>(
        &self,
        value: SlotValueWriter<'_, W>,
    ) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        write_backing_json(&self.backing, value)
    }

    pub(crate) fn write_slot_toml(&self) -> Result<toml::Value, SlotDataWriteError> {
        write_backing_toml(&self.backing)
    }
}

fn read_backing<S>(mut value: ValueReader<'_, '_, S>) -> Result<AssetSlotValue, SyntaxError>
where
    S: SyntaxEventSource,
{
    if value.is_string_scalar()? {
        return Ok(AssetSlotValue::Path(SourcePath::from(value.string()?)));
    }

    let mut object = value.object()?;
    let Some(first) = object.peek_prop_name()? else {
        return Err(object.missing_required_field(PATH_KEY));
    };

    if first == PATH_KEY {
        let Some(mut prop) = object.next_prop()? else {
            return Err(object.missing_required_field(PATH_KEY));
        };
        let path = SourcePath::from(prop.value().string()?);
        drop(prop);
        object.finish()?;
        return Ok(AssetSlotValue::Path(path));
    }

    let mut inline_key = None;
    let mut inline_text = None;
    while let Some(mut prop) = object.next_prop()? {
        let name = prop.name().to_string();
        if name == PATH_KEY {
            return Err(prop.unknown_field(&name, &["inline extension key"]));
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
    Ok(AssetSlotValue::Inline {
        extension,
        text: inline_text.unwrap_or_default(),
    })
}

fn write_backing_json<W>(
    backing: &AssetSlotValue,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    match backing {
        AssetSlotValue::Path(path) => value.string(path.as_str()),
        AssetSlotValue::Inline { extension, text } => {
            let mut object = value.object()?;
            object.prop(extension)?.string(text)?;
            object.finish()
        }
    }
}

fn write_backing_toml(backing: &AssetSlotValue) -> Result<toml::Value, SlotDataWriteError> {
    match backing {
        AssetSlotValue::Path(path) => Ok(toml::Value::String(path.as_str().to_string())),
        AssetSlotValue::Inline { extension, text } => {
            let mut table = toml::map::Map::new();
            table.insert(extension.clone(), toml::Value::String(text.clone()));
            Ok(toml::Value::Table(table))
        }
    }
}

impl FieldSlot for SourceFileSlot {
    const STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        Some(&StaticSlotShapeDescriptor::Custom {
            meta: StaticSlotMeta::EMPTY,
            codec: SOURCE_FILE_CODEC_ID,
            shape: &StaticSlotShapeDescriptor::Unit {
                meta: StaticSlotMeta::EMPTY,
            },
            refs: &[],
        });

    fn slot_field_shape() -> SlotShape {
        shape::custom(
            SOURCE_FILE_CODEC_ID,
            SlotShape::Unit {
                meta: SlotMeta::empty(),
            },
            Vec::new(),
        )
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Custom(self)
    }
}

impl FieldSlotMut for SourceFileSlot {
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Custom(self)
    }
}

impl SlotMapValueAccess for SourceFileSlot {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Custom(self)
    }
}

impl SlotMapValueMutAccess for SourceFileSlot {
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Custom(self)
    }
}

impl SlotRecordAccess for SourceFileSlot {
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

impl SlotRecordMutAccess for SourceFileSlot {
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

impl SlotCustomAccess for SourceFileSlot {
    fn custom_codec_id(&self) -> SlotShapeId {
        SOURCE_FILE_CODEC_ID
    }

    fn custom_revision(&self) -> Revision {
        self.revision
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

impl SlotCustomMutAccess for SourceFileSlot {
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SlotShapeRegistry;
    use crate::slot_codec::{SlotReader, TomlSyntaxSource, apply_reader_to_slot};

    fn read_source(text: &str) -> SourceFileSlot {
        let registry = SlotShapeRegistry::default();
        let wrapper = toml::from_str::<toml::Value>(text).expect("toml");
        let value = wrapper.get("source").unwrap_or(&wrapper);
        let mut reader = SlotReader::new(TomlSyntaxSource::new(value).expect("syntax"), &registry);
        let mut slot = SourceFileSlot::default();
        apply_reader_to_slot(
            slot.slot_field_data_mut(),
            &SourceFileSlot::slot_field_shape(),
            &registry,
            reader.value(),
        )
        .expect("read source");
        slot
    }

    #[test]
    fn parses_path_shorthand() {
        let slot = read_source(
            r#"
source = "./shader.glsl"
"#,
        );
        assert_eq!(slot.path_value().unwrap().as_str(), "./shader.glsl");
    }

    #[test]
    fn parses_dollar_path_table() {
        let slot = read_source(
            r#"
"$path" = "./shader.glsl"
"#,
        );
        assert_eq!(slot.path_value().unwrap().as_str(), "./shader.glsl");
    }

    #[test]
    fn parses_inline_glsl_table() {
        let slot = read_source(
            r#"
glsl = "void main() {}"
"#,
        );
        let (ext, text) = slot.inline_value().unwrap();
        assert_eq!(ext, "glsl");
        assert!(text.contains("main"));
    }

    #[test]
    fn round_trips_path_shorthand_toml() {
        let slot = SourceFileSlot::from_path("./a.glsl");
        let value = slot.write_slot_toml().expect("write");
        assert_eq!(value.as_str(), Some("./a.glsl"));
    }
}
