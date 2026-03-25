//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

//! Fixed-point 16.16 arithmetic builtins.
//!
//! Functions operate on i32 values representing fixed-point numbers
//! with 16 bits of fractional precision.

mod acos;
mod acosh;
mod add;
mod asin;
mod asinh;
mod atan;
mod atan2;
mod atanh;
mod cos;
mod cosh;
mod div;
mod exp;
mod exp2;
mod fma;
mod inversesqrt;
mod ldexp;
mod log;
mod log2;
mod mod_builtin;
mod mul;
mod pow;
mod round;
mod roundeven;
mod sin;
mod sinh;
mod sqrt;
mod sub;
mod tan;
mod tanh;

pub use acos::__lp_glsl_acos_q32;
pub use acosh::__lp_glsl_acosh_q32;
pub use add::__lp_lpir_fadd_q32;
pub use asin::__lp_glsl_asin_q32;
pub use asinh::__lp_glsl_asinh_q32;
pub use atan::__lp_glsl_atan_q32;
pub use atan2::__lp_glsl_atan2_q32;
pub use atanh::__lp_glsl_atanh_q32;
pub use cos::__lp_glsl_cos_q32;
pub use cosh::__lp_glsl_cosh_q32;
pub use div::__lp_lpir_fdiv_q32;
pub use exp::__lp_glsl_exp_q32;
pub use exp2::__lp_glsl_exp2_q32;
pub use fma::__lp_glsl_fma_q32;
pub use inversesqrt::__lp_glsl_inversesqrt_q32;
pub use ldexp::__lp_glsl_ldexp_q32;
pub use log::__lp_glsl_log_q32;
pub use log2::__lp_glsl_log2_q32;
pub use mod_builtin::__lp_glsl_mod_q32;
pub use mul::__lp_lpir_fmul_q32;
pub use pow::__lp_glsl_pow_q32;
pub use round::__lp_glsl_round_q32;
pub use roundeven::__lp_lpir_fnearest_q32;
pub use sin::__lp_glsl_sin_q32;
pub use sinh::__lp_glsl_sinh_q32;
pub use sqrt::__lp_lpir_fsqrt_q32;
pub use sub::__lp_lpir_fsub_q32;
pub use tan::__lp_glsl_tan_q32;
pub use tanh::__lp_glsl_tanh_q32;
