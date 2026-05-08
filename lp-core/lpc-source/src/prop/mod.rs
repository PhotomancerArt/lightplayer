pub mod kind_defaults;
pub mod src_binding;
pub mod src_shape;
pub mod src_texture_spec;
pub mod src_value_spec;
pub mod src_value_spec_wire;

mod toml_color;
mod toml_parse;

pub use kind_defaults::kind_default_presentation;
pub use src_binding::{BindingResolver, SrcBinding};
pub use src_shape::{SrcShape, SrcSlot};
pub use src_texture_spec::SrcTextureSpec;
pub use src_value_spec::{FromTomlError, LoadCtx, SrcValueSpec};
