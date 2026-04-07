#![no_std]

//! Fixed-point Q16.16 types, helpers, and encode/decode for the LightPlayer shader stack.

mod lpir;

pub mod fns;
pub mod mat2_q32;
pub mod mat3_q32;
pub mod mat4_q32;
pub mod q32;
pub mod q32_encode;
pub mod q32_options;
pub mod vec2_q32;
pub mod vec3_q32;
pub mod vec4_q32;

pub use mat2_q32::Mat2Q32;
pub use mat3_q32::Mat3Q32;
pub use mat4_q32::Mat4Q32;
pub use q32::Q32;
pub use q32_encode::{q32_encode, q32_encode_f64, q32_to_f64, Q32_FRAC, Q32_SHIFT};
pub use q32_options::{AddSubMode, DivMode, MulMode, Q32Options};
pub use vec2_q32::Vec2Q32;
pub use vec3_q32::Vec3Q32;
pub use vec4_q32::Vec4Q32;
