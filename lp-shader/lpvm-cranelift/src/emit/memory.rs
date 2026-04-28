use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::{FunctionBuilder, Variable};
use lpir::lpir_module::IrFunction;
use lpir::lpir_op::LpirOp;
use lpir::types::VReg;

use super::{EmitCtx, def_v, ir_type_for_mode, use_v};
use crate::error::CompileError;

pub(crate) fn emit_memory(
    op: &LpirOp,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctx: &EmitCtx,
) -> Result<bool, CompileError> {
    match op {
        LpirOp::SlotAddr { dst, slot } => {
            let ss = *ctx
                .slots
                .get(slot.0 as usize)
                .ok_or_else(|| CompileError::unsupported("invalid stack slot index"))?;
            let addr = builder.ins().stack_addr(ctx.pointer_type, ss, 0);
            def_v(builder, vars, *dst, addr);
        }
        LpirOp::Load { dst, base, offset } => {
            let ptr = operand_as_ptr(builder, vars, ctx, *base);
            let ty = ir_type_for_mode(
                func.vreg_types[dst.0 as usize],
                ctx.float_mode,
                ctx.pointer_type,
            );
            let val = builder.ins().load(
                ty,
                MemFlags::new(),
                ptr,
                i32::try_from(*offset)
                    .map_err(|_| CompileError::unsupported("load offset does not fit in i32"))?,
            );
            def_v(builder, vars, *dst, val);
        }
        LpirOp::Store {
            base,
            offset,
            value,
        } => {
            let ptr = operand_as_ptr(builder, vars, ctx, *base);
            let val = use_v(builder, vars, *value);
            builder.ins().store(
                MemFlags::new(),
                val,
                ptr,
                i32::try_from(*offset)
                    .map_err(|_| CompileError::unsupported("store offset does not fit in i32"))?,
            );
        }
        LpirOp::Store8 {
            base,
            offset,
            value,
        } => {
            let ptr = operand_as_ptr(builder, vars, ctx, *base);
            let val = use_v(builder, vars, *value);
            builder.ins().istore8(
                MemFlags::new(),
                val,
                ptr,
                i32::try_from(*offset)
                    .map_err(|_| CompileError::unsupported("store8 offset does not fit in i32"))?,
            );
        }
        LpirOp::Store16 {
            base,
            offset,
            value,
        } => {
            let ptr = operand_as_ptr(builder, vars, ctx, *base);
            let val = use_v(builder, vars, *value);
            builder.ins().istore16(
                MemFlags::new(),
                val,
                ptr,
                i32::try_from(*offset)
                    .map_err(|_| CompileError::unsupported("store16 offset does not fit in i32"))?,
            );
        }
        LpirOp::Load8U { dst, base, offset } => {
            let ptr = operand_as_ptr(builder, vars, ctx, *base);
            let val = builder.ins().uload8(
                types::I32,
                MemFlags::new(),
                ptr,
                i32::try_from(*offset)
                    .map_err(|_| CompileError::unsupported("load8u offset does not fit in i32"))?,
            );
            def_v(builder, vars, *dst, val);
        }
        LpirOp::Load8S { dst, base, offset } => {
            let ptr = operand_as_ptr(builder, vars, ctx, *base);
            let val = builder.ins().sload8(
                types::I32,
                MemFlags::new(),
                ptr,
                i32::try_from(*offset)
                    .map_err(|_| CompileError::unsupported("load8s offset does not fit in i32"))?,
            );
            def_v(builder, vars, *dst, val);
        }
        LpirOp::Load16U { dst, base, offset } => {
            let ptr = operand_as_ptr(builder, vars, ctx, *base);
            let off = i32::try_from(*offset)
                .map_err(|_| CompileError::unsupported("load16u offset does not fit in i32"))?;
            let val = if ctx.riscv_decompose_load16u {
                // Cranelift RISC-V32 does not legalize sub-i32 memory ops (`uload16` / `uload8`).
                // Emulated RV32 rejects misaligned `lw`, so read LE u16 via two 4-byte aligned loads
                // and bitwise extract using the *full* address `(ptr+off) & 3` (ptr may be unaligned).
                let addr = if off == 0 {
                    ptr
                } else {
                    builder.ins().iadd_imm(ptr, i64::from(off))
                };
                let byte_lo = builder.ins().band_imm(addr, 3);
                let word0_base = builder.ins().band_imm(addr, -4);
                let w0 = builder
                    .ins()
                    .load(types::I32, MemFlags::new(), word0_base, 0);
                let word1_base = builder.ins().iadd_imm(word0_base, 4);
                let w1 = builder
                    .ins()
                    .load(types::I32, MemFlags::new(), word1_base, 0);
                let r0 = builder.ins().band_imm(w0, 0xffff);
                let r1 = {
                    let s = builder.ins().ushr_imm(w0, 8);
                    builder.ins().band_imm(s, 0xffff)
                };
                let r2 = builder.ins().ushr_imm(w0, 16);
                let lo = builder.ins().ushr_imm(w0, 24);
                let hi = builder.ins().band_imm(w1, 0xff);
                let hi_shift = builder.ins().ishl_imm(hi, 8);
                let r3 = builder.ins().bor(lo, hi_shift);
                let c2 = builder.ins().icmp_imm(IntCC::Equal, byte_lo, 2);
                let inner = builder.ins().select(c2, r2, r3);
                let c1 = builder.ins().icmp_imm(IntCC::Equal, byte_lo, 1);
                let mid = builder.ins().select(c1, r1, inner);
                let c0 = builder.ins().icmp_imm(IntCC::Equal, byte_lo, 0);
                builder.ins().select(c0, r0, mid)
            } else {
                builder.ins().uload16(types::I32, MemFlags::new(), ptr, off)
            };
            def_v(builder, vars, *dst, val);
        }
        LpirOp::Load16S { dst, base, offset } => {
            let ptr = operand_as_ptr(builder, vars, ctx, *base);
            let val = builder.ins().sload16(
                types::I32,
                MemFlags::new(),
                ptr,
                i32::try_from(*offset)
                    .map_err(|_| CompileError::unsupported("load16s offset does not fit in i32"))?,
            );
            def_v(builder, vars, *dst, val);
        }
        LpirOp::Memcpy {
            dst_addr,
            src_addr,
            size,
        } => {
            let dst_ptr = operand_as_ptr(builder, vars, ctx, *dst_addr);
            let src_ptr = operand_as_ptr(builder, vars, ctx, *src_addr);
            if *size % 4 != 0 {
                return Err(CompileError::unsupported(
                    "memcpy size must be a multiple of 4",
                ));
            }
            let mut off = 0i32;
            let size_i = i32::try_from(*size)
                .map_err(|_| CompileError::unsupported("memcpy size does not fit in i32"))?;
            while off < size_i {
                let chunk = builder
                    .ins()
                    .load(types::I32, MemFlags::new(), src_ptr, off);
                builder.ins().store(MemFlags::new(), chunk, dst_ptr, off);
                off += 4;
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn operand_as_ptr(
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctx: &EmitCtx,
    v: VReg,
) -> Value {
    let val = use_v(builder, vars, v);
    if builder.func.dfg.value_type(val) == ctx.pointer_type {
        return val;
    }
    if ctx
        .vreg_wide_addr
        .get(v.0 as usize)
        .copied()
        .unwrap_or(false)
    {
        val
    } else {
        widen_to_ptr(builder, val, ctx.pointer_type)
    }
}

pub(super) fn widen_to_ptr(
    builder: &mut FunctionBuilder,
    val: Value,
    ptr_type: types::Type,
) -> Value {
    if ptr_type == types::I32 {
        val
    } else {
        builder.ins().uextend(ptr_type, val)
    }
}
