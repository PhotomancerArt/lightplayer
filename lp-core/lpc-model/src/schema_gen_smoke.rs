//! Compile-and-shape smoke tests that `schemars::schema_for!` succeeds and
//! produces the expected wire shape for representative model types. Gated on
//! `feature = "schema-gen"`, so `no_std`/default builds never see schemars.

#![cfg(feature = "schema-gen")]

#[cfg(test)]
mod tests {
    use crate::{
        ColorOrder, ControlLayout2d, ModuleDef, NodeInvocation, SlotMapDyn, SlotShapeRegistrySnapshot,
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
    fn schema_module_def() {
        assert_schema_compiles!(ModuleDef);
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

    /// `ControlLayout2d` serializes as the PACKED wire form (spans + base64
    /// centers), so its schema must describe that object — packing-span
    /// 5-tuples under `s`, a base64 string under `c` — and never the
    /// in-memory per-lamp vector.
    #[test]
    fn control_layout_schema_describes_the_packed_wire_form() {
        let json = assert_schema_compiles!(ControlLayout2d);
        assert!(
            json.contains(r#""s""#) && json.contains(r#""c""#),
            "ControlLayout2d schema should carry packed spans and centers: {json}"
        );
        assert!(
            json.contains(r#""maxItems":5"#) && json.contains(r#""minItems":5"#),
            "packing spans should be 5-element tuples: {json}"
        );
        assert!(
            !json.contains("lamp_index") && !json.contains(r#""lamps""#),
            "the in-memory lamp vector must not leak into the wire schema: {json}"
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
        for tag in [r#""unset""#, r#""ref""#] {
            assert!(
                json.contains(tag),
                "NodeInvocation schema missing variant {tag}: {json}"
            );
        }
    }
}
