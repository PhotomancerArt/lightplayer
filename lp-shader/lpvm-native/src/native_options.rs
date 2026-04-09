//! Backend-specific compile options (not shared with Cranelift / WASM).

/// Options for LPIR → native RV32 codegen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeCompileOptions {
    pub float_mode: lpir::FloatMode,
    /// When true, emission records LPIR op indices per instruction (for disassembly / future DWARF).
    pub debug_info: bool,
    /// When the `emu` feature is enabled: use per-instruction logging in lp-riscv-emu so failures
    /// can include [`Riscv32Emulator::format_logs`] / execution history in debug dumps.
    pub emu_trace_instructions: bool,
}

impl Default for NativeCompileOptions {
    fn default() -> Self {
        Self {
            float_mode: lpir::FloatMode::Q32,
            debug_info: false,
            emu_trace_instructions: false,
        }
    }
}
