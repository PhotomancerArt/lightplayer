//! WASM code generation from typed GLSL AST.

pub mod context;
pub mod expr;
pub mod numeric;
pub mod stmt;

use alloc::vec::Vec;
use wasm_encoder::{CodeSection, ExportKind, ExportSection, FunctionSection, Module, TypeSection};

use crate::options::WasmOptions;
use crate::types::glsl_type_to_wasm;
use lp_glsl_frontend::semantic::{TypedFunction, TypedShader};

/// Compile typed shader to WASM bytes.
pub fn compile_to_wasm(
    shader: &TypedShader,
    options: &WasmOptions,
) -> Result<Vec<u8>, lp_glsl_frontend::error::GlslDiagnostics> {
    let mut module = Module::new();

    // Collect functions to compile: main first, then user functions
    let mut functions: Vec<&TypedFunction> = Vec::new();
    if let Some(ref main) = shader.main_function {
        functions.push(main);
    }
    functions.extend(shader.user_functions.iter());

    if functions.is_empty() {
        // Empty shader: produce minimal valid module
        return Ok(module.finish());
    }

    // Type section: one type per function (params + results)
    let mut types = TypeSection::new();
    for func in &functions {
        let params: Vec<_> = func
            .parameters
            .iter()
            .map(|p| glsl_type_to_wasm(&p.ty, options.decimal_format))
            .collect();
        let results: Vec<_> = if matches!(
            func.return_type,
            lp_glsl_frontend::semantic::types::Type::Void
        ) {
            Vec::new()
        } else {
            alloc::vec![glsl_type_to_wasm(&func.return_type, options.decimal_format)]
        };
        types.ty().function(params, results);
    }
    module.section(&types);

    // Function section: each function references its type by index
    let mut func_section = FunctionSection::new();
    for (i, _) in functions.iter().enumerate() {
        func_section.function(i as u32);
    }
    module.section(&func_section);

    // Export section
    let mut exports = ExportSection::new();
    for (i, func) in functions.iter().enumerate() {
        exports.export(&func.name, ExportKind::Func, i as u32);
    }
    module.section(&exports);

    // Code section: function bodies
    let mut codes = CodeSection::new();
    for func in &functions {
        let body = stmt::emit_function(func, options)?;
        codes.function(&body);
    }
    module.section(&codes);

    Ok(module.finish())
}
