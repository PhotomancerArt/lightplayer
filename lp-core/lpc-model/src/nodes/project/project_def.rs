use alloc::string::String;

use crate::{MapSlot, NodeInvocationSlot, OptionSlot, Slotted, ValueSlot};

/// Authored root project node definition.
///
/// A project is a node artifact with `kind = "Project"`. Its `nodes` table
/// owns named child [`crate::NodeInvocationSlot`] entries; the runtime no
/// longer discovers children from filesystem directories.
#[derive(Clone, Debug, Default, PartialEq, Slotted)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProjectDef {
    pub name: OptionSlot<ValueSlot<String>>,
    /// Named child node positions owned by this project.
    pub nodes: MapSlot<String, NodeInvocationSlot>,
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
    use alloc::string::ToString;

    #[test]
    fn project_def_deserializes_named_nodes() {
        let json = r#"{
            "kind": "Project",
            "name": "basic",
            "nodes": {
                "texture": { "ref": "./texture.json" },
                "shader": { "ref": "./shader.json" }
            }
        }"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Project(def) = def else {
            panic!("expected project def");
        };
        assert!(def.is_project_kind());
        assert_eq!(def.name(), Some("basic"));
        assert_eq!(def.nodes.entries.len(), 2);
        assert!(def.nodes.entries.contains_key("texture"));
        assert!(def.nodes.entries.contains_key("shader"));
    }

    #[test]
    fn project_def_rejects_legacy_artifact_field() {
        let json = r#"{
            "kind": "Project",
            "nodes": {
                "texture": { "artifact": "./texture.json" }
            }
        }"#;
        let err = NodeDef::read_json(&registry(), json).unwrap_err();
        assert!(err.to_string().contains("ref"));
    }

    #[test]
    fn project_def_deserializes_inline_node() {
        let json = r#"{
            "kind": "Project",
            "nodes": {
                "clock": { "def": { "kind": "Clock" } }
            }
        }"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Project(def) = def else {
            panic!("expected project def");
        };
        let clock = def.nodes.entries.get("clock").expect("clock");
        assert!(matches!(
            clock.value().inline_def(),
            Some(NodeDef::Clock(_))
        ));
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }
}
