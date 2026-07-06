//! Dev-time `SlotShape` → JSON Schema (draft 2020-12) compiler.
//!
//! The slot codec parses artifact JSON against runtime [`SlotShape`] trees, so
//! a schema *compiled from those shapes* cannot silently diverge from the
//! parser the way a parallel schemars derive could. This module walks a shape
//! (resolving [`SlotShape::Ref`] through the [`SlotShapeRegistry`]) and emits a
//! `serde_json::Value` schema whose accept set mirrors what
//! `slot_codec::dynamic_slot_reader` / `slot_codec::slot_value_codec` actually
//! accept.
//!
//! # Fidelity contract
//!
//! The compiled schema tracks the *reader*, with a strict direction guarantee:
//! any JSON the codec accepts must validate against the schema. In the other
//! direction the schema is as tight as JSON Schema can express; the handful of
//! codec strictnesses that cannot be expressed are documented inline where
//! they occur and surface as `description` text rather than silent
//! divergence. Known expressiveness gaps:
//!
//! - Discriminator ordering: `expect_discriminator` requires the tag to be the
//!   *first* object property; JSON Schema has no property-order vocabulary.
//! - Integer-ness: the codec parses number *text* (`"1.0"` fails
//!   `parse::<u32>()`), while JSON Schema's `integer` type accepts any number
//!   with zero fractional part (`1.0`).
//! - Numeric map keys: range overflow (`"99999999999"`) fails Rust `parse` but
//!   fits the `propertyNames` digit pattern.
//! - Value-level parse validation that inspects string *content* beyond a
//!   regular pattern (e.g. `ArtifactSpec::parse` of `lib:` suffixes, the
//!   affine bottom-row epsilon check).
//!
//! # Custom-codec side table
//!
//! Semantic leaves do not describe their own syntax: a [`SlotShape::Custom`]
//! delegates to a hand-written codec keyed by `codec` id, and a
//! [`SlotShape::Value`] whose `SlotValueShape::id` names a semantic type (e.g.
//! `ColorOrderValue`) gains `FromLpValue` validation on the typed read path.
//! Rather than adding schema methods to runtime slot traits (which would put
//! host-tooling surface on the firmware path), this module keeps a side table
//! `SlotShapeId → fn() -> serde_json::Value` in
//! [`custom_codec_schemas`](self::custom_codec_schema) populated by reading
//! each codec's read/write impl in `crate::slots`. Unknown ids fall back to
//! compiling the inner/declared shape. `NodeInvocation` needs no entry: it is
//! an ordinary externally-tagged [`SlotShape::Enum`], covered by the generic
//! enum compiler.
//!
//! Scope note: CLI wiring, file output, and output determinism belong to the
//! schema tooling phase that consumes this module; this module only compiles
//! shapes and proves conformance in its unit tests.

mod custom_codec_schemas;
mod slot_shape_schema;

pub use custom_codec_schemas::custom_codec_schema;
pub use slot_shape_schema::{compile_registered_slot_shape_schema, compile_slot_shape_schema};

/// Shared accept/reject conformance harness for this module's unit tests.
///
/// For every case it asserts the two directions the module promises: JSON the
/// codec reads successfully validates against the compiled schema, and JSON
/// the codec rejects fails it. It also checks the compiled schema against the
/// draft 2020-12 meta-schema so structural schema bugs fail loudly instead of
/// vacuously accepting everything.
#[cfg(test)]
pub(crate) mod test_support {
    use crate::slot_codec::{JsonSyntaxSource, SlotReader, SyntaxError, apply_reader_to_slot};
    use crate::{SlotDataMutAccess, SlotShape, SlotShapeRegistry};

    use super::compile_slot_shape_schema;

    pub(crate) fn check_conformance(
        registry: &SlotShapeRegistry,
        shape: &SlotShape,
        mut read: impl FnMut(&str) -> Result<(), SyntaxError>,
        accepted: &[&str],
        rejected: &[&str],
    ) {
        let schema = compile_slot_shape_schema(registry, shape);
        jsonschema::meta::validate(&schema)
            .unwrap_or_else(|error| panic!("schema is not valid draft 2020-12: {error}\n{schema}"));
        let validator = jsonschema::draft202012::new(&schema).expect("build validator");

        for text in accepted {
            if let Err(error) = read(text) {
                panic!("codec rejected expected-accept input {text}: {error}");
            }
            let instance: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
            assert!(
                validator.is_valid(&instance),
                "schema rejected codec-accepted input {text}\nschema: {schema}"
            );
        }
        for text in rejected {
            assert!(
                read(text).is_err(),
                "codec accepted expected-reject input {text}"
            );
            let instance: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
            assert!(
                !validator.is_valid(&instance),
                "schema accepted codec-rejected input {text}\nschema: {schema}"
            );
        }
    }

    /// Read one JSON text into an already-constructed *typed* slot.
    ///
    /// This is the path real artifact loads take (typed `ValueSlot<T>` /
    /// `EnumSlot<T>` fields), which is what routes value leaves through
    /// `FromLpValue` validation. The pure-dynamic registry path stores plain
    /// `LpValue`s and skips that validation, so semantic leaf conformance must
    /// be tested against typed slots.
    pub(crate) fn typed_read(
        data: SlotDataMutAccess<'_>,
        shape: &SlotShape,
        text: &str,
    ) -> Result<(), SyntaxError> {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(JsonSyntaxSource::new(text)?, &registry);
        apply_reader_to_slot(data, shape, &registry, reader.value())
    }
}
