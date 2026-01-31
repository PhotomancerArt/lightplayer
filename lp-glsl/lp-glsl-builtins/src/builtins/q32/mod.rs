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

pub use acos::__lp_q32_acos;
pub use acosh::__lp_q32_acosh;
pub use add::__lp_q32_add;
pub use asin::__lp_q32_asin;
pub use asinh::__lp_q32_asinh;
pub use atan::__lp_q32_atan;
pub use atan2::__lp_q32_atan2;
pub use atanh::__lp_q32_atanh;
pub use cos::__lp_q32_cos;
pub use cosh::__lp_q32_cosh;
pub use div::__lp_q32_div;
pub use exp::__lp_q32_exp;
pub use exp2::__lp_q32_exp2;
pub use fma::__lp_q32_fma;
pub use inversesqrt::__lp_q32_inversesqrt;
pub use ldexp::__lp_q32_ldexp;
pub use log::__lp_q32_log;
pub use log2::__lp_q32_log2;
pub use mod_builtin::__lp_q32_mod;
pub use mul::__lp_q32_mul;
pub use pow::__lp_q32_pow;
pub use round::__lp_q32_round;
pub use roundeven::__lp_q32_roundeven;
pub use sin::__lp_q32_sin;
pub use sinh::__lp_q32_sinh;
pub use sqrt::__lp_q32_sqrt;
pub use sub::__lp_q32_sub;
pub use tan::__lp_q32_tan;
pub use tanh::__lp_q32_tanh;
