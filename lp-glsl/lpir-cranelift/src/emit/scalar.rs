use super::{bool_to_i32, def_v, def_v_expr, use_v};
use crate::error::CompileError;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, types};
use cranelift_frontend::{FunctionBuilder, Variable};
use lpir::module::IrFunction;
use lpir::op::Op;

pub(crate) fn emit_scalar(
    op: &Op,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
) -> Result<bool, CompileError> {
    match op {
        Op::Fadd { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().fadd(a, b));
        }
        Op::Fsub { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().fsub(a, b));
        }
        Op::Fmul { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().fmul(a, b));
        }
        Op::Fdiv { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().fdiv(a, b));
        }
        Op::Fneg { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().fneg(a));
        }
        Op::Fabs { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().fabs(a));
        }
        Op::Fsqrt { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().sqrt(a));
        }
        Op::Fmin { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().fmin(a, b));
        }
        Op::Fmax { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().fmax(a, b));
        }
        Op::Ffloor { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().floor(a));
        }
        Op::Fceil { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().ceil(a));
        }
        Op::Ftrunc { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().trunc(a));
        }
        Op::Fnearest { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().nearest(a));
        }
        Op::Iadd { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().iadd(a, b));
        }
        Op::Isub { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().isub(a, b));
        }
        Op::Imul { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().imul(a, b));
        }
        Op::IdivS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().sdiv(a, b));
        }
        Op::IdivU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().udiv(a, b));
        }
        Op::IremS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().srem(a, b));
        }
        Op::IremU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().urem(a, b));
        }
        Op::Ineg { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().ineg(a));
        }
        Op::Iand { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().band(a, b));
        }
        Op::Ior { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().bor(a, b));
        }
        Op::Ixor { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().bxor(a, b));
        }
        Op::Ibnot { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().bnot(a));
        }
        Op::Ishl { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().ishl(a, b));
        }
        Op::IshrS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().sshr(a, b));
        }
        Op::IshrU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().ushr(a, b));
        }
        Op::FconstF32 { dst, value } => {
            def_v_expr(builder, vars, *dst, |bd| bd.ins().f32const(*value));
        }
        Op::IconstI32 { dst, value } => {
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().iconst(types::I32, i64::from(*value))
            });
        }
        Op::IaddImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().iadd_imm(a, i64::from(*imm))
            });
        }
        Op::IsubImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            let immv = builder.ins().iconst(types::I32, i64::from(*imm));
            def_v_expr(builder, vars, *dst, |bd| bd.ins().isub(a, immv));
        }
        Op::ImulImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().imul_imm(a, i64::from(*imm))
            });
        }
        Op::IshlImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().ishl_imm(a, i64::from(*imm))
            });
        }
        Op::IshrSImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().sshr_imm(a, i64::from(*imm))
            });
        }
        Op::IshrUImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().ushr_imm(a, i64::from(*imm))
            });
        }
        Op::IeqImm { dst, src, imm } => {
            let a = use_v(builder, vars, *src);
            let cmp = builder.ins().icmp_imm(IntCC::Equal, a, i64::from(*imm));
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::Ieq { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::Equal, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::Ine { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::NotEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::IltS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::SignedLessThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::IleS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::IgtS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::IgeS { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::IltU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::UnsignedLessThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::IleU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::UnsignedLessThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::IgtU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::UnsignedGreaterThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::IgeU { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::Feq { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().fcmp(FloatCC::Equal, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::Fne { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().fcmp(FloatCC::NotEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::Flt { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().fcmp(FloatCC::LessThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::Fle { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().fcmp(FloatCC::LessThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::Fgt { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().fcmp(FloatCC::GreaterThan, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::Fge { dst, lhs, rhs } => {
            let a = use_v(builder, vars, *lhs);
            let b = use_v(builder, vars, *rhs);
            let cmp = builder.ins().fcmp(FloatCC::GreaterThanOrEqual, a, b);
            def_v_expr(builder, vars, *dst, |bd| bool_to_i32(bd, cmp));
        }
        Op::FtoiSatS { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().fcvt_to_sint_sat(types::I32, a)
            });
        }
        Op::FtoiSatU { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().fcvt_to_uint_sat(types::I32, a)
            });
        }
        Op::ItofS { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().fcvt_from_sint(types::F32, a)
            });
        }
        Op::ItofU { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().fcvt_from_uint(types::F32, a)
            });
        }
        Op::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => {
            let c = use_v(builder, vars, *cond);
            let t = use_v(builder, vars, *if_true);
            let f_v = use_v(builder, vars, *if_false);
            let pred = builder.ins().icmp_imm(IntCC::NotEqual, c, 0);
            def_v_expr(builder, vars, *dst, |bd| bd.ins().select(pred, t, f_v));
        }
        Op::Copy { dst, src } => {
            let a = use_v(builder, vars, *src);
            def_v(builder, vars, *dst, a);
        }
        _ => return Ok(false),
    }
    let _ = func;
    Ok(true)
}
