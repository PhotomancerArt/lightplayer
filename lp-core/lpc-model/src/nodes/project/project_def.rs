use alloc::string::String;

use crate::node::kind::NodeKind;
use crate::nodes::node_def::NodeDef;
use crate::node::node_invocation::NodeInvocation;
use crate::{MapSlot, OptionSlot, ValueSlot};

/// Authored root project node definition.
///
/// A project is a node artifact with `kind = "project"`. Its `nodes` table is
/// the explicit source of child node invocations; the runtime no longer
/// discovers children from filesystem directories.
#[derive(
    Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, lpc_slot_macros::SlotRecord,
)]
#[slot(root)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProjectDef {
    #[slot(skip)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub name: OptionSlot<ValueSlot<String>>,
    #[serde(default, skip_serializing_if = "MapSlot::is_empty")]
    pub nodes: MapSlot<String, NodeInvocation>,
}

impl ProjectDef {
    pub const KIND: &'static str = "project";

    pub fn is_project_kind(&self) -> bool {
        self.kind == Self::KIND
    }

    pub fn name(&self) -> Option<&str> {
        self.name.data.as_ref().map(|name| name.value().as_str())
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
    use crate::nodes::project::project_def::ProjectDef;

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
        assert_eq!(def.name(), Some("basic"));
        assert_eq!(def.nodes.entries.len(), 2);
        assert!(def.nodes.entries.contains_key("texture"));
        assert!(def.nodes.entries.contains_key("shader"));
    }
}
