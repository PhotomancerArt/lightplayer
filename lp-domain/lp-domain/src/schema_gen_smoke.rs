//! Compile-time smoke test that `schemars::schema_for!` succeeds on every
//! public type in `lp-domain`. Gated on `feature = "schema-gen"`.

#![cfg(feature = "schema-gen")]

#[cfg(test)]
mod tests {
    use crate::LpsType;
    use crate::binding::Binding;
    use crate::constraint::Constraint;
    use crate::kind::{Colorspace, Dimension, InterpMethod, Kind, Unit};
    use crate::presentation::Presentation;
    use crate::shape::{Shape, Slot};
    use crate::types::{
        ArtifactSpec, ChannelName, Name, NodePath, NodePathSegment, NodePropSpec, Uid,
    };
    use crate::value_spec::{TextureSpec, ValueSpec};

    macro_rules! assert_schema_compiles {
        ($t:ty) => {{
            let schema = schemars::schema_for!($t);
            let json = serde_json::to_string(&schema).unwrap();
            assert!(!json.is_empty(), "schema for {} was empty", stringify!($t));
        }};
    }

    #[test]
    fn schema_uid() {
        assert_schema_compiles!(Uid);
    }
    #[test]
    fn schema_name() {
        assert_schema_compiles!(Name);
    }
    #[test]
    fn schema_node_path() {
        assert_schema_compiles!(NodePath);
    }
    #[test]
    fn schema_node_path_seg() {
        assert_schema_compiles!(NodePathSegment);
    }
    #[test]
    fn schema_node_prop_spec() {
        assert_schema_compiles!(NodePropSpec);
    }
    #[test]
    fn schema_artifact_spec() {
        assert_schema_compiles!(ArtifactSpec);
    }
    #[test]
    fn schema_channel_name() {
        assert_schema_compiles!(ChannelName);
    }

    #[test]
    fn schema_dimension() {
        assert_schema_compiles!(Dimension);
    }
    #[test]
    fn schema_unit() {
        assert_schema_compiles!(Unit);
    }
    #[test]
    fn schema_colorspace() {
        assert_schema_compiles!(Colorspace);
    }
    #[test]
    fn schema_interp_method() {
        assert_schema_compiles!(InterpMethod);
    }
    #[test]
    fn schema_kind() {
        assert_schema_compiles!(Kind);
    }
    #[test]
    fn schema_constraint() {
        assert_schema_compiles!(Constraint);
    }

    #[test]
    fn schema_presentation() {
        assert_schema_compiles!(Presentation);
    }
    #[test]
    fn schema_binding() {
        assert_schema_compiles!(Binding);
    }
    #[test]
    fn schema_value_spec() {
        assert_schema_compiles!(ValueSpec);
    }
    #[test]
    fn schema_texture_spec() {
        assert_schema_compiles!(TextureSpec);
    }

    #[test]
    fn schema_shape() {
        assert_schema_compiles!(Shape);
    }
    #[test]
    fn schema_slot() {
        assert_schema_compiles!(Slot);
    }
    #[test]
    fn schema_lps_type() {
        assert_schema_compiles!(LpsType);
    }

    #[test]
    fn slot_schema_is_recursive_and_non_trivial() {
        let schema = schemars::schema_for!(Slot);
        let json = serde_json::to_string(&schema).unwrap();
        // Slot's serialization mentions "shape", "label", "bind", "present" —
        // pick one that's stable across schemars versions.
        assert!(
            json.contains("shape"),
            "Slot schema should mention `shape`: {json}"
        );
        // Slot is recursive via Shape::Array { element: Box<Slot>, ... } and
        // Shape::Struct { fields: Vec<(Name, Slot)>, ... }. The schema must
        // therefore have at least two definitions in its definitions table
        // (Slot itself + Shape, at minimum).
        assert!(
            json.contains("Slot") && json.contains("Shape"),
            "recursive schema lost Shape/Slot definitions: {json}",
        );
    }

    #[test]
    fn shape_schema_includes_all_variants() {
        let schema = schemars::schema_for!(Shape);
        let json = serde_json::to_string(&schema).unwrap();
        for variant in ["scalar", "array", "struct"] {
            assert!(
                json.to_lowercase().contains(variant),
                "Shape schema missing variant `{variant}`: {json}",
            );
        }
    }
}
