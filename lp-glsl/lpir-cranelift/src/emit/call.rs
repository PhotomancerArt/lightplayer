use alloc::vec::Vec;

use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::{FunctionBuilder, Variable};
use lpir::module::IrFunction;
use lpir::op::Op;

use super::{EmitCtx, def_v, use_v};
use crate::error::CompileError;

pub(crate) fn emit_call(
    op: &Op,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctx: &EmitCtx,
) -> Result<bool, CompileError> {
    match op {
        Op::Call {
            callee,
            args,
            results,
        } => {
            let import_count = ctx.ir.imports.len() as u32;
            let func_ref = if callee.0 < import_count {
                *ctx.import_func_refs
                    .get(callee.0 as usize)
                    .ok_or_else(|| CompileError::unsupported("call to unknown import index"))?
            } else {
                let local_idx = (callee.0 - import_count) as usize;
                *ctx.func_refs.get(local_idx).ok_or_else(|| {
                    CompileError::unsupported("call to unknown local function index")
                })?
            };
            let arg_vals: Vec<_> = func
                .pool_slice(*args)
                .iter()
                .map(|v| use_v(builder, vars, *v))
                .collect();
            let call = builder.ins().call(func_ref, &arg_vals);
            let result_regs = func.pool_slice(*results);
            let result_vals: Vec<_> = builder.inst_results(call).to_vec();
            if result_regs.len() != result_vals.len() {
                return Err(CompileError::cranelift(alloc::format!(
                    "call result arity mismatch: expected {}, got {}",
                    result_regs.len(),
                    result_vals.len()
                )));
            }
            for (vreg, val) in result_regs.iter().zip(result_vals) {
                def_v(builder, vars, *vreg, val);
            }
            Ok(true)
        }
        Op::Return { values } => {
            let slice = func.pool_slice(*values);
            let mut vs = Vec::with_capacity(slice.len());
            for v in slice {
                vs.push(use_v(builder, vars, *v));
            }
            builder.ins().return_(&vs);
            super::switch_to_unreachable_tail(builder);
            Ok(true)
        }
        _ => Ok(false),
    }
}
