//! Parent-owned instruction to instantiate a child node.
//!
//! The parent owns the invocation namespace. The child node definition itself
//! lives under `def`, either as a relative path locator or as an inline
//! [`NodeDef`].

use alloc::boxed::Box;
use alloc::string::{String, ToString};

use crate::artifact::artifact_loc::ArtifactLocator;
use crate::nodes::node_def::{NodeArtifact, NodeDef};
use crate::slot_codec::{
    ObjectReader, SlotDataWriteError, SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError,
    SyntaxEventSource, ValueReader,
};
use crate::{
    ArtifactPath, ArtifactPathSlot, FieldSlot, FieldSlotMut, Revision, SlotAccess,
    SlotCustomAccess, SlotCustomMutAccess, SlotDataAccess, SlotDataMutAccess, SlotMapValueAccess,
    SlotMapValueMutAccess, SlotRecordAccess, SlotRecordMutAccess, SlotShape, SlotShapeId,
    SlotShapeRegistry, SlotValueAccess, StaticSlotFieldShape, StaticSlotMeta, StaticSlotShape,
    StaticSlotShapeDescriptor,
};

pub(crate) const NODE_INVOCATION_CODEC_ID: SlotShapeId =
    SlotShapeId::from_static_name("lp::node::NodeInvocationCodec");

/// Parent-owned child node invocation.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeInvocation {
    pub def: NodeDefRef,
    def_slot: ArtifactPathSlot,
}

/// Authored child node definition reference.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeDefRef {
    Path(ArtifactLocator),
    Inline(Box<NodeDef>),
}

impl NodeInvocation {
    /// New path-backed invocation.
    #[must_use]
    pub fn new(def: ArtifactLocator) -> Self {
        Self::path(def)
    }

    #[must_use]
    pub fn path(def: ArtifactLocator) -> Self {
        let def_slot = ArtifactPathSlot::new(ArtifactPath(def.to_string()));
        Self {
            def: NodeDefRef::Path(def),
            def_slot,
        }
    }

    #[must_use]
    pub fn inline(def: NodeDef) -> Self {
        Self {
            def: NodeDefRef::Inline(Box::new(def)),
            def_slot: ArtifactPathSlot::new(ArtifactPath(String::new())),
        }
    }

    pub fn def_locator(&self) -> Option<&ArtifactLocator> {
        match &self.def {
            NodeDefRef::Path(locator) => Some(locator),
            NodeDefRef::Inline(_) => None,
        }
    }

    pub fn inline_def(&self) -> Option<&NodeDef> {
        match &self.def {
            NodeDefRef::Path(_) => None,
            NodeDefRef::Inline(def) => Some(def),
        }
    }

    pub(crate) fn read_invocation_slot<S>(
        &mut self,
        registry: &SlotShapeRegistry,
        value: ValueReader<'_, '_, S>,
    ) -> Result<(), SyntaxError>
    where
        S: SyntaxEventSource,
    {
        let mut object = value.object()?;
        let Some(mut prop) = object.next_prop()? else {
            return Err(object.missing_required_field("def"));
        };
        let name = prop.name().to_string();
        if name != "def" {
            return Err(prop.unknown_field(&name, &["def"]));
        }
        self.read_def_slot(registry, prop.value())?;
        drop(prop);

        if let Some(prop) = object.next_prop()? {
            let name = prop.name().to_string();
            return Err(prop.unknown_field(&name, &[]));
        }
        Ok(())
    }

    pub(crate) fn write_invocation_slot_json<W>(
        &self,
        registry: &SlotShapeRegistry,
        value: SlotValueWriter<'_, W>,
    ) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        let mut object = value.object()?;
        self.write_def_slot_json(registry, object.prop("def")?)?;
        object.finish()
    }

    pub(crate) fn write_invocation_slot_toml(
        &self,
        registry: &SlotShapeRegistry,
    ) -> Result<toml::Value, SlotDataWriteError> {
        let mut table = toml::Table::new();
        table.insert("def".into(), self.write_def_slot_toml(registry)?);
        Ok(toml::Value::Table(table))
    }

    pub(crate) fn read_def_slot<S>(
        &mut self,
        registry: &SlotShapeRegistry,
        value: ValueReader<'_, '_, S>,
    ) -> Result<(), SyntaxError>
    where
        S: SyntaxEventSource,
    {
        let mut object = value.object()?;
        let Some(first) = object.peek_prop_name()? else {
            return Err(object.missing_required_field("path"));
        };

        match first.as_str() {
            "path" => self.read_path_def(object),
            "kind" => {
                let artifact = read_node_artifact_from_object(registry, object)?;
                *self = Self::inline(artifact.into_node_def());
                Ok(())
            }
            _ => Err(SyntaxError::new(
                "def",
                None,
                "node def reference must contain `path` or inline `kind`",
            )),
        }
    }

    pub(crate) fn write_def_slot_json<W>(
        &self,
        registry: &SlotShapeRegistry,
        value: SlotValueWriter<'_, W>,
    ) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        match &self.def {
            NodeDefRef::Path(locator) => {
                let path = locator.to_string();
                let mut object = value.object()?;
                object.prop("path")?.string(&path)?;
                object.finish()
            }
            NodeDefRef::Inline(def) => {
                let artifact = NodeArtifact::new((**def).clone());
                registry.write_slot_json_value(NodeArtifact::SHAPE_ID, artifact.data(), value)
            }
        }
    }

    pub(crate) fn write_def_slot_toml(
        &self,
        registry: &SlotShapeRegistry,
    ) -> Result<toml::Value, SlotDataWriteError> {
        match &self.def {
            NodeDefRef::Path(locator) => {
                let mut table = toml::Table::new();
                table.insert("path".into(), toml::Value::String(locator.to_string()));
                Ok(toml::Value::Table(table))
            }
            NodeDefRef::Inline(def) => {
                let artifact = NodeArtifact::new((**def).clone());
                registry.write_slot_toml(&artifact)
            }
        }
    }

    fn read_path_def<S>(&mut self, mut object: ObjectReader<'_, '_, S>) -> Result<(), SyntaxError>
    where
        S: SyntaxEventSource,
    {
        let Some(mut prop) = object.next_prop()? else {
            return Err(object.missing_required_field("path"));
        };
        let path = prop.value().string()?;
        drop(prop);

        if object.next_prop()?.is_some() {
            return Err(SyntaxError::new(
                "def",
                None,
                "`def.path` cannot be combined with inline node definition fields",
            ));
        }

        let locator = ArtifactLocator::parse(&path)
            .map_err(|error| SyntaxError::new("def.path", None, alloc::format!("{error}")))?;
        *self = Self::path(locator);
        Ok(())
    }
}

impl Default for NodeInvocation {
    fn default() -> Self {
        Self::path(ArtifactLocator::path(""))
    }
}

impl From<NodeDefRef> for NodeInvocation {
    fn from(def: NodeDefRef) -> Self {
        match def {
            NodeDefRef::Path(locator) => Self::path(locator),
            NodeDefRef::Inline(def) => Self::inline(*def),
        }
    }
}

impl FieldSlot for NodeInvocation {
    const STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        match <ArtifactPathSlot as FieldSlot>::STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR {
            Some(def_shape) => Some(&StaticSlotShapeDescriptor::Custom {
                meta: StaticSlotMeta::EMPTY,
                codec: NODE_INVOCATION_CODEC_ID,
                shape: &StaticSlotShapeDescriptor::Record {
                    meta: StaticSlotMeta::EMPTY,
                    fields: &[StaticSlotFieldShape {
                        name: "def",
                        shape: def_shape,
                        semantics: crate::SlotSemantics::local(),
                        policy: crate::SlotPolicy::writable_persisted(),
                    }],
                },
                refs: &[NodeArtifact::SHAPE_ID],
            }),
            None => None,
        };

    fn slot_field_shape() -> SlotShape {
        crate::slot::shape::custom(
            NODE_INVOCATION_CODEC_ID,
            node_invocation_sync_shape(),
            alloc::vec![NodeArtifact::SHAPE_ID],
        )
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Custom(self)
    }
}

impl FieldSlotMut for NodeInvocation {
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Custom(self)
    }
}

impl SlotMapValueAccess for NodeInvocation {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Custom(self)
    }
}

impl SlotMapValueMutAccess for NodeInvocation {
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Custom(self)
    }
}

impl SlotRecordAccess for NodeInvocation {
    fn fields_revision(&self) -> Revision {
        self.def_slot.changed_at()
    }

    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(self.def_slot.slot_field_data()),
            _ => None,
        }
    }
}

impl SlotRecordMutAccess for NodeInvocation {
    fn fields_revision(&self) -> Revision {
        self.def_slot.changed_at()
    }

    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> {
        match index {
            0 => Some(self.def_slot.slot_field_data_mut()),
            _ => None,
        }
    }
}

impl SlotCustomAccess for NodeInvocation {
    fn custom_codec_id(&self) -> SlotShapeId {
        NODE_INVOCATION_CODEC_ID
    }

    fn custom_revision(&self) -> Revision {
        self.def_slot.changed_at()
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

impl SlotCustomMutAccess for NodeInvocation {
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

fn node_invocation_sync_shape() -> SlotShape {
    crate::slot::shape::record(alloc::vec![crate::slot::shape::field(
        "def",
        <ArtifactPathSlot as FieldSlot>::slot_field_shape(),
    )])
}

fn read_node_artifact_from_object<S>(
    registry: &SlotShapeRegistry,
    object: ObjectReader<'_, '_, S>,
) -> Result<NodeArtifact, SyntaxError>
where
    S: SyntaxEventSource,
{
    let object =
        crate::slot_codec::read_dynamic_slot_from_object(registry, NodeArtifact::SHAPE_ID, object)?;
    object
        .into_any()
        .downcast::<NodeArtifact>()
        .map(|artifact| *artifact)
        .map_err(|_| {
            SyntaxError::new(
                "",
                None,
                alloc::format!(
                    "slot reader returned unexpected type for shape {}",
                    NodeArtifact::SHAPE_ID
                ),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_invocation_toml_path_form_loads() {
        let invocation = read_invocation(
            r#"
def = { path = "./texture.toml" }
"#,
        );

        assert_eq!(
            invocation.def_locator().unwrap(),
            &ArtifactLocator::path("./texture.toml")
        );
    }

    #[test]
    fn node_invocation_rejects_legacy_artifact() {
        let err = read_invocation_err(
            r#"
artifact = "./texture.toml"
"#,
        );

        assert!(err.to_string().contains("def"));
    }

    #[test]
    fn node_invocation_toml_inline_form_loads() {
        let invocation = read_invocation(
            r#"
[def]
kind = "Clock"
"#,
        );

        assert!(matches!(invocation.inline_def(), Some(NodeDef::Clock(_))));
    }

    #[test]
    fn node_invocation_rejects_path_plus_inline_fields() {
        let err = read_invocation_err(
            r#"
[def]
path = "./clock.toml"
kind = "Clock"
"#,
        );

        assert!(err.to_string().contains("path"));
    }

    fn read_invocation(text: &str) -> NodeInvocation {
        read_invocation_result(text).unwrap()
    }

    fn read_invocation_err(text: &str) -> SyntaxError {
        read_invocation_result(text).unwrap_err()
    }

    fn read_invocation_result(text: &str) -> Result<NodeInvocation, SyntaxError> {
        let mut registry = SlotShapeRegistry::default();
        crate::slot_shapes::register_all_static_slot_shapes(&mut registry).expect("shapes");
        let value = toml::from_str::<toml::Value>(text).unwrap();
        let mut reader = crate::slot_codec::SlotReader::new(
            crate::slot_codec::TomlSyntaxSource::new(&value).unwrap(),
            &registry,
        );
        let mut invocation = NodeInvocation::default();
        crate::slot_codec::apply_reader_to_slot(
            invocation.slot_field_data_mut(),
            &NodeInvocation::slot_field_shape(),
            &registry,
            reader.value(),
        )?;
        Ok(invocation)
    }
}
