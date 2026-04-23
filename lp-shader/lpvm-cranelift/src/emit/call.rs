use alloc::vec::Vec;

use cranelift_codegen::ir::{InstBuilder, MemFlags, StackSlotData, StackSlotKind};
use cranelift_frontend::{FunctionBuilder, Variable};
use lpir::lpir_module::IrFunction;
use lpir::lpir_op::LpirOp;
use lpir::types::{CalleeRef, ImportId};

use super::{EmitCtx, def_v, ir_type_for_mode, use_v};
use crate::builtins::is_import_result_ptr_builtin;
use crate::error::CompileError;

pub(crate) fn emit_call(
    op: &LpirOp,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctx: &EmitCtx,
) -> Result<bool, CompileError> {
    match op {
        LpirOp::Call {
            callee,
            args,
            results,
        } => {
            let func_ref = match *callee {
                CalleeRef::Import(ImportId(i)) => *ctx
                    .import_func_refs
                    .get(i as usize)
                    .ok_or_else(|| CompileError::unsupported("call to unknown import index"))?,
                CalleeRef::Local(id) => {
                    let rank = *ctx.func_id_to_ir_rank.get(&id).ok_or_else(|| {
                        CompileError::unsupported("call to unknown local func id")
                    })?;
                    *ctx.func_refs.get(rank).ok_or_else(|| {
                        CompileError::unsupported("call to unknown local function index")
                    })?
                }
            };
            if let CalleeRef::Local(id) = *callee {
                let callee_ir = ctx.ir.functions.get(&id).ok_or_else(|| {
                    CompileError::unsupported("call to missing local function IR")
                })?;
                if callee_ir.sret_arg.is_some() {
                    // P3+ LPIR: args are [vmctx, sret, …]; no synthetic stack slot or load-back.
                    let arg_vals: Vec<_> = func
                        .pool_slice(*args)
                        .iter()
                        .map(|v| use_v(builder, vars, *v))
                        .collect();
                    let call = builder.ins().call(func_ref, &arg_vals);
                    let result_regs = func.pool_slice(*results);
                    let result_vals: Vec<_> = builder.inst_results(call).to_vec();
                    if !result_regs.is_empty() || !result_vals.is_empty() {
                        return Err(CompileError::cranelift(alloc::format!(
                            "LPIR sret call: expected no call results, got {} SSA / {} LPIR",
                            result_vals.len(),
                            result_regs.len()
                        )));
                    }
                    return Ok(true);
                }
            }

            // Handle builtins that use manual result-pointer ABI (e.g., LPFX functions that
            // return vectors via out-pointer). These are imports where the Cranelift signature
            // has no returns, but the LPIR import declaration expects multiple return values.
            if let CalleeRef::Import(ImportId(i)) = *callee {
                let import_idx = i as usize;
                let import_decl = &ctx.ir.imports[import_idx];
                if is_import_result_ptr_builtin(import_decl, ctx.pointer_type) {
                    let ret_n = import_decl.return_types.len();
                    let size_bytes = ret_n.checked_mul(4).ok_or_else(|| {
                        CompileError::unsupported("builtin result buffer size overflow")
                    })?;
                    let slot = builder.func.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        size_bytes as u32,
                        4,
                    ));
                    let base = builder.ins().stack_addr(ctx.pointer_type, slot, 0);
                    let mut arg_vals: Vec<_> = Vec::with_capacity(1 + func.pool_slice(*args).len());
                    arg_vals.push(base);
                    for v in func.pool_slice(*args) {
                        arg_vals.push(use_v(builder, vars, *v));
                    }
                    let call = builder.ins().call(func_ref, &arg_vals);
                    let result_regs = func.pool_slice(*results);
                    let result_vals: Vec<_> = builder.inst_results(call).to_vec();
                    if !result_vals.is_empty() {
                        return Err(CompileError::cranelift(alloc::format!(
                            "result-ptr builtin call should not produce SSA results, got {}",
                            result_vals.len()
                        )));
                    }
                    if result_regs.len() != ret_n {
                        return Err(CompileError::cranelift(alloc::format!(
                            "result-ptr builtin result arity mismatch: expected {}, got {}",
                            ret_n,
                            result_regs.len()
                        )));
                    }
                    for (idx, vreg) in result_regs.iter().enumerate() {
                        let offset = (idx * 4) as i32;
                        let ty = ir_type_for_mode(
                            import_decl.return_types[idx],
                            ctx.float_mode,
                            ctx.pointer_type,
                        );
                        let v = builder.ins().load(ty, MemFlags::trusted(), base, offset);
                        def_v(builder, vars, *vreg, v);
                    }
                    return Ok(true);
                }
            }

            if let CalleeRef::Import(ImportId(i)) = *callee {
                if ctx.ir.imports[i as usize].sret {
                    let arg_vals: Vec<_> = func
                        .pool_slice(*args)
                        .iter()
                        .map(|v| use_v(builder, vars, *v))
                        .collect();
                    let call = builder.ins().call(func_ref, &arg_vals);
                    let result_regs = func.pool_slice(*results);
                    let result_vals: Vec<_> = builder.inst_results(call).to_vec();
                    if !result_regs.is_empty() || !result_vals.is_empty() {
                        return Err(CompileError::cranelift(alloc::format!(
                            "LPIR import sret call: expected no call results, got {} SSA / {} LPIR",
                            result_vals.len(),
                            result_regs.len()
                        )));
                    }
                    return Ok(true);
                }
            }

            // VMContext is already in the Call args from lowering when the callee expects it
            // (shader functions and `ImportDecl::needs_vmctx` builtins).
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
        LpirOp::Return { values } => {
            let slice = func.pool_slice(*values);
            if ctx.uses_struct_return {
                if !slice.is_empty() {
                    return Err(CompileError::unsupported(
                        "LPIR sret function: return has values; use Memcpy to sret + empty Return",
                    ));
                }
                builder.ins().return_(&[]);
            } else {
                let mut vs = Vec::with_capacity(slice.len());
                for v in slice {
                    vs.push(use_v(builder, vars, *v));
                }
                builder.ins().return_(&vs);
            }
            super::switch_to_unreachable_tail(builder);
            Ok(true)
        }
        _ => Ok(false),
    }
}
