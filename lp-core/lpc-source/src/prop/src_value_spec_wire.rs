//! Private serde wire shape for [`crate::prop::src_value_spec::SrcValueSpec`].

use lpc_model::ModelValue;

use crate::prop::src_texture_spec::SrcTextureSpec;

// Internally-tagged `SrcValueSpec` for serde/JsonSchema; public API is `SrcValueSpec`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(crate) enum SrcValueSpecWire {
    Literal(ModelValue),
    Texture(SrcTextureSpec),
}

/// Delegates to [`SrcValueSpecWire`]'s `JsonSchema` impl so recursive [`crate::prop::src_shape::SrcShape`] / [`crate::prop::src_shape::SrcSlot`]
/// can derive schemas without exposing the wire type.
#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for crate::prop::src_value_spec::SrcValueSpec {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        <SrcValueSpecWire as schemars::JsonSchema>::schema_name()
    }

    fn schema_id() -> alloc::borrow::Cow<'static, str> {
        <SrcValueSpecWire as schemars::JsonSchema>::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <SrcValueSpecWire as schemars::JsonSchema>::json_schema(generator)
    }
}
