//! Q16.16 raw encoding/decoding for **compiler constant emission**.
//!
//! This module provides **raw `i32` encoding** (`f32`/`f64` → `i32` bits) for the codegen
//! path (e.g., `lpvm-cranelift`). These functions return raw fixed-point words suitable
//! for embedding in generated code (`iconst.i32`), not typed `Q32` values.
//!
//! ## Why a separate module?
//!
//! Codegen doesn't conceptually "create a Q32 instance"—it encodes floating-point
//! constants as raw fixed-point bits for the VM. Using `Q32::new(value).to_fixed()`
//! would be indirect and imply a typed value intermediate that isn't needed.
//!
//! ## Comparison: `q32_encode` vs `Q32::from_f32_wrapping`
//!
//! | Aspect | `q32_encode` (this module) | [`Q32::from_f32_wrapping`](crate::Q32::from_f32_wrapping) |
//! |--------|---------------------------|-----------------|
//! | Returns | `i32` (raw bits) | `Q32` (typed value) |
//! | Rounding | `libm::round` (nearest) | Truncate toward zero (`as i32`) |
//! | Out-of-range | **Saturate** to `0x7FFF_FFFF` / `i32::MIN` | **Wrap** (Rust `as` semantics) |
//! | Primary use | Compiler constant emission | Runtime conversion in builtins/engine |
//!
//! The saturation behavior ensures that a shader constant like `50000.0` becomes the
//! maximum representable Q32 value rather than wrapping to a negative. The rounding
//! gives slightly better accuracy for constants than truncation.
//!
//! For runtime conversions (e.g., inside builtin implementations), use [`Q32::from_f32_wrapping`]
//! which is faster (no rounding, no clamping) and matches the semantics of a cast in
//! generated code.

pub const Q32_SHIFT: i64 = 16;
const Q32_SCALE: f64 = 65536.0;
const Q32_MAX: i64 = 0x7FFF_FFFF;
const Q32_MIN: i64 = i32::MIN as i64;
pub const Q32_FRAC: i32 = (1 << Q32_SHIFT) - 1;

/// Encode an `f32` constant as raw Q16.16 bits for **compiler emission**.
///
/// Returns `i32` raw fixed-point bits (not a `Q32` value). Uses `libm::round` (not truncation)
/// and **saturates** to the Q16.16 representable range (`[i32::MIN, 0x7FFF_FFFF]`).
///
/// For constructing a runtime `Q32` value (e.g., in builtins), use [`Q32::from_f32_wrapping`](crate::Q32::from_f32_wrapping)
/// which truncates and wraps instead, matching the semantics of a cast in generated code.
///
/// # Example
/// ```
/// use lps_q32::q32_encode;
///
/// assert_eq!(q32_encode(1.5), 0x0001_8000);  // 98304 = 1.5 * 65536
/// assert_eq!(q32_encode(0.5), 0x0000_8000);  // 32768 = 0.5 * 65536 (rounded)
/// assert_eq!(q32_encode(50000.0), 0x7FFF_FFFF);  // saturated to max
/// ```
pub fn q32_encode(value: f32) -> i32 {
    q32_encode_f64(f64::from(value))
}

/// Encode an `f64` constant as raw Q16.16 bits for **compiler emission**.
///
/// `f64` version of [`q32_encode`] with rounding and saturation. Returns `i32` raw
/// fixed-point bits suitable for embedding in generated code. Used for Level-1 call
/// interchange where higher precision intermediate is needed.
pub fn q32_encode_f64(value: f64) -> i32 {
    let scaled = libm::round(value * Q32_SCALE);
    if scaled > Q32_MAX as f64 {
        Q32_MAX as i32
    } else if scaled < Q32_MIN as f64 {
        i32::MIN
    } else {
        scaled as i32
    }
}

/// Decode Q16.16 fixed-point to `f64`.
///
/// For the reverse operation (encoding), see [`q32_encode`] or [`Q32::to_f32`](crate::q32::Q32::to_f32).
pub fn q32_to_f64(raw: i32) -> f64 {
    f64::from(raw) / Q32_SCALE
}
