//! `LpvmInstance` trait - execution state and function calling.

use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::{LpsValueF32, LpsValueQ32};

use crate::LpvmBuffer;

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

    /// Hot path: invoke a synthesised `__render_texture_<format>` entry by name.
    ///
    /// **Resolution and cache:** the backend resolves the export/symbol on the
    /// first `call_render_texture` for a given `fn_name`, stores the result in
    /// per-instance state, and reuses it on later frames — so after warmup,
    /// per-frame cost is one direct machine call (no repeated name resolution).
    ///
    /// **Signature checks:** backends validate the LPIR shape expected by this
    /// API (see [`crate::validate_render_texture_sig_ir`]) on **first**
    /// resolution only. The `lp-shader` crate's `LpsPxShader::new` also checks
    /// the corresponding [`lps_shared::LpsFnSig`] at construction time via
    /// module metadata.
    ///
    /// **Guest shape:** the synthesised function is defined in
    /// `lp-shader/lp-shader/src/synth/render_texture.rs` (Phase 3). In LPIR it is
    /// `(tex_ptr: Pointer, width: I32, height: I32) -> ()` with implicit `vmctx`
    /// in vreg 0 (see [`crate::validate_render_texture_sig_ir`]); lowered calls
    /// pass `vmctx` plus those parameters per backend convention.
    ///
    /// `texture` supplies both the host pointer (deprecated host Cranelift JIT)
    /// and the 32-bit guest base (RV32 / emu / Wasmtime); each backend uses the
    /// representation its calling convention requires.
    ///
    /// Returns the backend's existing `Error` type for missing symbol,
    /// signature mismatch, or guest trap.
    fn call_render_texture(
        &mut self,
        fn_name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error>;

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

    /// Guest cycle estimate for the last **successful** RV32 emulator call (per active
    /// cost model); JIT/WASM return `None`.
    fn last_guest_cycle_count(&self) -> Option<u64> {
        None
    }
}
