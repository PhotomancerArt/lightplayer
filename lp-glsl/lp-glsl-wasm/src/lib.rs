//! GLSL to WebAssembly code generation.
//!
//! Compiles GLSL shaders to WASM modules. Uses lp-glsl-frontend for parsing
//! and semantic analysis; no Cranelift dependency.

#![no_std]

extern crate alloc;

pub mod codegen;
pub mod module;
pub mod options;
pub mod types;

pub use lp_glsl_frontend::{CompilationPipeline, DEFAULT_MAX_ERRORS, FloatMode};
pub use module::{WasmExport, WasmModule, WasmValType};
pub use options::WasmOptions;

use crate::codegen::compile_to_wasm;
use lp_glsl_frontend::error::GlslDiagnostics;

/// Compile GLSL source to a WASM module.
pub fn glsl_wasm(source: &str, options: WasmOptions) -> Result<WasmModule, GlslDiagnostics> {
    let semantic = CompilationPipeline::parse_and_analyze(source, options.max_errors)?;
    let wasm_bytes = compile_to_wasm(&semantic.typed_ast, &options)?;
    let exports = collect_exports(&semantic.typed_ast, &options);
    Ok(WasmModule {
        bytes: wasm_bytes,
        exports,
    })
}

fn collect_exports(
    shader: &lp_glsl_frontend::semantic::TypedShader,
    options: &WasmOptions,
) -> alloc::vec::Vec<WasmExport> {
    use crate::types::glsl_type_to_wasm;
    use alloc::vec::Vec;
    use lp_glsl_frontend::semantic::types::Type;

    let mut out = Vec::new();

    if let Some(ref main) = shader.main_function {
        out.push(WasmExport {
            name: main.name.clone(),
            params: main
                .parameters
                .iter()
                .map(|p| glsl_type_to_wasm(&p.ty, options.float_mode))
                .collect(),
            results: if matches!(
                main.return_type,
                lp_glsl_frontend::semantic::types::Type::Void
            ) {
                Vec::new()
            } else {
                alloc::vec![glsl_type_to_wasm(&main.return_type, options.float_mode)]
            },
            signature: lp_glsl_frontend::semantic::functions::FunctionSignature {
                name: main.name.clone(),
                return_type: main.return_type.clone(),
                parameters: main.parameters.clone(),
            },
        });
    }

    for f in &shader.user_functions {
        out.push(WasmExport {
            name: f.name.clone(),
            params: f
                .parameters
                .iter()
                .map(|p| glsl_type_to_wasm(&p.ty, options.float_mode))
                .collect(),
            results: if matches!(f.return_type, Type::Void) {
                Vec::new()
            } else {
                alloc::vec![glsl_type_to_wasm(&f.return_type, options.float_mode)]
            },
            signature: lp_glsl_frontend::semantic::functions::FunctionSignature {
                name: f.name.clone(),
                return_type: f.return_type.clone(),
                parameters: f.parameters.clone(),
            },
        });
    }

    out
}
