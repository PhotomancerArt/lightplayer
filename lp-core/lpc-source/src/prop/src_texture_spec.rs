//! Author-time **texture default** recipe (`SrcTextureSpec`).

use alloc::string::String;
use alloc::vec;
use lpc_model::LpValue;

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
    /// Handle-shaped [`LpValue`] struct for [`Kind::Texture`] storage (`quantity.md` §3).
    pub fn default_model_value(&self, ctx: &mut LoadCtx) -> LpValue {
        match self {
            Self::Black => texture_model_handle_value(ctx, 0, 1, 1),
        }
    }
}

fn texture_model_handle_value(ctx: &mut LoadCtx, format: i32, width: i32, height: i32) -> LpValue {
    let handle = ctx.next_texture_handle;
    LpValue::Struct {
        name: None,
        fields: vec![
            (String::from("format"), LpValue::I32(format)),
            (String::from("width"), LpValue::I32(width)),
            (String::from("height"), LpValue::I32(height)),
            (String::from("handle"), LpValue::I32(handle)),
        ],
    }
}
