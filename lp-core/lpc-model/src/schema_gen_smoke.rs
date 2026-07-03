//! Compile-and-shape smoke tests that `schemars::schema_for!` succeeds and
//! produces the expected wire shape for representative model types. Gated on
//! `feature = "schema-gen"`, so `no_std`/default builds never see schemars.

#![cfg(feature = "schema-gen")]

#[cfg(test)]
mod tests {
    use crate::{
        ColorOrder, ControlLamp2d, NodeInvocation, ProjectDef, SlotMapDyn,
        SlotShapeRegistrySnapshot,
    };

    macro_rules! assert_schema_compiles {
        ($t:ty) => {{
            let schema = schemars::schema_for!($t);
            let json = serde_json::to_string(&schema).unwrap();
            assert!(!json.is_empty(), "schema for {} was empty", stringify!($t));
            json
        }};
    }

    #[test]
    fn schema_color_order() {
        assert_schema_compiles!(ColorOrder);
    }

    #[test]
    fn schema_project_def() {
        assert_schema_compiles!(ProjectDef);
    }

    #[test]
    fn schema_slot_shape_registry_snapshot() {
        assert_schema_compiles!(SlotShapeRegistrySnapshot);
    }

    /// `SlotMapDyn` holds a `VecMap<SlotMapKey, SlotData>`; its schema exercises
    /// the hand-written `JsonSchema for VecMap`, which delegates to the canonical
    /// `BTreeMap` map schema (a JSON object), matching how `VecMap` serializes.
    #[test]
    fn vec_map_field_is_an_object_schema() {
        let json = assert_schema_compiles!(SlotMapDyn);
        assert!(
            json.contains(r#""entries""#),
            "SlotMapDyn schema missing `entries`: {json}"
        );
        assert!(
            json.contains(r#""type":"object""#),
            "VecMap should be an object schema: {json}"
        );
    }

    /// `ControlLamp2d` has a custom `Serialize` that emits a 5-element tuple, so
    /// its schema must be a fixed-length array, not a named-field struct.
    #[test]
    fn control_lamp_is_a_fixed_length_tuple_schema() {
        let json = assert_schema_compiles!(ControlLamp2d);
        assert!(
            json.contains(r#""type":"array""#),
            "ControlLamp2d should be an array schema: {json}"
        );
        assert!(
            json.contains(r#""prefixItems""#),
            "ControlLamp2d should use prefixItems for its tuple: {json}"
        );
        assert!(
            json.contains(r#""maxItems":5"#) && json.contains(r#""minItems":5"#),
            "ControlLamp2d should be exactly 5 elements: {json}"
        );
        // The tuple form must not be described as a named-field object.
        assert!(
            !json.contains(r#""properties""#),
            "ControlLamp2d tuple schema must not expose named struct fields: {json}"
        );
    }

    /// `NodeInvocation` is not a serde type; its authored wire form is an
    /// externally-tagged enum, mirrored by the hand-written `JsonSchema`.
    #[test]
    fn node_invocation_is_externally_tagged_one_of() {
        let json = assert_schema_compiles!(NodeInvocation);
        assert!(
            json.contains(r#""oneOf""#),
            "NodeInvocation should be a oneOf: {json}"
        );
        for tag in [r#""unset""#, r#""ref""#, r#""def""#] {
            assert!(
                json.contains(tag),
                "NodeInvocation schema missing variant {tag}: {json}"
            );
        }
    }
}
