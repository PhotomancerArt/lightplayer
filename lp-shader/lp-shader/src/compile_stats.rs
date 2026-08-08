use lpir::LpirModule;
use lpvm::LpvmModule;

/// How a compiled program actually performs its `float` arithmetic.
///
/// A *result* of compilation, not a request: the authored `float_mode` slot
/// says what the shader wants, and the backend reports through this what the
/// target could give it. Surfaced so a UI can disclose "soft float" without
/// guessing from the board (f32 roadmap D3).
///
/// Defined by `lpvm` alongside `LpvmModule`, because the compiled module is
/// the only thing that knows the answer, and re-exported here so this stays
/// the one place a consumer reads compile disclosure from.
pub use lpvm::FloatImpl;

/// Backend-agnostic statistics captured after shader compilation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LpsCompileStats {
    pub lpir_function_count: usize,
    pub lpir_import_count: usize,
    pub lpir_inst_count: usize,
    pub final_inst_count: Option<usize>,
    pub final_code_size_bytes: Option<usize>,
    /// Hardware-vs-soft float disclosure, reported by the compiled module
    /// itself (`LpvmModule::float_impl`) rather than assumed here.
    pub float_impl: FloatImpl,
}

/// Everything these stats need to know about the front-end LPIR module.
///
/// Three counters, taken while the module is still in hand. Compilation used
/// to keep the whole `LpirModule` alive alongside the backend job's copy just
/// so this summary could be computed after link; extracting it up front lets
/// the module be *moved* into the backend job instead (the jit path never gets
/// it back — [`LpvmModule::lpir_module`] is `None` there).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LpirModuleStats {
    function_count: usize,
    import_count: usize,
    inst_count: usize,
}

impl LpirModuleStats {
    pub(crate) fn from_ir(ir: &LpirModule) -> Self {
        Self {
            function_count: ir.functions.len(),
            import_count: ir.imports.len(),
            inst_count: ir
                .functions
                .values()
                .map(|function| function.body.len())
                .sum(),
        }
    }
}

impl LpsCompileStats {
    /// `fallback_ir` is the summary of the front-end module, used unless the
    /// backend retained its own LPIR (emu paths), which is then the more
    /// accurate answer because backend passes may have rewritten it.
    pub(crate) fn from_module<M: LpvmModule>(fallback_ir: LpirModuleStats, module: &M) -> Self {
        let ir = module
            .lpir_module()
            .map_or(fallback_ir, LpirModuleStats::from_ir);
        Self {
            lpir_function_count: ir.function_count,
            lpir_import_count: ir.import_count,
            lpir_inst_count: ir.inst_count,
            final_inst_count: module.final_instruction_count(),
            final_code_size_bytes: module.code_size_bytes(),
            float_impl: module.float_impl(),
        }
    }
}
