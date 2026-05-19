//! Parent-owned instruction to instantiate a child node.
//!
//! The parent owns the invocation namespace. The child node definition itself
//! lives under `def`, either as a relative path locator or as an inline
//! [`NodeDef`].

use alloc::boxed::Box;
use alloc::string::{String, ToString};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::artifact::artifact_loc::ArtifactLocator;
use crate::nodes::node_def::NodeDef;
use crate::{
    ArtifactPath, ArtifactPathSlot, FieldSlot, FieldSlotMut, Revision, SlotDataAccess,
    SlotDataMutAccess, SlotMapValueAccess, SlotMapValueMutAccess, SlotRecordAccess,
    SlotRecordMutAccess, SlotShape, SlotValueAccess,
};

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
}

impl Default for NodeInvocation {
    fn default() -> Self {
        Self::path(ArtifactLocator::path(""))
    }
}

impl Serialize for NodeInvocation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("def", &self.def)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for NodeInvocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)?;
        let table = value
            .as_table()
            .ok_or_else(|| serde::de::Error::custom("node invocation must be a table"))?;
        if table.len() != 1 || !table.contains_key("def") {
            return Err(serde::de::Error::custom(
                "node invocation must contain exactly one `def` field",
            ));
        }
        let def = NodeDefRef::from_toml_value(
            table
                .get("def")
                .ok_or_else(|| serde::de::Error::custom("missing `def`"))?
                .clone(),
        )
        .map_err(serde::de::Error::custom)?;
        Ok(Self::from(def))
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

impl Serialize for NodeDefRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Path(locator) => {
                use serde::ser::SerializeMap;

                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("path", &locator.to_string())?;
                map.end()
            }
            Self::Inline(def) => node_def_to_toml_value(def)
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for NodeDefRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)?;
        Self::from_toml_value(value).map_err(serde::de::Error::custom)
    }
}

impl NodeDefRef {
    fn from_toml_value(value: toml::Value) -> Result<Self, String> {
        let table = value
            .as_table()
            .ok_or_else(|| String::from("node def reference must be a table"))?;
        if let Some(path) = table.get("path") {
            if table.len() != 1 {
                return Err(String::from(
                    "`def.path` cannot be combined with inline node definition fields",
                ));
            }
            let Some(path) = path.as_str() else {
                return Err(String::from("`def.path` must be a string"));
            };
            return ArtifactLocator::parse(path)
                .map(Self::Path)
                .map_err(|e| e.to_string());
        }
        if table.contains_key("kind") {
            let text = toml::to_string(&value).map_err(|e| e.to_string())?;
            let def = NodeDef::from_toml_str(&text).map_err(|e| e.to_string())?;
            return Ok(Self::Inline(Box::new(def)));
        }
        Err(String::from(
            "node def reference must contain `path` or inline `kind`",
        ))
    }
}

impl FieldSlot for NodeInvocation {
    fn slot_field_shape() -> SlotShape {
        crate::slot::shape::record(alloc::vec![crate::slot::shape::field(
            "def",
            <ArtifactPathSlot as FieldSlot>::slot_field_shape(),
        )])
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl FieldSlotMut for NodeInvocation {
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Record(self)
    }
}

impl SlotMapValueAccess for NodeInvocation {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlotMapValueMutAccess for NodeInvocation {
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Record(self)
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

fn node_def_to_toml_value(def: &NodeDef) -> Result<toml::Value, String> {
    let mut registry = crate::SlotShapeRegistry::default();
    crate::slot_shapes::register_all_static_slot_shapes(&mut registry)
        .map_err(|e| e.to_string())?;
    let text = def.write_toml(&registry).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_invocation_toml_path_form_loads() {
        let invocation: NodeInvocation = toml::from_str(
            r#"
def = { path = "./texture.toml" }
"#,
        )
        .unwrap();

        assert_eq!(
            invocation.def_locator().unwrap(),
            &ArtifactLocator::path("./texture.toml")
        );
    }

    #[test]
    fn node_invocation_rejects_legacy_artifact() {
        let err = toml::from_str::<NodeInvocation>(
            r#"
artifact = "./texture.toml"
"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("def"));
    }

    #[test]
    fn node_invocation_toml_inline_form_loads() {
        let invocation: NodeInvocation = toml::from_str(
            r#"
[def]
kind = "Clock"
"#,
        )
        .unwrap();

        assert!(matches!(invocation.inline_def(), Some(NodeDef::Clock(_))));
    }

    #[test]
    fn node_invocation_rejects_path_plus_inline_fields() {
        let err = toml::from_str::<NodeInvocation>(
            r#"
[def]
path = "./clock.toml"
kind = "Clock"
"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("def.path"));
    }
}
