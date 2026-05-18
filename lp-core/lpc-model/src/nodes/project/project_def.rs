use alloc::string::String;

use crate::node::node_invocation::NodeInvocation;
use crate::{MapSlot, OptionSlot, Slotted, ValueSlot};

/// Authored root project node definition.
///
/// A project is a node artifact with `kind = "Project"`. Its `nodes` table is
/// the explicit source of child node invocations; the runtime no longer
/// discovers children from filesystem directories.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, Slotted)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProjectDef {
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub name: OptionSlot<ValueSlot<String>>,
    #[serde(default, skip_serializing_if = "MapSlot::is_empty")]
    pub nodes: MapSlot<String, NodeInvocation>,
}

impl ProjectDef {
    pub const KIND: &'static str = "project";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Project
    }

    pub fn is_project_kind(&self) -> bool {
        true
    }

    pub fn name(&self) -> Option<&str> {
        self.name.data.as_ref().map(|name| name.value().as_str())
    }
}

#[cfg(test)]
mod tests {
    use crate::{NodeDef, SlotShapeRegistry};

    #[test]
    fn project_def_deserializes_named_nodes() {
        let toml = r#"
            kind = "Project"
            name = "basic"

            [nodes.texture]
            artifact = "./texture.toml"

            [nodes.shader]
            artifact = "./shader.toml"
        "#;
        let def = NodeDef::read_toml(&registry(), toml).unwrap();
        let NodeDef::Project(def) = def else {
            panic!("expected project def");
        };
        assert!(def.is_project_kind());
        assert_eq!(def.name(), Some("basic"));
        assert_eq!(def.nodes.entries.len(), 2);
        assert!(def.nodes.entries.contains_key("texture"));
        assert!(def.nodes.entries.contains_key("shader"));
    }

    fn registry() -> SlotShapeRegistry {
        let mut registry = SlotShapeRegistry::default();
        crate::slot_shapes::register_all_static_slot_shapes(&mut registry).expect("shapes");
        registry
    }
}
