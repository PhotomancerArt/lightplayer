//! WASM code generation from typed GLSL AST.

mod builtin_wasm_import_types;

pub mod builtin_scan;
pub mod context;
pub mod expr;
pub mod memory;
pub mod numeric;
pub mod rvalue;
pub mod stmt;

use alloc::vec::Vec;
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, FunctionSection, ImportSection, MemoryType,
    Module, TypeSection,
};

use crate::options::WasmOptions;
use crate::types::glsl_type_to_wasm_components;
use hashbrown::HashMap;
use lp_glsl_builtin_ids::BuiltinId;
use lp_glsl_frontend::FloatMode;
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

    let mut func_index_map_provisional = hashbrown::HashMap::new();
    let mut func_return_type = hashbrown::HashMap::new();
    for (i, f) in functions.iter().enumerate() {
        func_index_map_provisional.insert(f.name.clone(), i as u32);
        func_return_type.insert(f.name.clone(), f.return_type.clone());
    }

    let builtins_used = builtin_scan::scan_shader_for_builtin_imports(
        shader,
        options,
        &func_index_map_provisional,
        &func_return_type,
    )?;

    let mut types = TypeSection::new();

    let mut builtin_order: Vec<_> = builtins_used.iter().copied().collect();
    builtin_order.sort_by_key(|b| b.name());

    if options.float_mode == FloatMode::Q32 {
        for bid in &builtin_order {
            let (params, results) = builtin_wasm_import_types::wasm_import_val_types(*bid);
            types.ty().function(params, results);
        }
    }

    let builtin_type_count = builtin_order.len() as u32;

    for func in &functions {
        let params: Vec<_> = func
            .parameters
            .iter()
            .flat_map(|p| glsl_type_to_wasm_components(&p.ty, options.float_mode))
            .collect();
        let results: Vec<_> = if matches!(
            func.return_type,
            lp_glsl_frontend::semantic::types::Type::Void
        ) {
            Vec::new()
        } else {
            glsl_type_to_wasm_components(&func.return_type, options.float_mode)
        };
        types.ty().function(params, results);
    }
    module.section(&types);

    let import_fn_count = if options.float_mode == FloatMode::Q32 {
        builtin_order.len() as u32
    } else {
        0
    };

    let import_memory = options.float_mode == FloatMode::Q32 && !builtin_order.is_empty();

    if import_memory || import_fn_count > 0 {
        let mut imports = ImportSection::new();
        if import_memory {
            imports.import(
                "env",
                "memory",
                MemoryType {
                    minimum: 1,
                    maximum: None,
                    memory64: false,
                    shared: false,
                    page_size_log2: None,
                },
            );
        }
        if options.float_mode == FloatMode::Q32 {
            for (i, bid) in builtin_order.iter().enumerate() {
                imports.import("builtins", bid.name(), EntityType::Function(i as u32));
            }
        }
        module.section(&imports);
    }

    // Function section: defined functions only; type indices follow builtin types
    let mut func_section = FunctionSection::new();
    for (i, _) in functions.iter().enumerate() {
        func_section.function(builtin_type_count + i as u32);
    }
    module.section(&func_section);

    // Export section: function index = imported funcs + local index
    let mut exports = ExportSection::new();
    for (i, func) in functions.iter().enumerate() {
        exports.export(&func.name, ExportKind::Func, import_fn_count + i as u32);
    }
    module.section(&exports);

    let mut func_index_map = hashbrown::HashMap::new();
    for (i, f) in functions.iter().enumerate() {
        func_index_map.insert(f.name.clone(), import_fn_count + i as u32);
    }

    let mut builtin_func_index: HashMap<BuiltinId, u32> = HashMap::new();
    if options.float_mode == FloatMode::Q32 {
        for (i, bid) in builtin_order.iter().enumerate() {
            builtin_func_index.insert(*bid, i as u32);
        }
    }

    let mut codes = CodeSection::new();
    for func in &functions {
        let body = stmt::emit_function(
            func,
            options,
            &func_index_map,
            &builtin_func_index,
            &func_return_type,
        )?;
        codes.function(&body);
    }
    module.section(&codes);

    Ok(module.finish())
}
