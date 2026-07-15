//! Engine ↔ shader-runtime ABI glue.
//!
//! The graphics backend seam itself lives in the `lp-gfx` crate family
//! (traits + handles) with the CPU implementation in `lp-gfx-lpvm`. What
//! remains here is the engine-side mapping between authored model
//! definitions/values and the `lp-shader` ABI: compute descriptors, type and
//! value conversions, and the visual uniform block.

pub mod compute_desc;
pub mod convert_type;
pub mod convert_value;
pub(crate) mod uniforms;

pub use compute_desc::{ComputeDescError, compute_desc_from_model_def};
pub use convert_type::model_type_to_lps_type;
pub use convert_value::{LpsValueToModelConversionError, lps_value_f32_to_model_value};
