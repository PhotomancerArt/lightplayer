//! Encode one [`lpir::IrFunction`] body into a `wasm_encoder::Function`.

use alloc::string::String;
use alloc::vec::Vec;

use lpir::FloatMode;
use lpir::{IrFunction, IrType, LpirModule, LpirOp};
use wasm_encoder::{Function, InstructionSink, ValType};

use crate::emit::control::{self, CtrlEntry, WasmOpenDepth};
use crate::emit::imports;
use crate::emit::memory;
use crate::emit::ops::emit_op;
use crate::emit::{EmitCtx, FdivRecipLocals, FuncEmitCtx};
use lps_q32::q32_options::DivMode;

fn ir_type_to_val(ty: IrType, mode: FloatMode) -> ValType {
    match (ty, mode) {
        (IrType::I32 | IrType::Pointer, _) => ValType::I32,
        (IrType::F32, FloatMode::Q32) => ValType::I32,
        (IrType::F32, FloatMode::F32) => ValType::F32,
    }
}

fn func_needs_fdiv_recip_scratch(f: &IrFunction, mode: FloatMode, ctx: &EmitCtx<'_>) -> bool {
    mode == FloatMode::Q32
        && ctx.q32.div == DivMode::Reciprocal
        && f.body.iter().any(|op| matches!(op, LpirOp::Fdiv { .. }))
}

fn func_needs_i64_scratch(f: &IrFunction, mode: FloatMode) -> bool {
    if mode != FloatMode::Q32 {
        return false;
    }
    f.body.iter().any(|op| {
        matches!(
            op,
            LpirOp::Fadd { .. }
                | LpirOp::Fsub { .. }
                | LpirOp::Fmul { .. }
                | LpirOp::ItofS { .. }
                | LpirOp::ItofU { .. }
        )
    })
}

/// WASM `(params) -> (results)` for `f`'s type section entry.
///
/// Params match [`IrFunction::vreg_types`] for all parameter vregs:
/// vmctx (`%0`), optional sret pointer (`%1`), then user params — see
/// [`IrFunction::total_param_slots`].
pub(crate) fn wasm_function_signature(
    f: &IrFunction,
    mode: FloatMode,
) -> (Vec<ValType>, Vec<ValType>) {
    let n = f.total_param_slots() as usize;
    let params: Vec<ValType> = (0..n)
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
    ir: &LpirModule,
    f: &IrFunction,
    ctx: &EmitCtx<'_>,
    mut func_ctx: FuncEmitCtx<'_>,
) -> Result<Function, String> {
    let mode = ctx.options.float_mode;

    // Local index mapping: WASM local index == vreg index. Parameter locals are
    // vregs `0..total_param_slots`; declared locals follow.

    // VMContext local is at index 0 (verified present)
    let _vmctx_local = func_ctx.vmctx_local.expect("vmctx_local must be set");
    let sp_global = func_ctx.sp_global;

    let param_slots = f.total_param_slots() as usize;

    // Declared locals: vregs that are not parameters
    let mut local_types: Vec<ValType> = Vec::new();
    for i in param_slots..f.vreg_types.len() {
        local_types.push(ir_type_to_val(f.vreg_types[i], mode));
    }

    let i64_scratch = if func_needs_i64_scratch(f, mode) {
        let idx = (param_slots + local_types.len()) as u32;
        local_types.push(ValType::I64);
        Some(idx)
    } else {
        None
    };
    func_ctx.i64_scratch = i64_scratch;

    let fdiv_recip_scratch = if func_needs_fdiv_recip_scratch(f, mode, ctx) {
        let base = (param_slots + local_types.len()) as u32;
        for _ in 0..7 {
            local_types.push(ValType::I32);
        }
        Some(FdivRecipLocals {
            divisor: base,
            dividend: base + 1,
            sign: base + 2,
            abs_dividend: base + 3,
            abs_divisor: base + 4,
            recip: base + 5,
            quot: base + 6,
        })
    } else {
        None
    };
    func_ctx.fdiv_recip_scratch = fdiv_recip_scratch;

    func_ctx.slot_offsets = memory::slot_offsets(f);
    let slot_frame = memory::aligned_frame_size(f);
    let result_buf = imports::max_result_ptr_buffer_bytes(ir, f);
    let (frame_size, result_buffer_base_offset) = if result_buf > 0 {
        (
            memory::align_up(slot_frame.saturating_add(result_buf), memory::FRAME_ALIGN),
            slot_frame,
        )
    } else {
        (slot_frame, 0u32)
    };
    if frame_size > 0 && sp_global.is_none() {
        return Err(String::from(
            "function needs shadow stack (slots or result-pointer calls) but module has no $sp global",
        ));
    }

    // Update func_ctx with calculated values
    func_ctx.frame_size = frame_size;
    func_ctx.result_buffer_base_offset = result_buffer_base_offset;

    let mut wasm_fn = Function::new_with_locals_types(local_types);
    let mut ctrl: Vec<CtrlEntry> = Vec::new();
    let mut wasm_open: WasmOpenDepth = 0;
    emit_function_ops(
        wasm_fn.instructions(),
        &mut ctrl,
        &mut func_ctx,
        ir,
        f,
        &mut wasm_open,
    )?;
    Ok(wasm_fn)
}

fn emit_function_ops(
    mut sink: InstructionSink<'_>,
    ctrl: &mut Vec<CtrlEntry>,
    fctx: &mut FuncEmitCtx<'_>,
    ir: &LpirModule,
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
            .is_some_and(|o| matches!(o, LpirOp::Return { .. }));
        if !last_is_return {
            memory::emit_shadow_epilogue(&mut sink, sp, fctx.frame_size);
        }
    }
    // Multi-result functions whose body ends with `end_if` after both branches `return` have no
    // fallthrough values; the implicit function `end` still type-checks the merge. Mark the tail
    // unreachable so validation matches void-only `if`/`else`/`end` behavior.
    if !f.return_types.is_empty() && f.body.last().is_some_and(|o| matches!(o, LpirOp::End)) {
        sink.unreachable();
    }
    sink.end();
    Ok(())
}
