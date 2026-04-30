//! Author-time **texture default** recipe (`SrcTextureSpec`).

use alloc::string::String;
use alloc::vec;
use lpc_model::ModelValue;

use crate::prop::src_value_spec::LoadCtx;

/// Recipe to build a default **texture** when author-time data is not a raw
/// handle. M2 defines only a universal 1×1 black (`quantity.md` §7).
///
/// The lpfx render MVP is expected to extend this recipe space for generated
/// image resources such as palette/gradient strips: TOML should preserve the
/// authoring recipe, while the runtime bakes width-by-one textures for shader
/// `sampler2D` uniforms.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SrcTextureSpec {
    /// 1×1 fully opaque black: the universal “no texture” default
    /// (`docs/design/lightplayer/quantity.md` §7).
    Black,
}

impl SrcTextureSpec {
    /// Handle-shaped [`ModelValue`] struct for [`Kind::Texture`] storage (`quantity.md` §3).
    pub fn default_model_value(&self, ctx: &mut LoadCtx) -> ModelValue {
        match self {
            Self::Black => texture_model_handle_value(ctx, 0, 1, 1),
        }
    }
}

fn texture_model_handle_value(
    ctx: &mut LoadCtx,
    format: i32,
    width: i32,
    height: i32,
) -> ModelValue {
    let handle = ctx.next_texture_handle;
    ModelValue::Struct {
        name: None,
        fields: vec![
            (String::from("format"), ModelValue::I32(format)),
            (String::from("width"), ModelValue::I32(width)),
            (String::from("height"), ModelValue::I32(height)),
            (String::from("handle"), ModelValue::I32(handle)),
        ],
    }
}
