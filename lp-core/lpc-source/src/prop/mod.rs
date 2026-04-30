pub mod kind_defaults;
pub mod src_binding;
pub mod src_shape;
pub mod src_texture_spec;
pub mod src_value_spec;
pub mod src_value_spec_wire;

mod toml_color;
mod toml_parse;

pub mod binding {
    //! Historical `lpc_model::prop::binding` path.
    pub use super::src_binding::*;
    pub use crate::Binding;
}
pub mod shape {
    //! Historical `lpc_model::prop::shape` path.
    pub use super::src_shape::*;
    pub use crate::{Shape, Slot};
}
pub mod value_spec {
    //! Historical `lpc_model::value_spec` path.
    pub use super::src_texture_spec::SrcTextureSpec;
    pub use super::src_value_spec::*;
}

pub use kind_defaults::{kind_default_bind, kind_default_presentation};
pub use src_binding::{BindingResolver, SrcBinding};
pub use src_shape::{SrcShape, SrcSlot};
pub use src_texture_spec::SrcTextureSpec;
pub use src_value_spec::{FromTomlError, LoadCtx, SrcValueSpec};
