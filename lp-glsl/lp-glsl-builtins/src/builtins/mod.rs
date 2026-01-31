//! LightPlayer Effects (LPFX) library native implementation
//!
//! Native implementation of the lpfx library used when running shaders on the cpu, providing
//! optimized implementations which are linked against.
//!
//! Writing these in rust provides two main advantages:
//! - rust compiler produces more optimized code
//! - lower memory footprint when compiling
//!
//! ## Function Pattern
//!
//! LPFX functions follow a two-layer pattern:
//!
//! 1. **`lpfx_*`** - Public Rust functions with nice types (Q32, Vec3Q32, Vec4Q32)
//!    - Contains the actual implementation
//!    - Can be inlined when called from other Rust code
//!    - Allows ergonomic calls between lpfx functions (e.g., `hsv2rgb` can call `hue2rgb` and `saturate` with nice types)
//!
//! 2. **`__lpfx_*`** - Extern C wrappers with expanded types (i32, flattened vectors)
//!    - Wraps the `lpfx_*` function for compiler/GLSL calls
//!    - Takes expanded types: Q32 becomes i32, Vec3Q32 becomes three i32 parameters
//!    - Has `#[lpfx_impl_macro::lpfx_impl]` annotation for auto-registration
//!    - Has `#[unsafe(no_mangle)]` and `pub extern "C"` attributes
//!
//! Example:
//! ```rust
//! use lp_glsl_builtins::glsl::q32::types::q32::Q32;
//!
//! // Public Rust API - can be inlined
//! #[inline(always)]
//! pub fn lpfx_saturate_q32(value: Q32) -> Q32 {
//!     // Actual implementation
//!     value.max(Q32::ZERO).min(Q32::ONE)
//! }
//!
//! // Extern C wrapper for compiler
//! #[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_saturate(float x)")]
//! #[unsafe(no_mangle)]
//! pub extern "C" fn __lpfx_saturate_q32(value: i32) -> i32 {
//!     lpfx_saturate_q32(Q32::from_fixed(value)).to_fixed()
//! }
//! ```
//!
//! This pattern allows:
//! - Other Rust code to call lpfx functions with nice types and get inlining benefits
//! - The compiler to call lpfx functions via the extern C interface with expanded types
//! - LPFX functions to call each other ergonomically without type conversions

pub mod lpfx;
pub mod q32;
