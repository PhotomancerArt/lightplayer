//! Map each [`lpir::Op`] to WASM instructions.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{IrFunction, IrModule, IrType, Op};
use lps_naga::FloatMode;
use wasm_encoder::{BlockType, Ieee32, InstructionSink, ValType};

use crate::emit::control::{
    innermost_loop_break_depth, innermost_loop_continue_depth, innermost_switch_selector, switch_merge_open_depth,
    unwind_ctrl_after_return, CtrlEntry, WasmOpenDepth,
};
use crate::emit::imports;
use crate::emit::memory;
use crate::emit::q32;
use crate::emit::FuncEmitCtx;

fn wasm_func_index(ctx: &FuncEmitCtx<'_>, callee: lpir::CalleeRef) -> Result<u32, String> {
    let m = ctx.module;
    let k = callee.0 as usize;
    if k < m.import_remap.len() {
        m.import_remap[k].ok_or_else(|| format!("call to pruned import {k}"))
    } else {
        let j = callee.0 - m.full_import_count;
        Ok(m.filtered_import_count + j)
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
    ir: &IrModule,
    func: &IrFunction,
    _pc: usize,
    op: &Op,
    wasm_open: &mut WasmOpenDepth,
) -> Result<(), String> {
    // In unreachable mode, only process structural ops needed for stack balance.
    let is_structural = matches!(
        op,
        Op::End
            | Op::Else
            | Op::SwitchStart { .. }
            | Op::CaseStart { .. }
            | Op::DefaultStart { .. }
    );

    if fctx.unreachable_mode && !is_structural {
        return Ok(());
    }
    let fm = fctx.module.options.float_mode;
    match op {
        Op::Iadd { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_add()
                .local_set(dst.0);
        }
        Op::Isub { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_sub()
                .local_set(dst.0);
        }
        Op::Imul { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_mul()
                .local_set(dst.0);
        }
        Op::IdivS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_div_s()
                .local_set(dst.0);
        }
        Op::IdivU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_div_u()
                .local_set(dst.0);
        }
        Op::IremS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_rem_s()
                .local_set(dst.0);
        }
        Op::IremU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_rem_u()
                .local_set(dst.0);
        }
        Op::Ineg { dst, src } => {
            sink.i32_const(0)
                .local_get(src.0)
                .i32_sub()
                .local_set(dst.0);
        }
        Op::Ieq { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_eq()
                .local_set(dst.0);
        }
        Op::Ine { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_ne()
                .local_set(dst.0);
        }
        Op::IltS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_lt_s()
                .local_set(dst.0);
        }
        Op::IleS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_le_s()
                .local_set(dst.0);
        }
        Op::IgtS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_gt_s()
                .local_set(dst.0);
        }
        Op::IgeS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_ge_s()
                .local_set(dst.0);
        }
        Op::IltU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_lt_u()
                .local_set(dst.0);
        }
        Op::IleU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_le_u()
                .local_set(dst.0);
        }
        Op::IgtU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_gt_u()
                .local_set(dst.0);
        }
        Op::IgeU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_ge_u()
                .local_set(dst.0);
        }
        Op::Iand { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_and()
                .local_set(dst.0);
        }
        Op::Ior { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_or()
                .local_set(dst.0);
        }
        Op::Ixor { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_xor()
                .local_set(dst.0);
        }
        Op::Ibnot { dst, src } => {
            sink.i32_const(-1)
                .local_get(src.0)
                .i32_xor()
                .local_set(dst.0);
        }
        Op::Ishl { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_shl()
                .local_set(dst.0);
        }
        Op::IshrS { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_shr_s()
                .local_set(dst.0);
        }
        Op::IshrU { dst, lhs, rhs } => {
            sink.local_get(lhs.0)
                .local_get(rhs.0)
                .i32_shr_u()
                .local_set(dst.0);
        }
        Op::IconstI32 { dst, value } => {
            sink.i32_const(*value).local_set(dst.0);
        }
        Op::IaddImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_add()
                .local_set(dst.0);
        }
        Op::IsubImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_sub()
                .local_set(dst.0);
        }
        Op::ImulImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_mul()
                .local_set(dst.0);
        }
        Op::IshlImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_shl()
                .local_set(dst.0);
        }
        Op::IshrSImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_shr_s()
                .local_set(dst.0);
        }
        Op::IshrUImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_shr_u()
                .local_set(dst.0);
        }
        Op::IeqImm { dst, src, imm } => {
            sink.local_get(src.0)
                .i32_const(*imm)
                .i32_eq()
                .local_set(dst.0);
        }
        Op::Select {
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
        Op::Copy { dst, src } => {
            sink.local_get(src.0).local_set(dst.0);
        }
        Op::IfStart { cond, .. } => {
            sink.local_get(cond.0).if_(BlockType::Empty);
            *wasm_open += 1;
            ctrl.push(CtrlEntry::If);
        }
        Op::Else => {
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
        Op::End => {
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
                Some(other) => {
                    ctrl.push(other);
                    return Err(String::from("unexpected `End` for control stack state"));
                }
                None => return Err(String::from("unexpected `End` (empty control stack)")),
            }
        }
        Op::LoopStart {
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
        Op::SwitchStart { selector, .. } => {
            sink.block(BlockType::Empty);
            *wasm_open += 1;
            ctrl.push(CtrlEntry::Switch {
                selector: selector.0,
                merge_wasm_open: *wasm_open,
            });
        }
        Op::CaseStart { value, .. } => {
            let sel = innermost_switch_selector(ctrl)?;
            sink.local_get(sel)
                .i32_const(*value)
                .i32_eq()
                .if_(BlockType::Empty);
            *wasm_open += 1;
            ctrl.push(CtrlEntry::SwitchCaseArm);
        }
        Op::DefaultStart { .. } => {
            ctrl.push(CtrlEntry::SwitchDefaultArm);
        }
        Op::Break => {
            let d = innermost_loop_break_depth(ctrl, *wasm_open)?;
            sink.br(d);
        }
        Op::Continue => {
            let d = innermost_loop_continue_depth(ctrl, *wasm_open)?;
            sink.br(d);
        }
        Op::BrIfNot { cond } => {
            let d = innermost_loop_break_depth(ctrl, *wasm_open)?;
            sink.local_get(cond.0).i32_eqz().br_if(d);
        }
        Op::Return { values } => {
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
        Op::Call {
            callee,
            args,
            results,
        } => {
            let idx = wasm_func_index(fctx, *callee)?;
            let callee_usize = callee.0 as usize;
            let is_import = callee_usize < fctx.module.full_import_count as usize;
            let is_result_ptr =
                is_import && imports::import_uses_result_pointer_abi(ir, callee_usize);

            let all_args = func.pool_slice(*args);
            let import_needs_vmctx = is_import
                && ir
                    .imports
                    .get(callee_usize)
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
        Op::FconstF32 { dst, value } => match fm {
            FloatMode::Q32 => {
                let q = q32::f32_to_q16_16(*value);
                sink.i32_const(q).local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.f32_const(Ieee32::from(*value)).local_set(dst.0);
            }
        },
        Op::Fadd { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                let s = fctx
                    .i64_scratch
                    .ok_or_else(|| String::from("internal: Q32 Fadd without i64 scratch local"))?;
                q32::emit_q32_fadd(sink, lhs.0, rhs.0, dst.0, s);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_add()
                    .local_set(dst.0);
            }
        },
        Op::Fsub { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                let s = fctx
                    .i64_scratch
                    .ok_or_else(|| String::from("internal: Q32 Fsub without i64 scratch local"))?;
                q32::emit_q32_fsub(sink, lhs.0, rhs.0, dst.0, s);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_sub()
                    .local_set(dst.0);
            }
        },
        Op::Fmul { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                let s = fctx
                    .i64_scratch
                    .ok_or_else(|| String::from("internal: Q32 Fmul without i64 scratch local"))?;
                q32::emit_q32_fmul(sink, lhs.0, rhs.0, dst.0, s);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_mul()
                    .local_set(dst.0);
            }
        },
        Op::Fdiv { dst, lhs, rhs } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_fdiv(sink, lhs.0, rhs.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(lhs.0)
                    .local_get(rhs.0)
                    .f32_div()
                    .local_set(dst.0);
            }
        },
        Op::Fneg { dst, src } => match fm {
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
        Op::Fabs { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_fabs(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_abs().local_set(dst.0);
            }
        },
        Op::Fsqrt { dst, src } => match fm {
            FloatMode::Q32 => {
                let callee = imports::import_callee(ir, "lpir", "sqrt")?;
                let idx = wasm_func_index(fctx, callee)?;
                sink.local_get(src.0).call(idx).local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_sqrt().local_set(dst.0);
            }
        },
        Op::Fmin { dst, lhs, rhs } => match fm {
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
        Op::Fmax { dst, lhs, rhs } => match fm {
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
        Op::Ffloor { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_ffloor(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_floor().local_set(dst.0);
            }
        },
        Op::Fceil { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_fceil(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_ceil().local_set(dst.0);
            }
        },
        Op::Ftrunc { dst, src } => match fm {
            FloatMode::Q32 => {
                q32::emit_q32_ftrunc(sink, src.0, dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_trunc().local_set(dst.0);
            }
        },
        Op::Fnearest { dst, src } => match fm {
            FloatMode::Q32 => {
                let callee = imports::import_callee(ir, "glsl", "round")?;
                let idx = wasm_func_index(fctx, callee)?;
                sink.local_get(src.0).call(idx).local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_nearest().local_set(dst.0);
            }
        },
        Op::Feq { dst, lhs, rhs } => match fm {
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
        Op::Fne { dst, lhs, rhs } => match fm {
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
        Op::Flt { dst, lhs, rhs } => match fm {
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
        Op::Fle { dst, lhs, rhs } => match fm {
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
        Op::Fgt { dst, lhs, rhs } => match fm {
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
        Op::Fge { dst, lhs, rhs } => match fm {
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
        Op::FtoiSatS { dst, src } => match fm {
            FloatMode::Q32 => {
                sink.local_get(src.0)
                    .i32_const(16)
                    .i32_shr_s()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).i32_trunc_sat_f32_s().local_set(dst.0);
            }
        },
        Op::FtoiSatU { dst, src } => match fm {
            FloatMode::Q32 => {
                sink.local_get(src.0)
                    .i32_const(16)
                    .i32_shr_u()
                    .local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).i32_trunc_sat_f32_u().local_set(dst.0);
            }
        },
        Op::ItofS { dst, src } => match fm {
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
        Op::ItofU { dst, src } => match fm {
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
        Op::FfromI32Bits { dst, src } => match fm {
            FloatMode::Q32 => {
                sink.local_get(src.0).local_set(dst.0);
            }
            FloatMode::F32 => {
                sink.local_get(src.0).f32_reinterpret_i32().local_set(dst.0);
            }
        },
        Op::SlotAddr { dst, slot } => {
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
        Op::Load { dst, base, offset } => {
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
        Op::Store {
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
        Op::Memcpy {
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
