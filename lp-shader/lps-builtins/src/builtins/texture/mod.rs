//! Reference sampling math and texture sampler `extern "C"` entry points.

mod sampler_helpers;

pub use sampler_helpers::{Texture1dUnormSampleArgs, Texture2dUnormSampleArgs};
#[cfg(feature = "float-f32")]
pub use sampler_helpers::{Texture1dUnormSampleArgsF32, Texture2dUnormSampleArgsF32};

pub mod r16_unorm_q32;
pub mod rgba16_unorm_q32;
pub mod sample_ref;

#[cfg(feature = "float-f32")]
pub mod r16_unorm_f32;
#[cfg(feature = "float-f32")]
pub mod rgba16_unorm_f32;
#[cfg(feature = "float-f32")]
pub mod sample_ref_f32;

pub use sample_ref::{
    LinearAxis, linear_indices_q32, nearest_index_height_one_q32, nearest_index_q32,
    texel_center_coord_q32, wrap_coord,
};

#[cfg(feature = "float-f32")]
pub use sample_ref_f32::{
    LinearAxisF32, linear_indices_f32, nearest_index_f32, nearest_index_height_one_f32,
    texel_center_coord_f32,
};
