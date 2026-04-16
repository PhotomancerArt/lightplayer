//! Shared compile options for JIT and object emission.

use lpir::FloatMode;

use lps_q32::q32_options::Q32Options;

/// Memory use strategy when lowering into a Cranelift module.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemoryStrategy {
    #[default]
    Default,
    LowMemory,
}

/// Options for LPIR → Cranelift compilation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    pub float_mode: FloatMode,
    pub q32_options: Q32Options,
    pub memory_strategy: MemoryStrategy,
    pub max_errors: Option<usize>,
    /// When true, the LPVM RV32 emulator enables instruction-level guest logging for debug dumps.
    /// Ignored by JIT and object-only compilation.
    pub emu_trace_instructions: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
            q32_options: Q32Options::default(),
            memory_strategy: MemoryStrategy::default(),
            max_errors: None,
            emu_trace_instructions: false,
        }
    }
}
