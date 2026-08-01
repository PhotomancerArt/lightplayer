use alloc::string::String;

use crate::nodes::ProvenanceDef;
use crate::{BindingDefs, BindingRef, MapSlot, NodeInvocationSlot, OptionSlot, Slotted, ValueSlot};

use super::ChannelMetaDef;

/// Authored root module node definition.
///
/// A module is a node artifact with `kind = "Module"`. Its `nodes` table
/// owns named child [`crate::NodeInvocationSlot`] entries; the runtime no
/// longer discovers children from filesystem directories.
///
/// The module carries the *technical spec* of its subtree. The project's
/// workspace identity (`format`, `uid`, `name`) lives in the non-node
/// `project.json` container manifest ([`crate::ProjectManifest`]) beside the
/// root `module.json` — the mitosis of docs/design/modules.md §1/§6.
#[derive(Clone, Debug, Default, PartialEq, Slotted)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ModuleDef {
    /// Named child node positions owned by this module.
    ///
    /// Read-only through mutations: node create/remove arrive as dedicated
    /// project operations, never as raw slot edits under this map.
    #[slot(policy = "read_only_persisted")]
    pub nodes: MapSlot<String, NodeInvocationSlot>,
    /// Authored slot bindings on the module node itself, like any node —
    /// this is where R7's contention pick ("two writers on `visual.out` =
    /// ambiguous until the author picks") becomes authorable, and where an
    /// export slot publishes outward.
    pub bindings: BindingDefs,
    /// Authored exports (R7): local produced-slot name → the inner-scope
    /// channel it mirrors (`bus:` ref form). The runtime interpretation —
    /// the slot mirroring the inner channel and publishing per R4 —
    /// arrives with the module runtime; here the def only carries data.
    pub exports: MapSlot<String, ValueSlot<BindingRef>>,
    /// Per-channel authored meta overrides for this module's scope
    /// (R9 / Q1 curation escape hatch).
    pub meta: MapSlot<String, ChannelMetaDef>,
    /// Authorship metadata (R14); normally carried by modules.
    pub provenance: OptionSlot<ProvenanceDef>,
}

impl ModuleDef {
    pub const KIND: &'static str = "module";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Module
    }

    pub fn is_module_kind(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::{NodeDef, SlotShapeRegistry};
    use alloc::string::ToString;

    #[test]
    fn module_def_deserializes_named_nodes() {
        let json = r#"{
            "kind": "Module",
            "nodes": {
                "texture": { "ref": "./texture.json" },
                "shader": { "ref": "./shader.json" }
            }
        }"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Module(def) = def else {
            panic!("expected module def");
        };
        assert!(def.is_module_kind());
        assert_eq!(def.nodes.entries.len(), 2);
        assert!(def.nodes.entries.contains_key("texture"));
        assert!(def.nodes.entries.contains_key("shader"));
    }

    #[test]
    fn module_def_rejects_container_manifest_fields() {
        // format/uid/name are container-manifest (`project.json`) concerns;
        // a module artifact carrying them is a pre-mitosis root and must
        // fail loudly rather than silently dropping identity fields.
        for json in [
            r#"{ "kind": "Module", "format": 2, "nodes": {} }"#,
            r#"{ "kind": "Module", "uid": "prj_0000000000000042", "nodes": {} }"#,
            r#"{ "kind": "Module", "name": "basic", "nodes": {} }"#,
        ] {
            let err = NodeDef::read_json(&registry(), json)
                .expect_err("container field on a module artifact");
            let text = err.to_string();
            assert!(
                text.contains("format") || text.contains("uid") || text.contains("name"),
                "{text}"
            );
        }
    }

    #[test]
    fn module_def_writes_kind_and_nodes_only() {
        // Default (empty) nodes map skips serialization entirely.
        let text = NodeDef::Module(crate::ModuleDef::default())
            .write_json(&registry())
            .unwrap();
        assert_eq!(text, "{\n  \"kind\": \"Module\"\n}\n");

        let json = r#"{ "kind": "Module", "nodes": { "clock": { "ref": "./clock.json" } } }"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let text = def.write_json(&registry()).unwrap();
        assert!(
            text.starts_with("{\n  \"kind\": \"Module\",\n  \"nodes\": {"),
            "{text}"
        );
    }

    #[test]
    fn module_def_rejects_legacy_artifact_field() {
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
    fn module_def_rejects_inline_node_definition() {
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
    fn module_def_nodes_are_read_only_persisted() {
        use crate::{SlotPolicy, SlotShape, StaticSlotShape};

        let SlotShape::Record { fields, .. } = crate::ModuleDef::slot_shape() else {
            panic!("module def shape must be a record");
        };
        let nodes = fields
            .iter()
            .find(|field| field.name.as_str() == "nodes")
            .expect("nodes field");
        assert_eq!(nodes.policy, SlotPolicy::read_only_persisted());
    }

    #[test]
    fn module_def_capability_fields_round_trip_byte_identically() {
        // P3: bindings / exports / meta / provenance survive load→save
        // byte-identically; absent fields serialize to nothing.
        let registry = registry();
        let json = r#"{
  "kind": "Module",
  "nodes": {
    "plasma": {
      "ref": "./modules/plasma/module.json"
    }
  },
  "bindings": {
    "output": {
      "target": "bus:visual.out"
    }
  },
  "exports": {
    "energy": "bus:audio.energy"
  },
  "meta": {
    "speed": {
      "label": "Master speed",
      "unit": "Hz",
      "min": 0,
      "max": 10
    }
  },
  "provenance": {
    "author": "Yona",
    "version": "0.1",
    "license": "CC0-1.0",
    "created": "2026-08-01"
  }
}
"#;
        let def = NodeDef::read_json(&registry, json).expect("capability fields parse");
        let NodeDef::Module(module) = &def else {
            panic!("expected module def");
        };
        assert!(module.bindings.0.entries.contains_key("output"));
        assert!(module.exports.entries.contains_key("energy"));
        let meta = module.meta.entries.get("speed").expect("meta entry");
        assert_eq!(
            meta.label.data.as_ref().map(|slot| slot.value().as_str()),
            Some("Master speed")
        );
        let provenance = module.provenance.data.as_ref().expect("provenance");
        assert!(!provenance.is_empty());

        let rewritten = def.write_json(&registry).expect("re-write");
        assert_eq!(rewritten, json, "capability fields must be byte-stable");

        // Absent capability fields serialize to nothing.
        let bare = NodeDef::read_json(&registry, r#"{ "kind": "Module", "nodes": {} }"#)
            .expect("bare module");
        let text = bare.write_json(&registry).expect("write bare");
        for key in ["bindings", "exports", "meta", "provenance"] {
            assert!(
                !text.contains(key),
                "{key} must not serialize when absent: {text}"
            );
        }
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }
}
