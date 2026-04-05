//! GLSL to WebAssembly: Naga GLSL frontend + LPIR + WASM emission (Stage V).

#![no_std]

extern crate alloc;

mod emit;
pub mod module;
pub mod options;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

pub use lp_glsl_naga::{CompileError, FloatMode, LpsType};
pub use module::{
    glsl_type_to_wasm_components, WasmExport, WasmModule, SHADOW_STACK_GLOBAL_EXPORT,
};
pub use options::WasmOptions;

use lp_glsl_naga::NagaModule;
use lpir::IrModule;

/// Full pipeline error (parse/metadata from [`lp_glsl_naga`], lowering, or WASM emission).
#[derive(Debug)]
pub enum GlslWasmError {
    Frontend(CompileError),
    Codegen(String),
}

impl fmt::Display for GlslWasmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frontend(e) => write!(f, "{e}"),
            Self::Codegen(s) => write!(f, "{s}"),
        }
    }
}

impl core::error::Error for GlslWasmError {}

impl From<CompileError> for GlslWasmError {
    fn from(e: CompileError) -> Self {
        Self::Frontend(e)
    }
}

/// Compile GLSL source to a WASM module (Naga → LPIR → WASM).
pub fn glsl_wasm(source: &str, options: WasmOptions) -> Result<WasmModule, GlslWasmError> {
    let naga_module = lp_glsl_naga::compile(source)?;
    let (ir_module, _) = lp_glsl_naga::lower(&naga_module)
        .map_err(|e| GlslWasmError::Codegen(alloc::format!("{e}")))?;
    let (wasm_bytes, shadow_stack_base) =
        emit::emit_module(&ir_module, &options).map_err(GlslWasmError::Codegen)?;
    let exports = collect_exports(&ir_module, &naga_module, &options);
    Ok(WasmModule {
        bytes: wasm_bytes,
        exports,
        shadow_stack_base,
    })
}

fn collect_exports(
    ir: &IrModule,
    naga_module: &NagaModule,
    options: &WasmOptions,
) -> Vec<WasmExport> {
    debug_assert_eq!(
        ir.functions.len(),
        naga_module.functions.len(),
        "LPIR and Naga should export the same functions in the same order"
    );
    ir.functions
        .iter()
        .zip(naga_module.functions.iter())
        .map(|(ir_f, (_, fi))| {
            // Build params with VMContext (i32) as first element, followed by user params.
            // This matches the WASM function signature which includes VMContext.
            let mut params: Vec<_> = alloc::vec![module::WasmValType::I32]; // VMContext
            params.extend(
                fi.params
                    .iter()
                    .flat_map(|p| module::glsl_type_to_wasm_components(&p.ty, options.float_mode)),
            );
            let results = module::glsl_type_to_wasm_components(&fi.return_type, options.float_mode);
            WasmExport {
                name: ir_f.name.clone(),
                params,
                results,
                return_type: fi.return_type.clone(),
                // param_types is user-visible params only (for test harness arg validation)
                param_types: fi.params.iter().map(|p| p.ty.clone()).collect(),
            }
        })
        .collect()
}
