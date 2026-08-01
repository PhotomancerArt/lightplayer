//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lps-builtins-gen-app --manifest-path lp-shader/lps-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

//! LPIR library operations (fixed-point Q32).

#[cfg(feature = "float-f32")]
pub mod fadd_f32;
pub mod fadd_q32;
#[cfg(feature = "float-f32")]
pub mod fdiv_f32;
pub mod fdiv_q32;
#[cfg(feature = "float-f32")]
pub mod fdiv_recip_f32;
pub mod fdiv_recip_q32;
#[cfg(feature = "float-f32")]
pub mod float_misc_f32;
pub mod float_misc_q32;
#[cfg(feature = "float-f32")]
pub mod fmul_f32;
pub mod fmul_q32;
#[cfg(feature = "float-f32")]
pub mod fnearest_f32;
pub mod fnearest_q32;
#[cfg(feature = "float-f32")]
pub mod fsqrt_f32;
pub mod fsqrt_q32;
#[cfg(feature = "float-f32")]
pub mod fsub_f32;
pub mod fsub_q32;
#[cfg(feature = "float-f32")]
pub mod ftoi_sat_f32;
pub mod ftoi_sat_q32;
#[cfg(feature = "float-f32")]
pub mod itof_s_f32;
pub mod itof_s_q32;
#[cfg(feature = "float-f32")]
pub mod itof_u_f32;
pub mod itof_u_q32;
#[cfg(feature = "float-f32")]
pub mod unorm_conv_f32;
pub mod unorm_conv_q32;
