//! Map each [`lpir::LpirOp`] to WASM instructions.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::FloatMode;
use lpir::{CalleeRef, FuncId, ImportId, IrFunction, IrType, LpirModule, LpirOp};
use lps_q32::q32_options::{AddSubMode, DivMode, MulMode};
use wasm_encoder::{BlockType, Ieee32, InstructionSink, ValType};

use crate::emit::FuncEmitCtx;
use crate::emit::control::{
    CtrlEntry, WasmOpenDepth, innermost_fwd_block_exit_depth, innermost_loop_break_depth,
    innermost_loop_continue_depth, innermost_switch_selector, switch_merge_open_depth,
    unwind_ctrl_after_return,
};
use crate::emit::imports;
use crate::emit::memory;
use crate::emit::q32;

fn wasm_func_index(ctx: &FuncEmitCtx<'_>, callee: CalleeRef) -> Result<u32, String> {
    let m = ctx.module;
    match callee {
        CalleeRef::Import(ImportId(i)) => {
            let k = i as usize;
            m.import_remap[k].ok_or_else(|| format!("call to pruned import {k}"))
        }
        CalleeRef::Local(FuncId(id)) => Ok(m.filtered_import_count + id as u32),
    }
}

fn vreg_val_ty(func: &IrFunction, reg: lpir::VReg, mode: FloatMode) -> Result<ValType, String> {
    let ty = func
        .vreg_types
        .get(reg.0 as usize)
        .copied()
        .ok_or_else(|| alloc::format!("v{} out of range", reg.0))?;
    Ok(match (ty, mode) {
        (IrType::I32 | IrType::Pointer, _) => ValType::I32,
        (IrType::F32, FloatMode::Q32) => ValType::I32,
        (IrType::F32, FloatMode::F32) => ValType::F32,
    })
}

pub(crate) fn emit_op(
    sink: &mut InstructionSink<'_>,
    ctrl: &mut Vec<CtrlEntry>,
    fctx: &mut FuncEmitCtx<'_>,
    ir: &LpirModule,
    func: &IrFunction,
    _pc: usize,
    op: &LpirOp,
    wasm_open: &mut WasmOpenDepth,
) -> Result<(), String> {
    // In unreachable mode, only process structural ops needed for stack balance.
    let is_structural = matches!(
        op,
        LpirOp::End
            | LpirOp::Else
            | LpirOp::Block { .. }
            | LpirOp::SwitchStart { .. }
            | LpirOp::CaseStart { .. }
            | LpirOp::DefaultStart { .. }
    );

    if fctx.unreachable_mode && !is_structural {
        return Ok(());
    }
    let fm = fctx.module.options.float_mode;
    match op {
        LpirOp::Iadd { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_add()
                .local_set(dst.0);
        }
        LpirOp::Isub { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_sub()
                .local_set(dst.0);
        }
        LpirOp::Imul { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_mul()
                .local_set(dst.0);
        }
        LpirOp::IdivS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_div_s()
                .local_set(dst.0);
        }
        LpirOp::IdivU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_div_u()
                .local_set(dst.0);
        }
        LpirOp::IremS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_rem_s()
                .local_set(dst.0);
        }
        LpirOp::IremU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_rem_u()
                .local_set(dst.0);
        }
        LpirOp::Ineg { dst, src } => {
            sink.i32_const(0)
                .local_get(src.0)
                .i32_sub()
                .local_set(dst.0);
        }
        LpirOp::Ieq { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_eq()
                .local_set(dst.0);
        }
        LpirOp::Ine { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_ne()
                .local_set(dst.0);
        }
        LpirOp::IltS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_lt_s()
                .local_set(dst.0);
        }
        LpirOp::IleS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_le_s()
                .local_set(dst.0);
        }
        LpirOp::IgtS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_gt_s()
                .local_set(dst.0);
        }
        LpirOp::IgeS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_ge_s()
                .local_set(dst.0);
        }
        LpirOp::IltU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_lt_u()
                .local_set(dst.0);
        }
        LpirOp::IleU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_le_u()
                .local_set(dst.0);
        }
        LpirOp::IgtU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_gt_u()
                .local_set(dst.0);
        }
        LpirOp::IgeU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_ge_u()
                .local_set(dst.0);
        }
        LpirOp::Iand { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_and()
                .local_set(dst.0);
        }
        LpirOp::Ior { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_or()
                .local_set(dst.0);
        }
        LpirOp::Ixor { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_xor()
                .local_set(dst.0);
        }
        LpirOp::Ibnot { dst, src } => {
            sink.i32_const(-1)
                .local_get(src.0)
                .i32_xor()
                .local_set(dst.0);
        }
        LpirOp::Ishl { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_shl()
                .local_set(dst.0);
        }
        LpirOp::IshrS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_shr_s()
                .local_set(dst.0);
        }
        LpirOp::IshrU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_shr_u()
                .local_set(dst.0);
        }
        LpirOp::IconstI32 { dst, value } => {
            sink.i32_const(*value).local_set(dst.0);
        }
        LpirOp::IaddImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_add()
                .local_set(dst.0);
        }
        LpirOp::IsubImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_sub()
                .local_set(dst.0);
        }
        LpirOp::ImulImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_mul()
                .local_set(dst.0);
        }
        LpirOp::IshlImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_shl()
                .local_set(dst.0);
        }
        LpirOp::IshrSImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_shr_s()
                .local_set(dst.0);
        }
        LpirOp::IshrUImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_shr_u()
                .local_set(dst.0);
        }
        LpirOp::IeqImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_eq()
                .local_set(dst.0);
        }
        LpirOp::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => {
            sink.local_get(if_true.0)
                .local_get(if_false.0)
                .local_get(cond.0)
                .select()
                .local_set(dst.0);
        }
        LpirOp::Copy { dst, src } => {
            sink.local_get(src.0).local_set(dst.0);
        }
        LpirOp::Block { .. } => {
            sink.block(BlockType::Empty);
            *wasm_open += 1;
            ctrl.push(CtrlEntry::FwdBlock {
                after_open_wasm_depth: *wasm_open,
            });
        }
        LpirOp::ExitBlock => {
            let d = innermost_fwd_block_exit_depth(ctrl, *wasm_open)?;
            sink.br(d);
        }
        LpirOp::IfStart { cond, .. } => {
            sink.local_get(cond.0).if_(BlockType::Empty);
            *wasm_open += 1;
            ctrl.push(CtrlEntry::If);
        }
        LpirOp::Else => {
            match ctrl.pop() {
                Some(CtrlEntry::If) => {
                    sink.else_();
                    ctrl.push(CtrlEntry::Else);
                    // When entering else branch, code is reachable again
                    fctx.unreachable_mode = false;
                }
                _ => return Err(String::from("`else` without matching `if`")),
            }
        }
        LpirOp::End => {
            if ctrl.is_empty() {
                // `return` may have already emitted matching `end`s via [`unwind_ctrl_after_return`].
                return Ok(());
            }
            if ctrl.len() == 1 && matches!(ctrl.last(), Some(CtrlEntry::Switch { .. })) {
                // `end_switch_arm` after `return` from a case: case `if` already closed in unwind.
                return Ok(());
            }
            if matches!(ctrl.last(), Some(CtrlEntry::SwitchCaseArm)) {
                let merge = switch_merge_open_depth(ctrl)?;
                ctrl.pop();
                let d = wasm_open.saturating_sub(merge);
                sink.br(d);
                sink.end();
                *wasm_open = wasm_open.saturating_sub(1);
                return Ok(());
            }
            if matches!(ctrl.last(), Some(CtrlEntry::SwitchDefaultArm)) {
                ctrl.pop();
                return Ok(());
            }

            match ctrl.pop() {
                Some(CtrlEntry::If) | Some(CtrlEntry::Else) => {
                    sink.end();
                    *wasm_open = wasm_open.saturating_sub(1);
                    // After closing an if/else block, subsequent code is reachable again.
                    fctx.unreachable_mode = false;
                }
                Some(CtrlEntry::Loop { .. }) => {
                    sink.br(0);
                    sink.end();
                    sink.end();
                    *wasm_open = wasm_open.saturating_sub(2);
                }
                Some(CtrlEntry::Switch { .. }) => {
                    sink.end();
                    *wasm_open = wasm_open.saturating_sub(1);
                }
                Some(CtrlEntry::FwdBlock { .. }) => {
                    sink.end();
                    *wasm_open = wasm_open.saturating_sub(1);
                }
                Some(other) => {
                    ctrl.push(other);
                    return Err(String::from("unexpected `End` for control stack state"));
                }
                None => return Err(String::from("unexpected `End` (empty control stack)")),
            }
        }
        LpirOp::LoopStart {
            continuing_offset, ..
        } => {
            sink.block(BlockType::Empty);
            let outer_open = *wasm_open;
            *wasm_open += 1;
            sink.loop_(BlockType::Empty);
            *wasm_open += 1;
            sink.block(BlockType::Empty);
            *wasm_open += 1;
            ctrl.push(CtrlEntry::Loop {
                continuing_offset: *continuing_offset,
                inner_closed: false,
                outer_open_depth: outer_open + 1,
            });
        }
        LpirOp::SwitchStart { selector, .. } => {
            sink.block(BlockType::Empty);
            *wasm_open += 1;
            ctrl.push(CtrlEntry::Switch {
                selector: selector.0,
                merge_wasm_open: *wasm_open,
            });
        }
        LpirOp::CaseStart { value, .. } => {
            let sel = innermost_switch_selector(ctrl)?;
            sink.local_get(sel)
                .i32_const(*value)
                .i32_eq()
                .if_(BlockType::Empty);
            *wasm_open += 1;
            ctrl.push(CtrlEntry::SwitchCaseArm);
        }
        LpirOp::DefaultStart { .. } => {
            ctrl.push(CtrlEntry::SwitchDefaultArm);
        }
        LpirOp::Break => {
            let d = innermost_loop_break_depth(ctrl, *wasm_open)?;
            sink.br(d);
        }
        LpirOp::Continue => {
            let d = innermost_loop_continue_depth(ctrl, *wasm_open)?;
            sink.br(d);
        }
        LpirOp::BrIfNot { cond } => {
            let d = innermost_loop_break_depth(ctrl, *wasm_open)?;
            sink.local_get(cond.0).i32_eqz().br_if(d);
        }
        LpirOp::Return { values } => {
            if fctx.frame_size > 0 {
                let sp = fctx
                    .sp_global
                    .ok_or_else(|| String::from("internal: return with frame but no $sp"))?;
                memory::emit_shadow_epilogue(sink, sp, fctx.frame_size);
            }
            for v in func.pool_slice(*values) {
                sink.local_get(v.0);
            }
            sink.return_();
            unwind_ctrl_after_return(sink, ctrl, wasm_open);
            // Mark subsequent code as unreachable. The control stack will be
            // drained by subsequent Op::End ops which still run to balance blocks.
            fctx.unreachable_mode = true;
        }
        LpirOp::Call {
            callee,
            args,
            results,
        } => {
            let idx = wasm_func_index(fctx, *callee)?;
            let (is_import, import_idx) = match *callee {
                CalleeRef::Import(ImportId(i)) => (true, i as usize),
                CalleeRef::Local(_) => (false, 0),
            };
            let is_result_ptr =
                is_import && imports::import_uses_result_pointer_abi(ir, import_idx);

            let all_args = func.pool_slice(*args);
            let import_needs_vmctx = is_import
                && ir
                    .imports
                    .get(import_idx)
                    .map(|d| d.needs_vmctx)
                    .unwrap_or(false);
            let args_to_pass =
                if is_import && !import_needs_vmctx && !all_args.is_empty() && all_args[0].0 == 0 {
                    &all_args[1..]
                } else {
                    all_args
                };

            if is_result_ptr {
                let sp = fctx.sp_global.ok_or_else(|| {
                    String::from("result-pointer builtin call without $sp global")
                })?;
                let base_off = i32::try_from(fctx.result_buffer_base_offset).unwrap_or(i32::MAX);

                // Hidden result pointer is the first argument (matches `extern "C"` builtins).
                sink.global_get(sp).i32_const(base_off).i32_add();
                for v in args_to_pass {
                    sink.local_get(v.0);
                }
                sink.call(idx);

                let m = memory::mem_arg0(0, 2);
                for (i, r) in func.pool_slice(*results).iter().enumerate() {
                    let off = base_off.saturating_add(i32::try_from(i * 4).unwrap_or(i32::MAX));
                    sink.global_get(sp)
                        .i32_const(off)
                        .i32_add()
                        .i32_load(m)
                        .local_set(r.0);
                }
            } else {
                for v in args_to_pass {
                    sink.local_get(v.0);
                }
                sink.call(idx);
                for r in func.pool_slice(*results).iter().rev() {
                    sink.local_set(r.0);
                }
            }
        }
        LpirOp::FconstF32 { dst, value } => match fm {
            FloatMode::Q32 => {
                let q = q32::f32_to_q16_16(*value);
                sink.i32_const(q).local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.f32_const(Ieee32::from(*value)).local_set(dst.0);
            }
        },
        LpirOp::Fadd { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => match fctx.module.q32.add_sub {
                AddSubMode::Saturating => {
                    let s = fctx.i64_scratch.ok_or_else(|| {
                        String::from("internal: Q32 Fadd without i64 scratch local")
                    })?;
                    q32::emit_q32_fadd(sink, lhs.0, rhs.0, dst.0, s);
                }
                AddSubMode::Wrapping => q32::emit_q32_fadd_wrap(sink, lhs.0, rhs.0, dst.0),
            },
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_add()
                    .local_set(dst.0);
            }
        },
        LpirOp::Fsub { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => match fctx.module.q32.add_sub {
                AddSubMode::Saturating => {
                    let s = fctx.i64_scratch.ok_or_else(|| {
                        String::from("internal: Q32 Fsub without i64 scratch local")
                    })?;
                    q32::emit_q32_fsub(sink, lhs.0, rhs.0, dst.0, s);
                }
                AddSubMode::Wrapping => q32::emit_q32_fsub_wrap(sink, lhs.0, rhs.0, dst.0),
            },
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_sub()
                    .local_set(dst.0);
            }
        },
        LpirOp::Fmul { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => match fctx.module.q32.mul {
                MulMode::Saturating => {
                    let s = fctx.i64_scratch.ok_or_else(|| {
                        String::from("internal: Q32 Fmul without i64 scratch local")
                    })?;
                    q32::emit_q32_fmul(sink, lhs.0, rhs.0, dst.0, s);
                }
                MulMode::Wrapping => q32::emit_q32_fmul_wrap(sink, lhs.0, rhs.0, dst.0),
            },
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_mul()
                    .local_set(dst.0);
            }
        },
        LpirOp::Fdiv { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => match fctx.module.q32.div {
                DivMode::Saturating => q32::emit_q32_fdiv(sink, lhs.0, rhs.0, dst.0),
                DivMode::Reciprocal => {
                    let loc = fctx.fdiv_recip_scratch.as_ref().ok_or_else(|| {
                        String::from("internal: Q32 Fdiv reciprocal without scratch locals")
                    })?;
                    q32::emit_q32_fdiv_recip(sink, lhs.0, rhs.0, dst.0, loc);
                }
            },
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_div()
                    .local_set(dst.0);
            }
        },
        LpirOp::Fneg { dst, src } => match fm {
            FloatMode::Q32 => {
                sink.i32_const(0)
                    .local_get(src.0)
                    .i32_sub()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_neg().local_set(dst.0);
            }
        },
        LpirOp::Fabs { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_fabs(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_abs().local_set(dst.0);
            }
        },
        LpirOp::Fsqrt { dst, src } => match fm {
            FloatMode::Q32 => {
                let callee = imports::import_callee(ir, "lpir", "sqrt")?;
                let idx = wasm_func_index(fctx, callee)?;
                sink.local_get(src.0).call(idx).local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_sqrt().local_set(dst.0);
            }
        },
        LpirOp::Fmin { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .local_get(lhs.0)
                    .local_get(rhs.0)
                    .i32_lt_s()
                    .select()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_min()
                    .local_set(dst.0);
            }
        },
        LpirOp::Fmax { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .local_get(lhs.0)
                    .local_get(rhs.0)
                    .i32_gt_s()
                    .select()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_max()
                    .local_set(dst.0);
            }
        },
        LpirOp::Ffloor { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_ffloor(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_floor().local_set(dst.0);
            }
        },
        LpirOp::Fceil { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_fceil(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_ceil().local_set(dst.0);
            }
        },
        LpirOp::Ftrunc { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_ftrunc(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_trunc().local_set(dst.0);
            }
        },
        LpirOp::Fnearest { dst, src } => match fm {
            FloatMode::Q32 => {
                let callee = imports::import_callee(ir, "glsl", "round")?;
                let idx = wasm_func_index(fctx, callee)?;
                sink.local_get(src.0).call(idx).local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_nearest().local_set(dst.0);
            }
        },
        LpirOp::Feq { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .i32_eq()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_eq()
                    .local_set(dst.0);
            }
        },
        LpirOp::Fne { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .i32_ne()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_ne()
                    .local_set(dst.0);
            }
        },
        LpirOp::Flt { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .i32_lt_s()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_lt()
                    .local_set(dst.0);
            }
        },
        LpirOp::Fle { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .i32_le_s()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_le()
                    .local_set(dst.0);
            }
        },
        LpirOp::Fgt { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .i32_gt_s()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_gt()
                    .local_set(dst.0);
            }
        },
        LpirOp::Fge { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .i32_ge_s()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_ge()
                    .local_set(dst.0);
            }
        },
        LpirOp::FtoiSatS { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_ftoi_sat_s(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).i32_trunc_sat_f32_s().local_set(dst.0);
            }
        },
        LpirOp::FtoiSatU { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_ftoi_sat_u(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).i32_trunc_sat_f32_u().local_set(dst.0);
            }
        },
        LpirOp::ItofS { dst, src } => match fm {
            FloatMode::Q32 => {
                let s = fctx
                    .i64_scratch
                    .ok_or_else(|| String::from("internal: Q32 ItofS without i64 scratch local"))?;
                q32::emit_q32_itof_s(sink, src.0, dst.0, s);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_convert_i32_s().local_set(dst.0);
            }
        },
        LpirOp::ItofU { dst, src } => match fm {
            FloatMode::Q32 => {
                let s = fctx
                    .i64_scratch
                    .ok_or_else(|| String::from("internal: Q32 ItofU without i64 scratch local"))?;
                q32::emit_q32_itof_u(sink, src.0, dst.0, s);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_convert_i32_u().local_set(dst.0);
            }
        },
        LpirOp::FfromI32Bits { dst, src } => match fm {
            FloatMode::Q32 => {
                sink.local_get(src.0).local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_reinterpret_i32().local_set(dst.0);
            }
        },
        LpirOp::FtoUnorm16 { dst, src } => match fm {
            FloatMode::Q32 => {
                sink.local_get(src.0)
                    .i32_const(0)
                    .local_get(src.0)
                    .i32_const(0)
                    .i32_gt_s()
                    .select()
                    .local_set(dst.0);
                sink.local_get(dst.0)
                    .i32_const(65535)
                    .local_get(dst.0)
                    .i32_const(65535)
                    .i32_lt_s()
                    .select()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0)
                    .f32_const(Ieee32::new(0.0f32.to_bits()))
                    .f32_max()
                    .f32_const(Ieee32::new(1.0f32.to_bits()))
                    .f32_min()
                    .f32_const(Ieee32::new(65535.0f32.to_bits()))
                    .f32_mul()
                    .i32_trunc_sat_f32_u()
                    .local_set(dst.0);
            }
        },
        LpirOp::FtoUnorm8 { dst, src } => match fm {
            FloatMode::Q32 => {
                sink.local_get(src.0)
                    .i32_const(8)
                    .i32_shr_u()
                    .local_set(dst.0);
                sink.local_get(dst.0)
                    .i32_const(0)
                    .local_get(dst.0)
                    .i32_const(0)
                    .i32_gt_s()
                    .select()
                    .local_set(dst.0);
                sink.local_get(dst.0)
                    .i32_const(255)
                    .local_get(dst.0)
                    .i32_const(255)
                    .i32_lt_s()
                    .select()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0)
                    .f32_const(Ieee32::new(0.0f32.to_bits()))
                    .f32_max()
                    .f32_const(Ieee32::new(1.0f32.to_bits()))
                    .f32_min()
                    .f32_const(Ieee32::new(255.0f32.to_bits()))
                    .f32_mul()
                    .i32_trunc_sat_f32_u()
                    .local_set(dst.0);
            }
        },
        LpirOp::Unorm16toF { dst, src } => match fm {
            FloatMode::Q32 => {
                sink.local_get(src.0)
                    .i32_const(0xFFFF)
                    .i32_and()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0)
                    .i32_const(0xFFFF)
                    .i32_and()
                    .f32_convert_i32_u()
                    .f32_const(Ieee32::new(65535.0f32.to_bits()))
                    .f32_div()
                    .local_set(dst.0);
            }
        },
        LpirOp::Unorm8toF { dst, src } => match fm {
            FloatMode::Q32 => {
                sink.local_get(src.0)
                    .i32_const(0xFF)
                    .i32_and()
                    .i32_const(8)
                    .i32_shl()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0)
                    .i32_const(0xFF)
                    .i32_and()
                    .f32_convert_i32_u()
                    .f32_const(Ieee32::new(255.0f32.to_bits()))
                    .f32_div()
                    .local_set(dst.0);
            }
        },
        LpirOp::SlotAddr { dst, slot } => {
            let off = fctx
                .slot_offsets
                .get(slot.0 as usize)
                .copied()
                .ok_or_else(|| alloc::format!("slot {} out of range", slot.0))?;
            let sp = fctx
                .sp_global
                .ok_or_else(|| String::from("SlotAddr without shadow stack global"))?;
            sink.global_get(sp)
                .i32_const(i32::try_from(off).unwrap_or(i32::MAX))
                .i32_add()
                .local_set(dst.0);
        }
        LpirOp::Load { dst, base, offset } => {
            let m = memory::mem_arg0(*offset, 2);
            match vreg_val_ty(func, *dst, fm)? {
                ValType::I32 => {
                    sink.local_get(base.0).i32_load(m).local_set(dst.0);
                }
                ValType::F32 => {
                    sink.local_get(base.0).f32_load(m).local_set(dst.0);
                }
                _ => return Err(String::from("Load: unsupported vreg type")),
            }
        }
        LpirOp::Load8U { dst, base, offset } => {
            let m = memory::mem_arg0(*offset, 0);
            sink.local_get(base.0).i32_load8_u(m).local_set(dst.0);
        }
        LpirOp::Load8S { dst, base, offset } => {
            let m = memory::mem_arg0(*offset, 0);
            sink.local_get(base.0).i32_load8_s(m).local_set(dst.0);
        }
        LpirOp::Load16U { dst, base, offset } => {
            let m = memory::mem_arg0(*offset, 1);
            sink.local_get(base.0).i32_load16_u(m).local_set(dst.0);
        }
        LpirOp::Load16S { dst, base, offset } => {
            let m = memory::mem_arg0(*offset, 1);
            sink.local_get(base.0).i32_load16_s(m).local_set(dst.0);
        }
        LpirOp::Store {
            base,
            offset,
            value,
        } => {
            let m = memory::mem_arg0(*offset, 2);
            match vreg_val_ty(func, *value, fm)? {
                ValType::I32 => {
                    sink.local_get(base.0).local_get(value.0).i32_store(m);
                }
                ValType::F32 => {
                    sink.local_get(base.0).local_get(value.0).f32_store(m);
                }
                _ => return Err(String::from("Store: unsupported vreg type")),
            }
        }
        LpirOp::Store8 {
            base,
            offset,
            value,
        } => {
            let m = memory::mem_arg0(*offset, 0);
            sink.local_get(base.0).local_get(value.0).i32_store8(m);
        }
        LpirOp::Store16 {
            base,
            offset,
            value,
        } => {
            let m = memory::mem_arg0(*offset, 1);
            sink.local_get(base.0).local_get(value.0).i32_store16(m);
        }
        LpirOp::Memcpy {
            dst_addr,
            src_addr,
            size,
        } => {
            sink.local_get(dst_addr.0)
                .local_get(src_addr.0)
                .i32_const(i32::try_from(*size).unwrap_or(i32::MAX))
                .memory_copy(0, 0);
        }
    }
    Ok(())
}
