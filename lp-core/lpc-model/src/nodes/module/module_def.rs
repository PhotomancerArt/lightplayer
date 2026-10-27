use alloc::string::String;

use crate::nodes::ProvenanceDef;
use crate::{
    BindingDefs, BindingRef, ChannelNameListSlot, MapSlot, NodeInvocationSlot, OptionSlot, Slotted,
    ValueSlot,
};

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
    #[slot(role = "fixed")]
    pub nodes: MapSlot<String, NodeInvocationSlot>,
    /// Authored slot bindings on the module node itself, like any node —
    /// this is where R7's contention pick ("two writers on `visual.out` =
    /// ambiguous until the author picks") becomes authorable, and where an
    /// export slot publishes outward.
    pub bindings: BindingDefs,
    /// The channels this module takes from wherever it is placed (bare
    /// names: `["palette", "detail"]`). The mirror of an export: a declared
    /// input lists in the module's host scope — through a playlist entry's
    /// sink, to the playlist's level — so the host's panel offers one
    /// control every module that declares it follows, until a module's own
    /// value cuts it loose (modules.md R2/R7; playlists-on-the-bus D3–D5).
    /// Names only: label and range come from the slots bound to the
    /// channel (R9). Absent declares nothing.
    pub inputs: OptionSlot<ChannelNameListSlot>,
    /// Authored exports (R7): local produced-slot name → the inner-scope
    /// channel it mirrors (`bus:` ref form), registered by the loader as
    /// a binding that publishes into the containing scope (R4).
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
            r#"{ "kind": "Module", "uid": "prj0000000000000042", "nodes": {} }"#,
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
    fn module_def_nodes_are_fixed() {
        use crate::{SlotRole, SlotShape, StaticSlotShape};

        let SlotShape::Record { fields, .. } = crate::ModuleDef::slot_shape() else {
            panic!("module def shape must be a record");
        };
        let nodes = fields
            .iter()
            .find(|field| field.name.as_str() == "nodes")
            .expect("nodes field");
        assert_eq!(nodes.role, SlotRole::Fixed);
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
        for key in ["bindings", "inputs", "exports", "meta", "provenance"] {
            assert!(
                !text.contains(key),
                "{key} must not serialize when absent: {text}"
            );
        }
    }

    /// A pattern declares what it takes (playlists-on-the-bus D4): the list
    /// round-trips byte for byte, beside `exports`, and a module without it
    /// writes nothing new.
    #[test]
    fn declared_inputs_round_trip_byte_identically() {
        let registry = registry();
        let json = r#"{
  "kind": "Module",
  "nodes": {
    "shader": {
      "ref": "./shader.json"
    }
  },
  "inputs": [
    "palette",
    "detail"
  ],
  "provenance": {
    "author": "Photomancer",
    "version": "1",
    "license": "CC0-1.0"
  }
}
"#;
        let def = NodeDef::read_json(&registry, json).expect("inputs parse");
        let NodeDef::Module(module) = &def else {
            panic!("expected module def");
        };
        let inputs = module.inputs.data.as_ref().expect("inputs");
        assert!(inputs.value().contains("palette"));
        assert!(inputs.value().contains("detail"));
        assert_eq!(def.write_json(&registry).expect("re-write"), json);
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }
}
