//! `LpvmInstance` trait - execution state and function calling.

use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::{LpsValueF32, LpsValueQ32};

/// An execution instance with mutable state.
///
/// Instances hold execution context: fuel, globals, uniforms, and any
/// backend-specific state (wasmtime Store, emulator, etc.).
///
/// The `call` method provides semantic function calling by name with
/// marshaled `LpsValue` arguments and returns.
///
/// # Limitations
///
/// The semantic `call` API does **not** support calling functions with
/// `out` or `inout` parameters directly. Such functions must be wrapped
/// in a plain function that returns values normally. Attempting to call
/// a function with `out`/`inout` parameters should produce a clear error:
/// "out/inout parameters are not supported for direct calling."
///
/// Q32 filetests and embedded callers may use [`LpvmInstance::call_q32`] with
/// pre-flattened `i32` words; layout matches concatenating [`crate::flatten_q32_arg`]
/// for each `in` parameter.
///
/// # Uniforms
///
/// [`LpvmInstance::set_uniform`] sets one uniform field by path (e.g. `u_time`,
/// `touch_input.touches[0].x`) using [`LpsValueF32`], matching [`LpvmInstance::call`] semantics.
///
/// [`LpvmInstance::set_uniform_q32`] sets the same using pre-encoded [`LpsValueQ32`] words,
/// matching [`LpvmInstance::call_q32`]. Encoding follows [`crate::LpsModuleSig`] std430 layout.
pub trait LpvmInstance {
    /// Error type for execution failures (trap, unknown function, type mismatch).
    type Error: core::fmt::Display;

    /// Call a function by name with marshaled arguments.
    ///
    /// The function must be declared in the module with `in` parameters only.
    /// `out`/`inout` parameters are not supported — see module-level docs.
    ///
    /// Arguments are marshaled from `LpsValueF32` to the backend's native
    /// representation. Results are unmarshaled back to `LpsValueF32`.
    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error>;

    /// Call with flattened Q32 ABI words: all parameter lanes concatenated in order.
    ///
    /// Return value is flattened the same way as [`crate::flatten_q32_return`] for the
    /// function’s logical return type. For `void`, returns an empty vector.
    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error>;

    /// Set a uniform by dot/bracket path with F32-level values (encoded per backend float mode).
    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error>;

    /// Set a uniform by path with Q32-encoded values (raw fixed-point lanes).
    fn set_uniform_q32(&mut self, path: &str, value: &LpsValueQ32) -> Result<(), Self::Error>;

    /// Optional backend diagnostics (e.g. emulator registers); `None` if unavailable.
    fn debug_state(&self) -> Option<String> {
        None
    }

    /// Guest instructions executed for the last **successful** call (e.g. RV32 emulator
    /// `call_function` body only, after per-call reset). JIT/WASM typically return `None`.
    fn last_guest_instruction_count(&self) -> Option<u64> {
        None
    }
}
