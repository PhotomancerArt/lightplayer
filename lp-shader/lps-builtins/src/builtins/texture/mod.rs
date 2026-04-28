//! Reference sampling math and texture sampler `extern "C"` entry points.

mod sampler_helpers;

pub use sampler_helpers::{Texture1dUnormSampleArgs, Texture2dUnormSampleArgs};

pub mod r16_unorm_q32;
pub mod rgba16_unorm_q32;
pub mod sample_ref;

pub use sample_ref::{
    LinearAxis, linear_indices_q32, nearest_index_height_one_q32, nearest_index_q32,
    texel_center_coord_q32, wrap_coord,
};
