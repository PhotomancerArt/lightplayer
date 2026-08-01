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

impl LpsCompileStats {
    pub(crate) fn from_module<M: LpvmModule>(fallback_ir: &LpirModule, module: &M) -> Self {
        let ir = module.lpir_module().unwrap_or(fallback_ir);
        Self {
            lpir_function_count: ir.functions.len(),
            lpir_import_count: ir.imports.len(),
            lpir_inst_count: count_lpir_insts(ir),
            final_inst_count: module.final_instruction_count(),
            final_code_size_bytes: module.code_size_bytes(),
            float_impl: module.float_impl(),
        }
    }
}

fn count_lpir_insts(ir: &LpirModule) -> usize {
    ir.functions
        .values()
        .map(|function| function.body.len())
        .sum()
}
