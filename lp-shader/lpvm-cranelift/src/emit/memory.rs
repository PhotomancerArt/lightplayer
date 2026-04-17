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
                i32::try_from(*offset).map_err(|_| {
                    CompileError::unsupported("store16 offset does not fit in i32")
                })?,
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
            let val = builder.ins().uload16(
                types::I32,
                MemFlags::new(),
                ptr,
                i32::try_from(*offset)
                    .map_err(|_| CompileError::unsupported("load16u offset does not fit in i32"))?,
            );
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
