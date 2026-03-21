//! GLSL to WebAssembly: Naga GLSL frontend + stack-machine WASM emission.

#![no_std]

extern crate alloc;

mod emit;
mod emit_vec;
mod locals;
pub mod module;
pub mod options;
pub mod types;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

pub use lp_glsl_naga::{CompileError, FloatMode, GlslType};
pub use module::{WasmExport, WasmModule};
pub use options::WasmOptions;

use lp_glsl_naga::NagaModule;

/// Full pipeline error (parse/metadata from [`lp_glsl_naga`], or WASM lowering).
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

/// Compile GLSL source to a WASM module.
pub fn glsl_wasm(source: &str, options: WasmOptions) -> Result<WasmModule, GlslWasmError> {
    let naga_module = lp_glsl_naga::compile(source)?;
    let wasm_bytes = emit::emit_module(&naga_module, &options).map_err(GlslWasmError::Codegen)?;
    let exports = collect_exports(&naga_module, &options);
    Ok(WasmModule {
        bytes: wasm_bytes,
        exports,
    })
}

fn collect_exports(naga_module: &NagaModule, options: &WasmOptions) -> Vec<WasmExport> {
    naga_module
        .functions
        .iter()
        .map(|(_, fi)| {
            let params: Vec<_> = fi
                .params
                .iter()
                .flat_map(|(_, ty)| types::glsl_type_to_wasm_components(ty, options.float_mode))
                .collect();
            let results = types::glsl_type_to_wasm_components(&fi.return_type, options.float_mode);
            WasmExport {
                name: fi.name.clone(),
                params,
                results,
                return_type: fi.return_type.clone(),
                param_types: fi.params.iter().map(|(_, ty)| ty.clone()).collect(),
            }
        })
        .collect()
}
