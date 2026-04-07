//! Q16.16 encoding/decoding for **compiler constant emission**.
//!
//! These functions are designed for the codegen path (e.g., `lpvm-cranelift`) to encode
//! `f32` shader constants as Q16.16 `i32` values embedded in generated code. They differ
//! from [`Q32::from_f32`](crate::types::q32::Q32::from_f32) in two important ways:
//!
//! | Aspect | `q32_encode` (this module) | `Q32::from_f32` |
//! |--------|---------------------------|-----------------|
//! | Rounding | `libm::round` (nearest) | Truncate toward zero (`as i32`) |
//! | Out-of-range | **Saturate** to `0x7FFF_FFFF` / `i32::MIN` | **Wrap** (Rust `as` semantics) |
//! | Primary use | Compiler constant emission | Runtime conversion in builtins/engine |
//!
//! The saturation behavior ensures that a shader constant like `50000.0` becomes the
//! maximum representable Q32 value rather than wrapping to a negative. The rounding
//! gives slightly better accuracy for constants than truncation.
//!
//! For runtime conversions (e.g., inside builtin implementations), use `Q32::from_f32`
//! which is faster (no rounding, no clamping) and matches the semantics of a cast in
//! generated code.

pub const Q32_SHIFT: i64 = 16;
const Q32_SCALE: f64 = 65536.0;
const Q32_MAX: i64 = 0x7FFF_FFFF;
const Q32_MIN: i64 = i32::MIN as i64;
pub const Q32_FRAC: i32 = (1 << Q32_SHIFT) - 1;

/// Encode an `f32` constant as Q16.16 for **compiler emission**.
///
/// Uses `libm::round` (not truncation) and **saturates** to the Q16.16 representable
/// range (`[i32::MIN, 0x7FFF_FFFF]`). For runtime conversions, use [`Q32::from_f32`](crate::types::q32::Q32::from_f32)
/// which truncates and wraps instead.
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

/// Encode `f64` as Q16.16 for **compiler emission** (with rounding and saturation).
///
/// See [`q32_encode`] for details. This is the `f64` version used for Level-1 call
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
/// For the reverse operation (encoding), see [`q32_encode`] or [`Q32::to_f32`](crate::types::q32::Q32::to_f32).
pub fn q32_to_f64(raw: i32) -> f64 {
    f64::from(raw) / Q32_SCALE
}
