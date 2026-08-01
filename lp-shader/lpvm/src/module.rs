//! `LpvmModule` trait - compiled artifact with metadata.

use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::debug::ModuleDebugInfo;
use crate::instance::LpvmInstance;

/// How a compiled module's float arithmetic is **actually** implemented.
///
/// A *result* of compilation, never a request (f32 roadmap D3). The request is
/// `FloatMode`, which says what the shader means; this says what the backend
/// managed to emit for it. The two can differ legitimately — an `F32` module
/// compiled for a part without an FPU is [`Self::SoftF32`] — and the
/// difference is a ~30x performance fact a shader author is entitled to see.
///
/// It lives here, next to [`LpvmModule`], rather than in the stats type that
/// reports it, because the module is the only thing that knows the answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FloatImpl {
    /// Q16.16 fixed point on integer instructions.
    #[default]
    Fixed,
    /// IEEE f32 executed by a hardware FPU.
    HardwareF32,
    /// IEEE f32 emulated by soft-float library calls.
    SoftF32,
}

impl FloatImpl {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::HardwareF32 => "hardware-f32",
            Self::SoftF32 => "soft-f32",
        }
    }
}

/// A compiled shader module that can be instantiated for execution.
///
/// Modules are immutable after compilation. The `signatures()` method
/// provides access to function signatures for type checking and call
/// marshaling. Multiple instances can be created from one module,
/// each with independent execution state.
pub trait LpvmModule {
    /// Instance type produced by this module.
    type Instance: LpvmInstance;

    /// Error type for instantiation failures.
    type Error: core::fmt::Display;

    /// Get the function signatures for this module.
    fn signatures(&self) -> &LpsModuleSig;

    /// Create a new execution instance.
    ///
    /// The instance has independent VM state (fuel, globals, uniforms).
    /// Multiple instances can execute concurrently (subject to `Send` bounds).
    fn instantiate(&self) -> Result<Self::Instance, Self::Error>;

    /// Compilation debug info. Returns None if not available for this backend.
    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        None
    }

    /// LPIR this module was compiled from, when the backend retains it (RV32 emu paths).
    fn lpir_module(&self) -> Option<&LpirModule> {
        None
    }

    /// Final emitted code size in bytes, when the backend exposes a compact code artifact.
    fn code_size_bytes(&self) -> Option<usize> {
        None
    }

    /// Final emitted instruction count, when the backend can report one.
    fn final_instruction_count(&self) -> Option<usize> {
        self.debug_info()
            .map(|debug| debug.functions.values().map(|func| func.inst_count).sum())
    }

    /// How this module's float arithmetic ended up being implemented.
    ///
    /// Defaults to [`FloatImpl::Fixed`], which is the honest answer for every
    /// backend that only compiles `FloatMode::Q32` — a Q16.16 `float` *is* an
    /// integer and lowers to integer instructions. Backends that can emit
    /// hardware or soft float override this; overriding it is the whole
    /// mechanism behind the `float_impl` line in compile stats.
    fn float_impl(&self) -> FloatImpl {
        FloatImpl::Fixed
    }
}
