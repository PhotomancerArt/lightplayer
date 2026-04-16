//! LPIR → WASM emission (Stage V).

mod builtin_wasm_import_types;
mod control;
mod func;
mod imports;
mod memory;
mod ops;
mod q32;

use alloc::string::String;
use alloc::vec::Vec;

use lpir::FloatMode;
use lpir::LpirModule;

use crate::module::EnvMemorySpec;
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, MemArg, MemoryType, Module,
    TypeSection, ValType,
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
    /// Local index for VMContext pointer. Always Some(0) in current implementation.
    pub vmctx_local: Option<u32>,
    pub i64_scratch: Option<u32>,
    pub sp_global: Option<u32>,
    pub frame_size: u32,
    /// Slot offsets for memory operations. Stored as Vec for ownership.
    pub slot_offsets: alloc::vec::Vec<u32>,
    /// Byte offset from `$sp` after prologue for result-pointer builtin scratch (after slots).
    pub result_buffer_base_offset: u32,
    /// After a return instruction, code is unreachable. Skip non-structural ops
    /// to avoid stack type errors, but still process End/Else for control stack balance.
    pub unreachable_mode: bool,
}

pub(crate) fn emit_module(
    ir: &LpirModule,
    options: &crate::options::WasmOptions,
) -> Result<(Vec<u8>, Option<i32>, Option<EnvMemorySpec>), String> {
    let filtered = imports::build_filtered_imports(ir)?;
    let filtered_fn_count = filtered.decls.len() as u32;

    let mut types = TypeSection::new();
    let mut next_type = 0u32;
    let mut import_fn_types = Vec::new();

    for decl in &filtered.decls {
        let (params, results) = imports::import_decl_val_types(decl, options.float_mode)?;
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

    // Detect `vec4 render(vec2, vec2, float)` → 5 params, 4 results in Q32.
    let render_entry = find_render_entry(ir, options.float_mode);
    let render_frame_type_idx = if render_entry.is_some() {
        types
            .ty()
            .function([ValType::I32; 4], core::iter::empty::<ValType>());
        let idx = next_type;
        next_type += 1;
        Some(idx)
    } else {
        None
    };
    let _ = next_type;

    let any_slots = ir.functions.iter().any(|f| !f.slots.is_empty());
    let needs_result_ptr_calls = imports::module_needs_result_ptr_calls(ir);
    let needs_shadow_stack = any_slots || needs_result_ptr_calls;
    let mut import_section = ImportSection::new();
    let needs_memory = !filtered.decls.is_empty()
        || ir.functions.iter().any(|f| f.uses_memory())
        || render_entry.is_some();
    let env_memory = if needs_memory {
        let spec = EnvMemorySpec::shader_import_limits();
        let min = spec.initial_pages as u64;
        import_section.import(
            "env",
            "memory",
            MemoryType {
                minimum: min,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            },
        );
        Some(spec)
    } else {
        None
    };
    for (decl, &ty_idx) in filtered.decls.iter().zip(import_fn_types.iter()) {
        let wasm_name = imports::builtins_wasm_name(decl)?;
        import_section.import("builtins", wasm_name, EntityType::Function(ty_idx));
    }

    let mut functions = FunctionSection::new();
    for ty_idx in &def_fn_types {
        functions.function(*ty_idx);
    }
    if let Some(ty_idx) = render_frame_type_idx {
        functions.function(ty_idx);
    }

    let mut exports = ExportSection::new();
    if needs_shadow_stack {
        exports.export(
            crate::module::SHADOW_STACK_GLOBAL_EXPORT,
            ExportKind::Global,
            0,
        );
    }
    for (i, f) in ir.functions.iter().enumerate() {
        let wasm_fn_index = filtered_fn_count + i as u32;
        exports.export(f.name.as_str(), ExportKind::Func, wasm_fn_index);
    }
    if render_entry.is_some() {
        let render_fn_index = filtered_fn_count + ir.functions.len() as u32;
        exports.export("render_frame", ExportKind::Func, render_fn_index);
    }

    let ctx = EmitCtx {
        options,
        import_remap: &filtered.remap,
        full_import_count: filtered.full_count,
        filtered_import_count: filtered_fn_count,
    };

    // $sp is global index 0 — only valid while it's the sole WASM global.
    let sp_global = if needs_shadow_stack { Some(0u32) } else { None };

    // VMContext local index - always 0 (first local in every function)
    let vmctx_local = Some(0u32);
    let mut globals = GlobalSection::new();
    if needs_shadow_stack {
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(memory::SHADOW_STACK_BASE),
        );
    }

    let mut code = CodeSection::new();
    for f in &ir.functions {
        let func_ctx = FuncEmitCtx {
            module: &ctx,
            vmctx_local,
            i64_scratch: None, // Will be calculated inside encode_ir_function
            sp_global,
            frame_size: 0, // Will be calculated inside encode_ir_function
            slot_offsets: alloc::vec::Vec::new(),
            result_buffer_base_offset: 0,
            unreachable_mode: false,
        };
        let wasm_fn = func::encode_ir_function(ir, f, &ctx, func_ctx)?;
        code.function(&wasm_fn);
    }
    if let Some((render_idx, _)) = render_entry {
        let render_wasm_idx = filtered_fn_count + render_idx as u32;
        let rf = emit_render_frame(render_wasm_idx, sp_global);
        code.function(&rf);
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

    let shadow_stack_base = if needs_shadow_stack {
        Some(memory::SHADOW_STACK_BASE)
    } else {
        None
    };
    Ok((module.finish(), shadow_stack_base, env_memory))
}

// ---------------------------------------------------------------------------
// render_frame: pixel loop emitted as raw WASM
// ---------------------------------------------------------------------------

/// Match `vec4 render(vec2, vec2, float)` — WASM `(vmctx, 5×i32) -> 4×i32` in Q32.
fn find_render_entry(ir: &LpirModule, mode: FloatMode) -> Option<(usize, u32)> {
    for (i, f) in ir.functions.iter().enumerate() {
        if f.name != "render" {
            continue;
        }
        let (params, results) = func::wasm_function_signature(f, mode);
        if params.len() == 6 && results.len() == 4 {
            return Some((i, f.param_count as u32));
        }
    }
    None
}

/// Emit `render_frame(width, height, time, out_ptr)` — loops over every pixel,
/// calls `render`, converts Q16.16 vec4 → RGBA8, stores to linear memory.
///
/// One WASM call per frame instead of W×H JS→WASM transitions.
fn emit_render_frame(main_fn_idx: u32, sp_global: Option<u32>) -> Function {
    // params: 0=width  1=height  2=time  3=out_ptr
    // locals: 4=y  5=x  6=ptr  7=r  8=g  9=b  10=a
    let mut f = Function::new([(7, ValType::I32)]);
    let mut s = f.instructions();

    if let Some(sp) = sp_global {
        s.i32_const(memory::SHADOW_STACK_BASE).global_set(sp);
    }

    // ptr = out_ptr
    s.local_get(3).local_set(6);
    // y = 0
    s.i32_const(0).local_set(4);

    // -- outer: for y in 0..height --
    s.block(BlockType::Empty); // @1 break
    s.loop_(BlockType::Empty); // @2 continue
    {
        s.local_get(4).local_get(1).i32_ge_u().br_if(1);

        s.i32_const(0).local_set(5); // x = 0

        // -- inner: for x in 0..width --
        s.block(BlockType::Empty); // @3 break
        s.loop_(BlockType::Empty); // @4 continue
        {
            s.local_get(5).local_get(0).i32_ge_u().br_if(1);

            // render(vmctx, x<<16, y<<16, w<<16, h<<16, time)
            s.i32_const(0);
            s.local_get(5).i32_const(16).i32_shl();
            s.local_get(4).i32_const(16).i32_shl();
            s.local_get(0).i32_const(16).i32_shl();
            s.local_get(1).i32_const(16).i32_shl();
            s.local_get(2);
            s.call(main_fn_idx);
            // stack: r g b a
            s.local_set(10); // a
            s.local_set(9); // b
            s.local_set(8); // g
            s.local_set(7); // r

            let m = MemArg {
                offset: 0,
                align: 0,
                memory_index: 0,
            };

            // R
            s.local_get(6);
            emit_q32_to_u8(&mut s, 7);
            s.i32_store8(m);
            // G
            s.local_get(6);
            emit_q32_to_u8(&mut s, 8);
            s.i32_store8(MemArg { offset: 1, ..m });
            // B
            s.local_get(6);
            emit_q32_to_u8(&mut s, 9);
            s.i32_store8(MemArg { offset: 2, ..m });
            // A
            s.local_get(6);
            emit_q32_to_u8(&mut s, 10);
            s.i32_store8(MemArg { offset: 3, ..m });

            // ptr += 4;  x += 1
            s.local_get(6).i32_const(4).i32_add().local_set(6);
            s.local_get(5).i32_const(1).i32_add().local_set(5);
            s.br(0);
        }
        s.end(); // loop @4
        s.end(); // block @3

        // y += 1
        s.local_get(4).i32_const(1).i32_add().local_set(4);
        s.br(0);
    }
    s.end(); // loop @2
    s.end(); // block @1

    s.end(); // function
    f
}

/// `clamp((v * 255) >> 16, 0, 255)` — leaves a u8-range i32 on the stack.
///
/// Reuses `local` as scratch (the caller's value is already consumed).
fn emit_q32_to_u8(s: &mut wasm_encoder::InstructionSink<'_>, local: u32) {
    // raw = (v * 255) >>s 16  (signed: negatives stay negative)
    s.local_get(local)
        .i32_const(255)
        .i32_mul()
        .i32_const(16)
        .i32_shr_s();

    // clamp low: max(raw, 0) via select(raw, 0, raw >= 0)
    s.i32_const(0)
        .local_get(local)
        .i32_const(0)
        .i32_ge_s()
        .select();

    // clamp high: min(result, 255) — tee into `local` so we can compare
    s.local_tee(local);
    s.i32_const(255);
    s.local_get(local).i32_const(255).i32_le_s().select();
}
