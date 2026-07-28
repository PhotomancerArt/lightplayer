use alloc::string::String;

use crate::{MapSlot, NodeInvocationSlot, OptionSlot, Slotted, ValueSlot};

use super::PromotedControlDef;

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
    ///
    /// Read-only through mutations: only the loader format gate and the
    /// (future) offline upgrader own this value.
    #[slot(policy = "read_only_persisted")]
    pub format: OptionSlot<ValueSlot<u32>>,
    /// Stable project identity (`prj_…`, base-62), minted by the library
    /// when a project enters it. Travels with the files: parity checks,
    /// history, and device associations key off it (PM roadmap M1/M3).
    pub uid: OptionSlot<ValueSlot<String>>,
    pub name: OptionSlot<ValueSlot<String>>,
    /// Named child node positions owned by this project.
    ///
    /// Read-only through mutations: node create/remove will arrive as
    /// dedicated project operations (Studio authoring M2), never as raw
    /// slot edits under this map.
    #[slot(policy = "read_only_persisted")]
    pub nodes: MapSlot<String, NodeInvocationSlot>,
    /// Promoted controls — the project's curated public knobs, keyed by a
    /// stable user-facing name (effects-are-projects ADR). Each entry
    /// aliases a slot on a direct child; values live on the target slot.
    pub controls: MapSlot<String, PromotedControlDef>,
    /// Provenance: effect author attribution (plain string, optional).
    pub author: OptionSlot<ValueSlot<String>>,
    /// Provenance: authored version string (no semver semantics yet).
    pub version: OptionSlot<ValueSlot<String>>,
    /// Provenance: license identifier (e.g. `"CC0-1.0"`). Repo samples are
    /// CC0 unless otherwise noted.
    pub license: OptionSlot<ValueSlot<String>>,
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

    #[test]
    fn project_def_format_and_nodes_are_read_only_persisted_name_writable() {
        use crate::{SlotPolicy, SlotShape, StaticSlotShape};

        let SlotShape::Record { fields, .. } = crate::ProjectDef::slot_shape() else {
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

    #[test]
    fn project_def_without_new_fields_writes_byte_identically() {
        // Effects-are-projects ADR: `controls`/`author`/`version`/`license`
        // are additive and serialize skip-if-default — existing artifacts
        // must stay byte-identical.
        let json = "{\n  \"kind\": \"Project\",\n  \"format\": 1,\n  \"name\": \"basic\",\n  \"nodes\": {\n    \"shader\": {\n      \"ref\": \"./shader.json\"\n    }\n  }\n}\n";
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let rewritten = def.write_json(&registry()).unwrap();
        assert_eq!(rewritten, json);
        assert!(!rewritten.contains("controls"), "{rewritten}");
        assert!(!rewritten.contains("author"), "{rewritten}");
        assert!(!rewritten.contains("version"), "{rewritten}");
        assert!(!rewritten.contains("license"), "{rewritten}");
    }

    #[test]
    fn project_def_controls_and_provenance_round_trip_byte_stably() {
        use crate::nodes::project::PromotedControlDef;
        use crate::{BindingRef, MapSlot, OptionSlot, ValueSlot};
        use lp_collection::VecMap;

        let mut controls = VecMap::new();
        let mut speed =
            PromotedControlDef::to_target(BindingRef::parse("node:./shader#speed").unwrap());
        speed.label = OptionSlot::some(ValueSlot::new("Speed".to_string()));
        speed.min = OptionSlot::some(ValueSlot::new(0.0));
        speed.max = OptionSlot::some(ValueSlot::new(4.0));
        controls.insert("speed".to_string(), speed);

        let def = crate::ProjectDef {
            format: crate::ProjectDef::current_format_slot(),
            name: OptionSlot::some(ValueSlot::new("glow".to_string())),
            controls: MapSlot::new(controls),
            author: OptionSlot::some(ValueSlot::new("photomancer".to_string())),
            version: OptionSlot::some(ValueSlot::new("1".to_string())),
            license: OptionSlot::some(ValueSlot::new("CC0-1.0".to_string())),
            ..crate::ProjectDef::default()
        };
        let first = NodeDef::Project(def).write_json(&registry()).unwrap();
        assert!(first.contains("\"controls\""), "{first}");
        assert!(
            first.contains("\"target\": \"node:shader#speed\""),
            "{first}"
        );
        assert!(first.contains("\"license\": \"CC0-1.0\""), "{first}");

        let read = NodeDef::read_json(&registry(), &first).unwrap();
        let NodeDef::Project(project) = &read else {
            panic!("expected project");
        };
        let control = project.controls.entries.get("speed").expect("speed");
        assert_eq!(
            control.target.value(),
            &crate::BindingRef::parse("node:./shader#speed").unwrap()
        );
        assert_eq!(
            control.min.data.as_ref().map(|slot| *slot.value()),
            Some(0.0)
        );
        let second = read.write_json(&registry()).unwrap();
        assert_eq!(first, second);
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }
}
