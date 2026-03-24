//! LPIR → WASM emission (Stage V).

mod control;
mod func;
mod imports;
mod memory;
mod ops;
mod q32;

use alloc::string::String;
use alloc::vec::Vec;

use lpir::IrModule;
use wasm_encoder::{
    CodeSection, ConstExpr, EntityType, ExportKind, ExportSection, FunctionSection, GlobalSection,
    GlobalType, ImportSection, MemoryType, Module, TypeSection, ValType,
};

/// Per-module state threaded through op emission.
pub(crate) struct EmitCtx<'a> {
    pub options: &'a crate::options::WasmOptions,
    pub import_remap: &'a [Option<u32>],
    pub full_import_count: u32,
    pub filtered_import_count: u32,
}

/// Per-function state (scratch local, shadow stack, slot layout).
pub(crate) struct FuncEmitCtx<'a> {
    pub module: &'a EmitCtx<'a>,
    pub i64_scratch: Option<u32>,
    pub sp_global: Option<u32>,
    pub frame_size: u32,
    pub slot_offsets: &'a [u32],
}

pub(crate) fn emit_module(
    ir: &IrModule,
    options: &crate::options::WasmOptions,
) -> Result<Vec<u8>, String> {
    let filtered = imports::build_filtered_imports(ir)?;
    let filtered_fn_count = filtered.decls.len() as u32;

    let mut types = TypeSection::new();
    let mut next_type = 0u32;
    let mut import_fn_types = Vec::new();

    for decl in &filtered.decls {
        let (params, results) = imports::import_decl_val_types(decl, options.float_mode);
        types.ty().function(params, results);
        import_fn_types.push(next_type);
        next_type += 1;
    }

    let mut def_fn_types = Vec::new();
    for f in &ir.functions {
        let (params, results) = func::wasm_function_signature(f, options.float_mode);
        types.ty().function(params, results);
        def_fn_types.push(next_type);
        next_type += 1;
    }

    let any_slots = ir.functions.iter().any(|f| !f.slots.is_empty());
    let mut import_section = ImportSection::new();
    let needs_memory = !filtered.decls.is_empty() || any_slots;
    if needs_memory {
        import_section.import(
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
    for (decl, &ty_idx) in filtered.decls.iter().zip(import_fn_types.iter()) {
        let wasm_name = imports::builtins_wasm_name(decl)?;
        import_section.import("builtins", wasm_name, EntityType::Function(ty_idx));
    }

    let mut functions = FunctionSection::new();
    for ty_idx in &def_fn_types {
        functions.function(*ty_idx);
    }

    let mut exports = ExportSection::new();
    for (i, f) in ir.functions.iter().enumerate() {
        let wasm_fn_index = filtered_fn_count + i as u32;
        exports.export(f.name.as_str(), ExportKind::Func, wasm_fn_index);
    }

    let ctx = EmitCtx {
        options,
        import_remap: &filtered.remap,
        full_import_count: filtered.full_count,
        filtered_import_count: filtered_fn_count,
    };

    // $sp is global index 0 — only valid while it's the sole WASM global.
    let sp_global = if any_slots { Some(0u32) } else { None };
    let mut globals = GlobalSection::new();
    if any_slots {
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(65536),
        );
    }

    let mut code = CodeSection::new();
    for f in &ir.functions {
        let wasm_fn = func::encode_ir_function(ir, f, &ctx, sp_global)?;
        code.function(&wasm_fn);
    }

    let mut module = Module::new();
    module.section(&types);
    if !import_section.is_empty() {
        module.section(&import_section);
    }
    module.section(&functions);
    if !globals.is_empty() {
        module.section(&globals);
    }
    module.section(&exports);
    module.section(&code);

    Ok(module.finish())
}
