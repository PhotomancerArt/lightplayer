//! Backend-specific compile options (not shared with Cranelift / WASM).

/// Options for LPIR → native RV32 codegen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCompileOptions {
    pub float_mode: lpir::FloatMode,

    /// When true, emission records LPIR op indices per instruction (for disassembly / future DWARF).
    pub debug_info: bool,

    /// When the `emu` feature is enabled: use per-instruction logging in lp-riscv-emu so failures
    /// can include [`Riscv32Emulator::format_logs`] / execution history in debug dumps.
    pub emu_trace_instructions: bool,

    /// When true, print register-allocation trace (liveness + assignments) during codegen.
    /// Off by default for production and normal test runs.
    pub alloc_trace: bool,

    /// When true, log backend compile stage transitions and function indices.
    /// Intended for targeted profiling and stress harnesses.
    pub stage_trace: bool,

    /// When true (default), insert fuel checks: check-then-decrement at every
    /// loop back-edge and check-only at every function entry. Always on for
    /// device codegen and filetest targets; `false` exists for tests and perf
    /// comparison only.
    pub fuel: bool,

    /// Middle-end LPIR pass settings (inline, etc.).
    pub config: lpir::CompilerConfig,
}

impl Default for NativeCompileOptions {
    fn default() -> Self {
        Self {
            float_mode: lpir::FloatMode::Q32,
            debug_info: false,
            emu_trace_instructions: false,
            alloc_trace: false,
            stage_trace: false,
            fuel: true,
            config: lpir::CompilerConfig::default(),
        }
    }
}
