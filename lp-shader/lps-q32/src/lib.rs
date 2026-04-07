#![no_std]

//! Fixed-point Q16.16 types, helpers, and encode/decode for the LightPlayer shader stack.

mod lpir;

pub mod fns;
pub mod q32_encode;
pub mod q32_options;
pub mod types;

pub use q32_encode::{Q32_FRAC, Q32_SHIFT, q32_encode, q32_encode_f64, q32_to_f64};
pub use q32_options::{AddSubMode, DivMode, MulMode, Q32Options};
pub use types::mat2_q32::Mat2Q32;
pub use types::mat3_q32::Mat3Q32;
pub use types::mat4_q32::Mat4Q32;
pub use types::q32::{Q32, ToQ32, ToQ32Clamped};
pub use types::vec2_q32::Vec2Q32;
pub use types::vec3_q32::Vec3Q32;
pub use types::vec4_q32::Vec4Q32;
