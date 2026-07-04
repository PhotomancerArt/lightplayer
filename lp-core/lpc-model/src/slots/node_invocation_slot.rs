//! Parent-owned child node invocation.
//!
//! A [`NodeInvocation`] is the authored value stored by a parent when it owns a
//! child node position. It can be unset or reference another node artifact —
//! strictly one node definition per artifact file.
//!
//! A [`NodeInvocationSlot`] is the slot wrapper used by slotted node
//! definitions. Prefer the slot alias for fields in authored model structs, and
//! use [`NodeInvocation`] for the value after reading or unwrapping the slot.

use alloc::string::ToString;

use crate::artifact::artifact_spec::ArtifactSpec;
use crate::{ArtifactPath, ArtifactPathSlot, EnumSlot, Slotted};

/// Slot wrapper for an authored child node invocation.
pub type NodeInvocationSlot = EnumSlot<NodeInvocation>;

/// Authored value for one parent-owned child node position.
#[derive(Clone, Debug, PartialEq, Slotted)]
#[slot(enum_encoding = "external", rename_all = "snake_case")]
pub enum NodeInvocation {
    /// Reserved map entry with no wiring yet (valid while editing).
    #[default]
    Unset,
    Ref(ArtifactPathSlot),
}

// `NodeInvocation` is not a serde type; it is read/written through the slot
// codec as an externally-tagged enum (`enum_encoding = "external"`,
// `rename_all = "snake_case"`). Since the JSON artifact cutover (one node per
// artifact file), its authored wire form is one of:
//   `{ "unset": {} }` or `{ "ref": <artifact path string> }`.
#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for NodeInvocation {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "NodeInvocation".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let ref_schema = <ArtifactPathSlot as schemars::JsonSchema>::json_schema(generator);
        schemars::json_schema!({
            "description": "Parent-owned child node invocation (externally tagged).",
            "oneOf": [
                {
                    "type": "object",
                    "properties": { "unset": { "type": "object", "additionalProperties": false } },
                    "required": ["unset"],
                    "additionalProperties": false,
                },
                {
                    "type": "object",
                    "properties": { "ref": ref_schema },
                    "required": ["ref"],
                    "additionalProperties": false,
                },
            ],
        })
    }
}

impl NodeInvocation {
    /// Construct a path-backed invocation.
    #[must_use]
    pub fn new(specifier: ArtifactSpec) -> Self {
        Self::path(specifier)
    }

    #[must_use]
    pub fn path(specifier: ArtifactSpec) -> Self {
        Self::Ref(ArtifactPathSlot::new(ArtifactPath(specifier.to_string())))
    }

    pub fn ref_specifier(&self) -> Option<ArtifactSpec> {
        match self {
            Self::Unset => None,
            Self::Ref(path) => {
                let text = path.value().as_str();
                if text.is_empty() {
                    None
                } else {
                    ArtifactSpec::parse(text).ok()
                }
            }
        }
    }

    pub fn is_unset(&self) -> bool {
        matches!(self, Self::Unset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EnumSlot, FieldSlotMut, NodeDef, SlotEnumShape, SlotShapeRegistry};

    #[test]
    fn node_invocation_default_is_unset() {
        assert!(NodeInvocation::default().is_unset());
    }

    #[test]
    fn node_invocation_json_unset_form_loads() {
        let invocation = read_invocation(r#"{ "unset": {} }"#);
        assert!(invocation.is_unset());
    }

    #[test]
    fn node_invocation_json_ref_form_loads() {
        let invocation = read_invocation(r#"{ "ref": "./texture.json" }"#);

        assert_eq!(
            invocation.ref_specifier().unwrap(),
            ArtifactSpec::path("./texture.json")
        );
    }

    #[test]
    fn node_invocation_rejects_legacy_def_path_form() {
        let err = read_invocation_err(r#"{ "def": { "path": "./texture.json" } }"#);

        assert!(err.to_string().contains("def") || err.to_string().contains("unknown"));
    }

    #[test]
    fn node_invocation_rejects_legacy_artifact_field() {
        let err = read_invocation_err(r#"{ "artifact": "./texture.json" }"#);

        assert!(err.to_string().contains("artifact") || err.to_string().contains("unknown"));
    }

    #[test]
    fn node_invocation_rejects_inline_def_form() {
        let err = read_invocation_err(r#"{ "def": { "kind": "Clock" } }"#);

        assert!(err.to_string().contains("def"), "{err}");
    }

    #[test]
    fn node_invocation_round_trips_unset_form() {
        let text = r#"{
  "kind": "Project",
  "nodes": {
    "placeholder": { "unset": {} }
  }
}"#;
        round_trip_project_fragment(text);
    }

    #[test]
    fn node_invocation_round_trips_ref_form() {
        let text = r#"{
  "kind": "Project",
  "nodes": {
    "shader": { "ref": "./shader.json" }
  }
}"#;
        round_trip_project_fragment(text);
    }

    fn round_trip_project_fragment(text: &str) {
        let registry = SlotShapeRegistry::default();
        let def = NodeDef::read_json(&registry, text).unwrap();
        let written = NodeDef::write_json(&def, &registry).unwrap();
        let again = NodeDef::read_json(&registry, &written).unwrap();
        assert_eq!(def, again);
    }

    fn read_invocation(text: &str) -> NodeInvocation {
        read_invocation_result(text).unwrap()
    }

    fn read_invocation_err(text: &str) -> crate::slot_codec::SyntaxError {
        read_invocation_result(text).unwrap_err()
    }

    fn read_invocation_result(
        text: &str,
    ) -> Result<NodeInvocation, crate::slot_codec::SyntaxError> {
        let registry = SlotShapeRegistry::default();
        let mut reader = crate::slot_codec::SlotReader::new(
            crate::slot_codec::JsonSyntaxSource::new(text).unwrap(),
            &registry,
        );
        let mut invocation = EnumSlot::new(NodeInvocation::default());
        crate::slot_codec::apply_reader_to_slot(
            invocation.slot_field_data_mut(),
            &NodeInvocation::slot_enum_shape(),
            &registry,
            reader.value(),
        )?;
        Ok(invocation.into_inner())
    }
}
