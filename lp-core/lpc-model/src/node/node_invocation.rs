//! Parent-owned instruction to instantiate a child node.
//!
//! A parent either references an artifact-backed node definition or embeds the
//! child definition inline at the invocation site. Artifact references remain
//! the normal sharable form; inline definitions are useful for small built-ins
//! such as a project-local clock.

use crate::ArtifactPathSlot;
use crate::artifact::artifact_loc::ArtifactLocator;
use crate::{
    FieldSlot, LpType, LpValue, ModelStructMember, NodeDef, Revision, SlotDataAccess, SlotMeta,
    SlotShape, SlotValueAccess, SlotValueShape,
};
use alloc::string::{String, ToString};
use alloc::vec;

/// Parent-owned child node invocation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum NodeInvocation {
    /// Artifact to load for this child node definition.
    Artifact { artifact: ArtifactPathSlot },
    /// Inline child node definition owned by this invocation.
    Inline(NodeDef),
}

impl NodeInvocation {
    /// New artifact-backed invocation.
    pub fn new(artifact: ArtifactLocator) -> Self {
        Self::Artifact {
            artifact: ArtifactPathSlot::new(artifact.to_string()),
        }
    }

    /// New inline invocation.
    pub fn inline(def: NodeDef) -> Self {
        Self::Inline(def)
    }

    pub fn artifact_locator(&self) -> Option<Result<ArtifactLocator, &'static str>> {
        match self {
            Self::Artifact { artifact } => Some(ArtifactLocator::parse(artifact.value())),
            Self::Inline(_) => None,
        }
    }

    pub fn artifact_path_text(&self) -> Option<&str> {
        match self {
            Self::Artifact { artifact } => Some(artifact.value().as_str()),
            Self::Inline(_) => None,
        }
    }

    pub fn inline_def(&self) -> Option<&NodeDef> {
        match self {
            Self::Artifact { .. } => None,
            Self::Inline(def) => Some(def),
        }
    }
}

impl SlotValueAccess for NodeInvocation {
    fn changed_at(&self) -> Revision {
        match self {
            Self::Artifact { artifact } => artifact.changed_at(),
            Self::Inline(_) => Revision::default(),
        }
    }

    fn value(&self) -> LpValue {
        match self {
            Self::Artifact { artifact } => LpValue::Struct {
                name: Some(String::from("NodeInvocation")),
                fields: vec![
                    (
                        String::from("form"),
                        LpValue::String(String::from("artifact")),
                    ),
                    (
                        String::from("artifact"),
                        LpValue::String(artifact.value().clone()),
                    ),
                ],
            },
            Self::Inline(def) => LpValue::Struct {
                name: Some(String::from("NodeInvocation")),
                fields: vec![
                    (
                        String::from("form"),
                        LpValue::String(String::from("inline")),
                    ),
                    (
                        String::from("node_kind"),
                        LpValue::String(String::from(def.kind_name())),
                    ),
                ],
            },
        }
    }
}

impl FieldSlot for NodeInvocation {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(node_invocation_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

fn node_invocation_shape() -> SlotValueShape {
    SlotValueShape {
        id: crate::SlotShapeId::from_static_name("lp::node::Invocation"),
        ty: LpType::Struct {
            name: Some(String::from("NodeInvocation")),
            fields: vec![
                ModelStructMember {
                    name: String::from("form"),
                    ty: LpType::String,
                },
                ModelStructMember {
                    name: String::from("artifact"),
                    ty: LpType::String,
                },
                ModelStructMember {
                    name: String::from("node_kind"),
                    ty: LpType::String,
                },
            ],
        },
        meta: SlotMeta::empty(),
        editor: Default::default(),
    }
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for NodeInvocation {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        alloc::borrow::Cow::Borrowed("NodeInvocation")
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <alloc::collections::BTreeMap<String, String> as schemars::JsonSchema>::json_schema(
            generator,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_invocation_round_trips() {
        let config = NodeInvocation::new(ArtifactLocator::path("./fluid.vis"));
        let json = serde_json::to_string(&config).unwrap();
        let back: NodeInvocation = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_invocation_json_has_artifact_only_shape() {
        let config = NodeInvocation::new(ArtifactLocator::path("./test.lp"));
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("artifact"));
        assert!(!json.contains("override"));
    }

    #[test]
    fn node_invocation_toml_round_trips() {
        let config = NodeInvocation::new(ArtifactLocator::path("./pattern.lp"));
        let toml_str = toml::to_string(&config).unwrap();
        let back: NodeInvocation = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_invocation_toml_table_form_loads() {
        let toml = r#"
            artifact = "./texture.toml"
        "#;
        let invocation: NodeInvocation = toml::from_str(toml).unwrap();
        assert_eq!(
            invocation.artifact_locator().unwrap().unwrap(),
            ArtifactLocator::path("./texture.toml")
        );
    }

    #[test]
    fn node_invocation_toml_inline_form_loads() {
        let toml = r#"
            kind = "output"
            pin = 18
        "#;
        let invocation: NodeInvocation = toml::from_str(toml).unwrap();
        let Some(NodeDef::Output(def)) = invocation.inline_def() else {
            panic!("inline output def");
        };
        assert_eq!(def.pin(), 18);
    }
}
