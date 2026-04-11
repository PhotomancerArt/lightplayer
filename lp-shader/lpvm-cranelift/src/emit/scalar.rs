use super::{EmitCtx, bool_to_i32, def_v, def_v_expr, memory, use_v};
use crate::error::CompileError;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, StackSlotData, StackSlotKind, types};
use cranelift_frontend::{FunctionBuilder, Variable};
use lpir::FloatMode;
use lpir::lpir_module::IrFunction;
use lpir::lpir_op::LpirOp;
use lpir::types::IrType;

fn q32_lpir_refs<'a>(ctx: &'a EmitCtx<'_>) -> Result<&'a super::LpirBuiltinRefs, CompileError> {
    ctx.lpir_builtins
        .as_ref()
        .ok_or_else(|| CompileError::unsupported("missing Q32 LPIR opcode builtins"))
}

pub(crate) fn emit_scalar(
    op: &LpirOp,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctx: &EmitCtx,
) -> Result<bool, CompileError> {
    match op {
        LpirOp::Fadd { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fadd(a, b)),
                FloatMode::Q32 => {
                    let refs = q32_lpir_refs(ctx)?;
                    let call = builder.ins().call(refs.fadd, &[a, b]);
                    let out = builder.inst_results(call)[0];
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fsub { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fsub(a, b)),
                FloatMode::Q32 => {
                    let refs = q32_lpir_refs(ctx)?;
                    let call = builder.ins().call(refs.fsub, &[a, b]);
                    let out = builder.inst_results(call)[0];
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fmul { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fmul(a, b)),
                FloatMode::Q32 => {
                    let refs = q32_lpir_refs(ctx)?;
                    let call = builder.ins().call(refs.fmul, &[a, b]);
                    let out = builder.inst_results(call)[0];
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fdiv { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fdiv(a, b)),
                FloatMode::Q32 => {
                    let refs = q32_lpir_refs(ctx)?;
                    let call = builder.ins().call(refs.fdiv, &[a, b]);
                    let out = builder.inst_results(call)[0];
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fneg { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fneg(a)),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_fneg(builder, a);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fabs { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fabs(a)),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_fabs(builder, a);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fsqrt { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().sqrt(a)),
                FloatMode::Q32 => {
                    let refs = q32_lpir_refs(ctx)?;
                    let call = builder.ins().call(refs.fsqrt, &[a]);
                    let out = builder.inst_results(call)[0];
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fmin { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fmin(a, b)),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_fmin(builder, a, b);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fmax { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fmax(a, b)),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_fmax(builder, a, b);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Ffloor { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().floor(a)),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_ffloor(builder, a);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fceil { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().ceil(a)),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_fceil(builder, a);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Ftrunc { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().trunc(a)),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_ftrunc(builder, a);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Fnearest { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().nearest(a)),
                FloatMode::Q32 => {
                    let refs = q32_lpir_refs(ctx)?;
                    let call = builder.ins().call(refs.fnearest, &[a]);
                    let out = builder.inst_results(call)[0];
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::Iadd { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| {
                let ta = bd.func.dfg.value_type(a);
                let tb = bd.func.dfg.value_type(b);
                let (a, b) = if ta == tb {
                    (a, b)
                } else if ta.bits() > tb.bits() {
                    (a, bd.ins().uextend(ta, b))
                } else {
                    (bd.ins().uextend(tb, a), b)
                };
                bd.ins().iadd(a, b)
            });
        }
        LpirOp::Isub { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().isub(a, b));
        }
        LpirOp::Imul { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().imul(a, b));
        }
        LpirOp::IdivS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().sdiv(a, b));
        }
        LpirOp::IdivU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().udiv(a, b));
        }
        LpirOp::IremS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().srem(a, b));
        }
        LpirOp::IremU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().urem(a, b));
        }
        LpirOp::Ineg { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().ineg(a));
        }
        LpirOp::Iand { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().band(a, b));
        }
        LpirOp::Ior { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().bor(a, b));
        }
        LpirOp::Ixor { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().bxor(a, b));
        }
        LpirOp::Ibnot { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().bnot(a));
        }
        LpirOp::Ishl { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().ishl(a, b));
        }
        LpirOp::IshrS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().sshr(a, b));
        }
        LpirOp::IshrU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().ushr(a, b));
        }
        LpirOp::FconstF32 { dst, value } => match ctx.float_mode {
            FloatMode::F32 => {
                def_v_expr(builder, vars, *dst, |bd| bd.ins().f32const(*value));
            }
            FloatMode::Q32 => {
                let encoded = lps_q32::q32_encode::q32_encode(*value);
                def_v_expr(builder, vars, *dst, |bd| {
                    bd.ins().iconst(types::I32, i64::from(encoded))
                });
            }
        },
        LpirOp::IconstI32 { dst, value } => {
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().iconst(types::I32, i64::from(*value))
            });
        }
        LpirOp::IaddImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().iadd_imm(a, i64::from(*imm))
            });
        }
        LpirOp::IsubImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            let immv = builder.ins().iconst(types::I32, i64::from(*imm));
            def_v_expr(builder, vars, *dst, |bd| bd.ins().isub(a, immv));
        }
        LpirOp::ImulImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().imul_imm(a, i64::from(*imm))
            });
        }
        LpirOp::IshlImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().ishl_imm(a, i64::from(*imm))
            });
        }
        LpirOp::IshrSImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().sshr_imm(a, i64::from(*imm))
            });
        }
        LpirOp::IshrUImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().ushr_imm(a, i64::from(*imm))
            });
        }
        LpirOp::IeqImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            let cmp = builder.ins().icmp_imm(IntCC::Equal, a, i64::from(*imm));
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::Ieq { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::Equal, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::Ine { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::NotEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::IltS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::SignedLessThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::IleS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::IgtS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::IgeS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::IltU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::UnsignedLessThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::IleU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::UnsignedLessThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::IgtU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::UnsignedGreaterThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::IgeU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        LpirOp::Feq { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => {
                    let cmp = builder.ins().fcmp(FloatCC::Equal, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
                FloatMode::Q32 => {
                    let cmp = builder.ins().icmp(IntCC::Equal, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
            }
        }
        LpirOp::Fne { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => {
                    let cmp = builder.ins().fcmp(FloatCC::NotEqual, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
                FloatMode::Q32 => {
                    let cmp = builder.ins().icmp(IntCC::NotEqual, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
            }
        }
        LpirOp::Flt { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => {
                    let cmp = builder.ins().fcmp(FloatCC::LessThan, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
                FloatMode::Q32 => {
                    let cmp = builder.ins().icmp(IntCC::SignedLessThan, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
            }
        }
        LpirOp::Fle { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => {
                    let cmp = builder.ins().fcmp(FloatCC::LessThanOrEqual, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
                FloatMode::Q32 => {
                    let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
            }
        }
        LpirOp::Fgt { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => {
                    let cmp = builder.ins().fcmp(FloatCC::GreaterThan, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
                FloatMode::Q32 => {
                    let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
            }
        }
        LpirOp::Fge { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            match ctx.float_mode {
                FloatMode::F32 => {
                    let cmp = builder.ins().fcmp(FloatCC::GreaterThanOrEqual, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
                FloatMode::Q32 => {
                    let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, a, b);
                    def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
                }
            }
        }
        LpirOp::FtoiSatS { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| {
                    bd.ins().fcvt_to_sint_sat(types::I32, a)
                }),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_to_sint(builder, a);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::FtoiSatU { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| {
                    bd.ins().fcvt_to_uint_sat(types::I32, a)
                }),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_to_uint(builder, a);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::ItofS { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| {
                    bd.ins().fcvt_from_sint(types::F32, a)
                }),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_from_sint(builder, a);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::ItofU { dst, src } => {
            let a = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| {
                    bd.ins().fcvt_from_uint(types::F32, a)
                }),
                FloatMode::Q32 => {
                    let out = crate::q32_emit::emit_from_uint(builder, a);
                    def_v(builder, vars, *dst, out);
                }
            }
        }
        LpirOp::FfromI32Bits { dst, src } => {
            let bits = use_v(builder, vars, *src);
            match ctx.float_mode {
                FloatMode::Q32 => {
                    def_v(builder, vars, *dst, bits);
                }
                FloatMode::F32 => {
                    let slot = builder.func.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        4,
                        4,
                    ));
                    builder.ins().stack_store(bits, slot, 0);
                    let f = builder.ins().stack_load(types::F32, slot, 0);
                    def_v(builder, vars, *dst, f);
                }
            }
        }
        LpirOp::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => {
            let dst_ir = func.vreg_types[dst.0 as usize];
            let c = use_v(builder, vars, *cond);
            let t_raw = use_v(builder, vars, *if_true);
            let t = match (
                dst_ir,
                func.vreg_types[if_true.0 as usize],
                ctx.pointer_type != types::I32,
            ) {
                (IrType::Pointer, IrType::I32, true) => {
                    memory::widen_to_ptr(builder, t_raw, ctx.pointer_type)
                }
                (IrType::I32, IrType::Pointer, true) => builder.ins().ireduce(types::I32, t_raw),
                _ => t_raw,
            };
            let f_raw = use_v(builder, vars, *if_false);
            let f_v = match (
                dst_ir,
                func.vreg_types[if_false.0 as usize],
                ctx.pointer_type != types::I32,
            ) {
                (IrType::Pointer, IrType::I32, true) => {
                    memory::widen_to_ptr(builder, f_raw, ctx.pointer_type)
                }
                (IrType::I32, IrType::Pointer, true) => builder.ins().ireduce(types::I32, f_raw),
                _ => f_raw,
            };
            let pred = builder.ins().icmp_imm(IntCC::NotEqual, c, 0);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().select(pred, t, f_v));
        }
        LpirOp::Copy { dst, src } => {
            let a = use_v(builder, vars, *src);
            let dst_ir = func.vreg_types[dst.0 as usize];
            let src_ir = func.vreg_types[src.0 as usize];
            let val = match (dst_ir, src_ir) {
                (IrType::Pointer, IrType::I32) if ctx.pointer_type != types::I32 => {
                    memory::widen_to_ptr(builder, a, ctx.pointer_type)
                }
                (IrType::I32, IrType::Pointer) if ctx.pointer_type != types::I32 => {
                    builder.ins().ireduce(types::I32, a)
                }
                _ => a,
            };
            def_v(builder, vars, *dst, val);
        }
        _ => return Ok(false),
    }
    let _ = func;
    Ok(true)
}
