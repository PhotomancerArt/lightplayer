use alloc::string::String;

use crate::{MapSlot, NodeInvocationSlot, OptionSlot, Slotted, ValueSlot};

/// Monotonic format version of authored `project.json` artifacts.
///
/// The project root carries this as its top-level `format` key; child node
/// files are versioned transitively through their project root. Loaders
/// reject roots whose format is missing or does not match, so bump this when
/// making a format-breaking change to authored artifacts.
pub const PROJECT_FORMAT_VERSION: u32 = 1;

/// Authored root project node definition.
///
/// A project is a node artifact with `kind = "Project"`. Its `nodes` table
/// owns named child [`crate::NodeInvocationSlot`] entries; the runtime no
/// longer discovers children from filesystem directories.
#[derive(Clone, Debug, Default, PartialEq, Slotted)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProjectDef {
    /// Authored format version; see [`PROJECT_FORMAT_VERSION`].
    pub format: OptionSlot<ValueSlot<u32>>,
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

    /// Authored format version, when the artifact carries one.
    pub fn format(&self) -> Option<u32> {
        self.format.data.as_ref().map(|format| *format.value())
    }

    /// Format slot carrying the current [`PROJECT_FORMAT_VERSION`].
    ///
    /// Every writer of a new project root must set this so freshly authored
    /// projects pass the loader format gate.
    pub fn current_format_slot() -> OptionSlot<ValueSlot<u32>> {
        OptionSlot::some(ValueSlot::new(PROJECT_FORMAT_VERSION))
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
            "format": 1,
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
        assert_eq!(def.format(), Some(super::PROJECT_FORMAT_VERSION));
        assert_eq!(def.name(), Some("basic"));
        assert_eq!(def.nodes.entries.len(), 2);
        assert!(def.nodes.entries.contains_key("texture"));
        assert!(def.nodes.entries.contains_key("shader"));
    }

    #[test]
    fn project_def_format_is_none_when_absent() {
        let json = r#"{
            "kind": "Project",
            "nodes": {}
        }"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Project(def) = def else {
            panic!("expected project def");
        };
        assert_eq!(def.format(), None);
    }

    #[test]
    fn project_def_writes_format_alongside_kind() {
        let def = crate::ProjectDef {
            format: crate::ProjectDef::current_format_slot(),
            ..crate::ProjectDef::default()
        };
        let text = NodeDef::Project(def).write_json(&registry()).unwrap();
        assert!(
            text.starts_with("{\n  \"kind\": \"Project\",\n  \"format\": 1"),
            "{text}"
        );
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
    fn project_def_rejects_inline_node_definition() {
        let json = r#"{
            "kind": "Project",
            "nodes": {
                "clock": { "def": { "kind": "Clock" } }
            }
        }"#;
        let err = NodeDef::read_json(&registry(), json).unwrap_err();
        assert!(err.to_string().contains("def"), "{err}");
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }
}
