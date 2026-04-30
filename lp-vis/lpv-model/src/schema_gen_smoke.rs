//! Compile-time smoke test that `schemars::schema_for!` succeeds on every
//! public type in `lp-domain`. Gated on `feature = "schema-gen"`.

#![cfg(feature = "schema-gen")]

#[cfg(test)]
mod tests {
    use crate::LpsType;
    use crate::NodeName;
    use crate::constraint::Constraint;
    use crate::kind::{Colorspace, Dimension, InterpMethod, Kind, Unit};
    use crate::presentation::Presentation;
    use crate::value_spec::{TextureSpec, ValueSpec};
    use crate::{
        Effect, EffectRef, Live, LiveCandidate, ParamsTable, Pattern, Playlist, PlaylistBehavior,
        PlaylistEntry, ShaderRef, Stack, Transition, TransitionRef, VisualInput,
    };
    use lpc_model::node::node_id::NodeId;
    use lpc_model::node::node_prop_spec::NodePropSpec;
    use lpc_model::prop::binding::Binding;
    use lpc_model::prop::shape::{Shape, Slot};
    use lpc_model::tree::tree_path::{NodePathSegment, TreePath};
    use lpc_model::{ArtifactSpec, ChannelName};

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

    // M3 — Visual kinds + key substructure (derive / hand-written JsonSchema).
    #[test]
    fn schema_pattern() {
        assert_schema_compiles!(Pattern);
    }
    #[test]
    fn schema_effect() {
        assert_schema_compiles!(Effect);
    }
    #[test]
    fn schema_transition() {
        assert_schema_compiles!(Transition);
    }
    #[test]
    fn schema_stack() {
        assert_schema_compiles!(Stack);
    }
    #[test]
    fn schema_live() {
        assert_schema_compiles!(Live);
    }
    #[test]
    fn schema_playlist() {
        assert_schema_compiles!(Playlist);
    }
    #[test]
    fn schema_shader_ref() {
        assert_schema_compiles!(ShaderRef);
    }
    #[test]
    fn schema_visual_input() {
        assert_schema_compiles!(VisualInput);
    }
    #[test]
    fn schema_params_table() {
        assert_schema_compiles!(ParamsTable);
    }
    #[test]
    fn schema_transition_ref() {
        assert_schema_compiles!(TransitionRef);
    }
    #[test]
    fn schema_playlist_entry() {
        assert_schema_compiles!(PlaylistEntry);
    }
    #[test]
    fn schema_playlist_behavior() {
        assert_schema_compiles!(PlaylistBehavior);
    }
    #[test]
    fn schema_live_candidate() {
        assert_schema_compiles!(LiveCandidate);
    }
    #[test]
    fn schema_effect_ref() {
        assert_schema_compiles!(EffectRef);
    }

    #[test]
    fn slot_schema_is_recursive_and_non_trivial() {
        let schema = schemars::schema_for!(Slot);
        let json = serde_json::to_string(&schema).unwrap();
        // Wire-true `JsonSchema` for `Slot` (see `impl JsonSchema for Slot`): a
        // `oneOf` of scalar range/choice/free, array, and struct arms — no
        // separate `subschema_for::<Shape>()` (the tagged `Shape` enum is not
        // what `impl Serialize for Slot` emits). Recursion is the root-`#` $ref
        // on `element` and on struct `fields` items.
        assert!(
            json.contains("oneOf"),
            "Slot wire schema should be a oneOf: {json}"
        );
        assert!(
            json.contains("\"$ref\":\"#\""),
            "Slot should recurse with root-anchored $ref: {json}"
        );
        for needle in [r#""const":"array""#, r#""const":"struct""#] {
            assert!(
                json.contains(needle),
                "Slot wire schema missing {needle}: {json}"
            );
        }
        for needle in ["kind", "element", "fields"] {
            assert!(
                json.contains(needle),
                "Slot wire schema missing `{needle}`: {json}"
            );
        }
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
