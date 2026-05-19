use lpir::LpirModule;
use lpvm::LpvmModule;

/// Backend-agnostic statistics captured after shader compilation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LpsCompileStats {
    pub lpir_function_count: usize,
    pub lpir_import_count: usize,
    pub lpir_inst_count: usize,
    pub final_inst_count: Option<usize>,
    pub final_code_size_bytes: Option<usize>,
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
        }
    }
}

fn count_lpir_insts(ir: &LpirModule) -> usize {
    ir.functions
        .values()
        .map(|function| function.body.len())
        .sum()
}
