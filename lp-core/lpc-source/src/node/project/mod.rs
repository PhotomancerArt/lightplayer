use crate::node::{NodeDef, NodeInvocation, NodeKind};
use alloc::collections::BTreeMap;
use alloc::string::String;
use lpc_model::NodeName;

/// Authored root project node definition.
///
/// A project is a node artifact with `kind = "project"`. Its `nodes` table is
/// the explicit source of child node invocations; the runtime no longer
/// discovers children from filesystem directories.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProjectDef {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nodes: BTreeMap<NodeName, NodeInvocation>,
}

impl ProjectDef {
    pub const KIND: &'static str = "project";

    pub fn is_project_kind(&self) -> bool {
        self.kind == Self::KIND
    }
}

impl NodeDef for ProjectDef {
    fn kind(&self) -> NodeKind {
        NodeKind::Project
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_def_deserializes_named_nodes() {
        let toml = r#"
            kind = "project"
            name = "basic"

            [nodes.texture]
            artifact = "./texture.toml"

            [nodes.shader]
            artifact = "./shader.toml"
        "#;
        let def: ProjectDef = toml::from_str(toml).unwrap();
        assert!(def.is_project_kind());
        assert_eq!(def.name.as_deref(), Some("basic"));
        assert_eq!(def.nodes.len(), 2);
        assert!(def.nodes.contains_key(&NodeName::parse("texture").unwrap()));
        assert!(def.nodes.contains_key(&NodeName::parse("shader").unwrap()));
    }
}
