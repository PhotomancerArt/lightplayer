use alloc::string::String;

use crate::{MapSlot, NodeInvocationSlot, OptionSlot, Slotted, ValueSlot};

/// Monotonic format version of authored `project.json` artifacts.
///
/// The project root carries this as its top-level `format` key; child node
/// files are versioned transitively through their project root. Loaders
/// reject roots whose format is missing or does not match, so bump this when
/// making a format-breaking change to authored artifacts.
///
/// History:
/// - `2` — shader nodes replaced the `glsl_opts` record (`add_sub`/`mul`/`div`
///   Q32 mode slots) with a single `float_mode` slot. Artifacts at version `1`
///   are refused, not migrated (alpha format posture: bump and refuse).
pub const PROJECT_FORMAT_VERSION: u32 = 2;

/// Authored root module node definition.
///
/// A module is a node artifact with `kind = "Module"`. Its `nodes` table
/// owns named child [`crate::NodeInvocationSlot`] entries; the runtime no
/// longer discovers children from filesystem directories.
#[derive(Clone, Debug, Default, PartialEq, Slotted)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ModuleDef {
    /// Authored format version; see [`PROJECT_FORMAT_VERSION`].
    ///
    /// Read-only through mutations: only the loader format gate and the
    /// (future) offline upgrader own this value.
    #[slot(policy = "read_only_persisted")]
    pub format: OptionSlot<ValueSlot<u32>>,
    /// Stable project identity (`prj_…`, base-62), minted by the library
    /// when a project enters it. Travels with the files: parity checks,
    /// history, and device associations key off it (PM roadmap M1/M3).
    ///
    /// Read-only through mutations: identity is minted by the library on
    /// entry (and re-minted on import, so a shared copy never collides
    /// with its source). Editing it in place would silently reassign a
    /// project's history and device associations, so no surface may offer
    /// it — the constraint lives here rather than in each view.
    #[slot(policy = "read_only_persisted")]
    pub uid: OptionSlot<ValueSlot<String>>,
    /// Human-readable project name — the one authored field of the root's
    /// identity, and the Studio project pane's title.
    pub name: OptionSlot<ValueSlot<String>>,
    /// Named child node positions owned by this project.
    ///
    /// Read-only through mutations: node create/remove will arrive as
    /// dedicated project operations (Studio authoring M2), never as raw
    /// slot edits under this map.
    #[slot(policy = "read_only_persisted")]
    pub nodes: MapSlot<String, NodeInvocationSlot>,
}

impl ModuleDef {
    pub const KIND: &'static str = "module";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Module
    }

    pub fn is_module_kind(&self) -> bool {
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
            "kind": "Module",
            "format": 2,
            "name": "basic",
            "nodes": {
                "texture": { "ref": "./texture.json" },
                "shader": { "ref": "./shader.json" }
            }
        }"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Module(def) = def else {
            panic!("expected project def");
        };
        assert!(def.is_module_kind());
        assert_eq!(def.format(), Some(super::PROJECT_FORMAT_VERSION));
        assert_eq!(def.name(), Some("basic"));
        assert_eq!(def.nodes.entries.len(), 2);
        assert!(def.nodes.entries.contains_key("texture"));
        assert!(def.nodes.entries.contains_key("shader"));
    }

    #[test]
    fn project_def_format_is_none_when_absent() {
        let json = r#"{
            "kind": "Module",
            "nodes": {}
        }"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Module(def) = def else {
            panic!("expected project def");
        };
        assert_eq!(def.format(), None);
    }

    #[test]
    fn project_def_writes_format_alongside_kind() {
        let def = crate::ModuleDef {
            format: crate::ModuleDef::current_format_slot(),
            ..crate::ModuleDef::default()
        };
        let text = NodeDef::Module(def).write_json(&registry()).unwrap();
        assert!(
            text.starts_with("{\n  \"kind\": \"Project\",\n  \"format\": 2"),
            "{text}"
        );
    }

    #[test]
    fn project_def_rejects_legacy_artifact_field() {
        let json = r#"{
            "kind": "Module",
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
            "kind": "Module",
            "nodes": {
                "clock": { "def": { "kind": "Clock" } }
            }
        }"#;
        let err = NodeDef::read_json(&registry(), json).unwrap_err();
        assert!(err.to_string().contains("def"), "{err}");
    }

    #[test]
    fn project_def_format_and_nodes_are_read_only_persisted_name_writable() {
        use crate::{SlotPolicy, SlotShape, StaticSlotShape};

        let SlotShape::Record { fields, .. } = crate::ModuleDef::slot_shape() else {
            panic!("project def shape must be a record");
        };
        let policy = |name: &str| {
            fields
                .iter()
                .find(|field| field.name.as_str() == name)
                .unwrap_or_else(|| panic!("{name} field"))
                .policy
        };
        assert_eq!(policy("format"), SlotPolicy::read_only_persisted());
        assert_eq!(policy("nodes"), SlotPolicy::read_only_persisted());
        assert_eq!(policy("name"), SlotPolicy::writable_persisted());
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }
}
