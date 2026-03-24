//! Encode one [`lpir::IrFunction`] body into a `wasm_encoder::Function`.

use alloc::string::String;
use alloc::vec::Vec;

use lp_glsl_naga::FloatMode;
use lpir::{IrFunction, IrModule, IrType, Op};
use wasm_encoder::{Function, InstructionSink, ValType};

use crate::emit::control::{self, CtrlEntry, WasmOpenDepth};
use crate::emit::memory;
use crate::emit::ops::emit_op;
use crate::emit::{EmitCtx, FuncEmitCtx};

fn ir_type_to_val(ty: IrType, mode: FloatMode) -> ValType {
    match (ty, mode) {
        (IrType::I32, _) => ValType::I32,
        (IrType::F32, FloatMode::Q32) => ValType::I32,
        (IrType::F32, FloatMode::Float) => ValType::F32,
    }
}

fn func_needs_i64_scratch(f: &IrFunction, mode: FloatMode) -> bool {
    if mode != FloatMode::Q32 {
        return false;
    }
    f.body.iter().any(|op| {
        matches!(
            op,
            Op::Fadd { .. }
                | Op::Fsub { .. }
                | Op::Fmul { .. }
                | Op::ItofS { .. }
                | Op::ItofU { .. }
        )
    })
}

/// WASM `(params) -> (results)` for `f`'s type section entry.
pub(crate) fn wasm_function_signature(
    f: &IrFunction,
    mode: FloatMode,
) -> (Vec<ValType>, Vec<ValType>) {
    let params: Vec<ValType> = (0..f.param_count as usize)
        .map(|i| ir_type_to_val(f.vreg_types[i], mode))
        .collect();
    let results: Vec<ValType> = f
        .return_types
        .iter()
        .copied()
        .map(|t| ir_type_to_val(t, mode))
        .collect();
    (params, results)
}

/// Encode `f` into a WASM function body (locals + op stream).
pub(crate) fn encode_ir_function(
    ir: &IrModule,
    f: &IrFunction,
    ctx: &EmitCtx<'_>,
    sp_global: Option<u32>,
) -> Result<Function, String> {
    let mode = ctx.options.float_mode;
    let mut local_types: Vec<ValType> = Vec::new();
    for i in f.param_count as usize..f.vreg_types.len() {
        local_types.push(ir_type_to_val(f.vreg_types[i], mode));
    }
    let i64_scratch = if func_needs_i64_scratch(f, mode) {
        local_types.push(ValType::I64);
        Some(f.vreg_types.len() as u32)
    } else {
        None
    };

    let slot_offsets = memory::slot_offsets(f);
    let frame_size = memory::aligned_frame_size(f);
    if frame_size > 0 && sp_global.is_none() {
        return Err(String::from(
            "function has slots but module has no $sp global",
        ));
    }

    let func_ctx = FuncEmitCtx {
        module: ctx,
        i64_scratch,
        sp_global,
        frame_size,
        slot_offsets: slot_offsets.as_slice(),
    };

    let mut wasm_fn = Function::new_with_locals_types(local_types);
    let mut ctrl: Vec<CtrlEntry> = Vec::new();
    let mut wasm_open: WasmOpenDepth = 0;
    emit_function_ops(
        wasm_fn.instructions(),
        &mut ctrl,
        &func_ctx,
        ir,
        f,
        &mut wasm_open,
    )?;
    Ok(wasm_fn)
}

fn emit_function_ops(
    mut sink: InstructionSink<'_>,
    ctrl: &mut Vec<CtrlEntry>,
    fctx: &FuncEmitCtx<'_>,
    ir: &IrModule,
    f: &IrFunction,
    wasm_open: &mut WasmOpenDepth,
) -> Result<(), String> {
    if fctx.frame_size > 0 {
        let sp = fctx
            .sp_global
            .ok_or_else(|| String::from("internal: frame without $sp"))?;
        memory::emit_shadow_prologue(&mut sink, sp, fctx.frame_size);
    }
    for (pc, op) in f.body.iter().enumerate() {
        control::close_loop_inner_at_continuing(&mut sink, ctrl, wasm_open, pc);
        emit_op(&mut sink, ctrl, fctx, ir, f, pc, op, wasm_open)?;
    }
    debug_assert_eq!(
        *wasm_open, 0,
        "WASM block depth should balance before function end"
    );
    if !ctrl.is_empty() {
        return Err(String::from(
            "unclosed control construct at end of function",
        ));
    }
    if fctx.frame_size > 0 {
        let sp = fctx
            .sp_global
            .ok_or_else(|| String::from("internal: frame without $sp"))?;
        let last_is_return = f
            .body
            .last()
            .is_some_and(|o| matches!(o, Op::Return { .. }));
        if !last_is_return {
            memory::emit_shadow_epilogue(&mut sink, sp, fctx.frame_size);
        }
    }
    // Multi-result functions whose body ends with `end_if` after both branches `return` have no
    // fallthrough values; the implicit function `end` still type-checks the merge. Mark the tail
    // unreachable so validation matches void-only `if`/`else`/`end` behavior.
    if !f.return_types.is_empty() && f.body.last().is_some_and(|o| matches!(o, Op::End)) {
        sink.unreachable();
    }
    sink.end();
    Ok(())
}
