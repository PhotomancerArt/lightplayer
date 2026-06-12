//! Parent-owned child node invocation.
//!
//! A [`NodeInvocation`] is the authored value stored by a parent when it owns a
//! child node position. It can be unset, reference another node artifact, or
//! carry an inline [`NodeDef`].
//!
//! A [`NodeInvocationSlot`] is the slot wrapper used by slotted node
//! definitions. Prefer the slot alias for fields in authored model structs, and
//! use [`NodeInvocation`] for the value after reading or unwrapping the slot.

use alloc::string::ToString;

use crate::artifact::artifact_spec::ArtifactSpec;
use crate::nodes::node_def::{NodeArtifact, NodeDef};
use crate::{
    ArtifactPath, ArtifactPathSlot, EnumSlot, FieldSlot, FieldSlotMut, SlotDataAccess,
    SlotDataMutAccess, SlotShape, Slotted, StaticSlotShape, StaticSlotShapeDescriptor,
};

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
    Def(NodeInvocationBody),
}

/// Inline node definition body referenced by shape id to avoid static descriptor cycles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeInvocationBody(pub NodeArtifact);

impl NodeInvocationBody {
    pub fn new(def: NodeDef) -> Self {
        Self(NodeArtifact::new(def))
    }

    pub fn value(&self) -> &NodeDef {
        self.0.node_def()
    }
}

impl FieldSlot for NodeInvocationBody {
    const STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        Some(&StaticSlotShapeDescriptor::Ref {
            id: NodeArtifact::SHAPE_ID,
        });

    fn slot_field_shape() -> SlotShape {
        SlotShape::reference(<NodeArtifact as StaticSlotShape>::SHAPE_ID)
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        self.0.slot_field_data()
    }
}

impl FieldSlotMut for NodeInvocationBody {
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        self.0.slot_field_data_mut()
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

    #[must_use]
    pub fn inline(def: NodeDef) -> Self {
        Self::Def(NodeInvocationBody::new(def))
    }

    pub fn ref_specifier(&self) -> Option<ArtifactSpec> {
        match self {
            Self::Unset | Self::Def(_) => None,
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

    pub fn inline_def(&self) -> Option<&NodeDef> {
        match self {
            Self::Unset | Self::Ref(_) => None,
            Self::Def(body) => Some(body.value()),
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
    fn node_invocation_toml_unset_form_loads() {
        let invocation = read_invocation(
            r#"
unset = {}
"#,
        );
        assert!(invocation.is_unset());
    }

    #[test]
    fn node_invocation_toml_ref_form_loads() {
        let invocation = read_invocation(
            r#"
ref = "./texture.toml"
"#,
        );

        assert_eq!(
            invocation.ref_specifier().unwrap(),
            ArtifactSpec::path("./texture.toml")
        );
    }

    #[test]
    fn node_invocation_rejects_legacy_def_path_form() {
        let err = read_invocation_err(
            r#"
def = { path = "./texture.toml" }
"#,
        );

        assert!(err.to_string().contains("def") || err.to_string().contains("unknown"));
    }

    #[test]
    fn node_invocation_rejects_legacy_artifact_field() {
        let err = read_invocation_err(
            r#"
artifact = "./texture.toml"
"#,
        );

        assert!(err.to_string().contains("artifact") || err.to_string().contains("unknown"));
    }

    #[test]
    fn node_invocation_toml_inline_def_form_loads() {
        let invocation = read_invocation(
            r#"
[def]
kind = "Clock"
"#,
        );

        assert!(matches!(invocation.inline_def(), Some(NodeDef::Clock(_))));
    }

    #[test]
    fn node_invocation_rejects_ref_plus_inline_def() {
        let err = read_invocation_err(
            r#"
ref = "./clock.toml"

[def]
kind = "Clock"
"#,
        );

        assert!(err.to_string().contains("def") || err.to_string().contains("unknown"));
    }

    #[test]
    fn node_invocation_round_trips_unset_form() {
        let text = r#"
kind = "Project"

[nodes.placeholder]
unset = {}
"#;
        round_trip_project_fragment(text);
    }

    #[test]
    fn node_invocation_round_trips_ref_form() {
        let text = r#"
kind = "Project"

[nodes.shader]
ref = "./shader.toml"
"#;
        round_trip_project_fragment(text);
    }

    #[test]
    fn node_invocation_round_trips_inline_def_form() {
        let text = r#"
kind = "Project"

[nodes.clock.def]
kind = "Clock"
"#;
        round_trip_project_fragment(text);
    }

    fn round_trip_project_fragment(text: &str) {
        let registry = SlotShapeRegistry::default();
        let def = NodeDef::read_toml(&registry, text).unwrap();
        let written = NodeDef::write_toml(&def, &registry).unwrap();
        let again = NodeDef::read_toml(&registry, &written).unwrap();
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
        let value = toml::from_str::<toml::Value>(text).unwrap();
        let mut reader = crate::slot_codec::SlotReader::new(
            crate::slot_codec::TomlSyntaxSource::new(&value).unwrap(),
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
