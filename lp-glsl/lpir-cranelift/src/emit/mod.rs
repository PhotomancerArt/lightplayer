//! LPIR → CLIF translation: scalar ops, structured control flow, memory, and local calls.

use alloc::vec::Vec;

use cranelift_codegen::ir::{AbiParam, ArgumentPurpose, Signature, types};
use cranelift_codegen::ir::{Block, FuncRef, InstBuilder, StackSlot, TrapCode, Value};
use cranelift_codegen::isa::{CallConv, TargetIsa};
use cranelift_frontend::{FunctionBuilder, Variable};
use lpir::FloatMode;
use lpir::module::{IrFunction, IrModule};
use lpir::types::{IrType, VReg};

use crate::error::CompileError;

mod call;
mod control;
mod memory;
mod scalar;

/// FuncRefs for Q32 LPIR float ops lowered to `__lp_lpir_*_q32` calls.
pub(crate) struct LpirBuiltinRefs {
    pub fadd: FuncRef,
    pub fsub: FuncRef,
    pub fmul: FuncRef,
    pub fdiv: FuncRef,
    pub fsqrt: FuncRef,
    pub fnearest: FuncRef,
}

/// Per-function context for calls, stack slots, and pointer width.
pub(crate) struct EmitCtx<'a> {
    pub func_refs: &'a [FuncRef],
    pub import_func_refs: &'a [FuncRef],
    pub slots: &'a [StackSlot],
    pub ir: &'a IrModule,
    pub pointer_type: types::Type,
    /// `true` for VRegs defined by `slot_addr` — use [`Self::pointer_type`] for their SSA variable.
    pub vreg_is_stack_addr: Vec<bool>,
    pub float_mode: FloatMode,
    pub lpir_builtins: Option<LpirBuiltinRefs>,
    /// This function uses Cranelift `StructReturn` (RISC-V32: >2 scalar returns).
    pub uses_struct_return: bool,
    /// Per user function: same as [`signature_uses_struct_return`].
    pub callee_struct_return: &'a [bool],
}

pub(crate) enum CtrlFrame {
    If {
        then_block: Block,
        else_block: Block,
        merge_block: Block,
    },
    Else {
        else_block: Block,
        merge_block: Block,
    },
    Loop {
        header_block: Block,
        continue_block: Block,
        exit_block: Block,
        loop_start_pc: usize,
        continue_pc: usize,
    },
    Switch {
        selector: Value,
        merge_block: Block,
    },
    Case {
        body_block: Block,
        merge_block: Block,
        next_case_block: Block,
    },
    Default {
        entry_block: Block,
        merge_block: Block,
    },
}

/// RISC-V32 cannot return >2 scalars in registers; use a hidden StructReturn pointer (Cranelift #9510).
pub(crate) fn signature_uses_struct_return(isa: &dyn TargetIsa, func: &IrFunction) -> bool {
    use target_lexicon::Architecture;
    matches!(isa.triple().architecture, Architecture::Riscv32(_)) && func.return_types.len() > 2
}

/// Build the Cranelift [`Signature`] for `func`, including RISC-V32 StructReturn when required.
pub fn signature_for_ir_func(
    func: &IrFunction,
    call_conv: CallConv,
    mode: FloatMode,
    pointer_type: types::Type,
    isa: &dyn TargetIsa,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    let sr = signature_uses_struct_return(isa, func);
    if sr {
        sig.params.push(AbiParam::special(
            pointer_type,
            ArgumentPurpose::StructReturn,
        ));
    }
    for i in 0..func.param_count as usize {
        sig.params
            .push(AbiParam::new(ir_type_for_mode(func.vreg_types[i], mode)));
    }
    if !sr {
        for t in &func.return_types {
            sig.returns.push(AbiParam::new(ir_type_for_mode(*t, mode)));
        }
    }
    sig
}

pub(crate) fn ir_type_for_mode(t: IrType, mode: FloatMode) -> types::Type {
    match (t, mode) {
        (IrType::I32, _) => types::I32,
        (IrType::F32, FloatMode::F32) => types::F32,
        (IrType::F32, FloatMode::Q32) => types::I32,
    }
}

pub(crate) fn use_v(builder: &mut FunctionBuilder, vars: &[Variable], v: VReg) -> Value {
    let variable = vars[v.0 as usize];
    builder.use_var(variable)
}

pub(crate) fn def_v(builder: &mut FunctionBuilder, vars: &[Variable], v: VReg, val: Value) {
    builder.def_var(vars[v.0 as usize], val);
}

pub(crate) fn def_v_expr(
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    dst: VReg,
    f: impl FnOnce(&mut FunctionBuilder) -> Value,
) {
    let val = f(builder);
    def_v(builder, vars, dst, val);
}

pub(crate) fn bool_to_i32(builder: &mut FunctionBuilder, b: Value) -> Value {
    builder.ins().uextend(types::I32, b)
}

/// After an instruction that ends the current block (`return`, `jump`, etc.), switch to a fresh
/// block sealed with `trap` so `FunctionBuilder` invariants hold and later ops (e.g. `End`) do not
/// append to a filled block.
pub(crate) fn switch_to_unreachable_tail(builder: &mut FunctionBuilder) {
    let dead = builder.create_block();
    builder.switch_to_block(dead);
    builder.ins().trap(TrapCode::unwrap_user(1));
}

pub fn translate_function(
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    ctx: &EmitCtx,
) -> Result<(), CompileError> {
    let mut vars = Vec::with_capacity(func.vreg_types.len());
    for (i, ty) in func.vreg_types.iter().enumerate() {
        let ct = if ctx.vreg_is_stack_addr.get(i).copied().unwrap_or(false) {
            ctx.pointer_type
        } else {
            ir_type_for_mode(*ty, ctx.float_mode)
        };
        vars.push(builder.declare_var(ct));
    }

    let entry = builder.current_block().expect("entry block");
    let params: Vec<Value> = builder.block_params(entry).to_vec();
    let param_base = usize::from(ctx.uses_struct_return);
    for (i, val) in params.into_iter().enumerate() {
        if i < param_base {
            continue;
        }
        let user_idx = i - param_base;
        if (user_idx as u16) < func.param_count {
            def_v(builder, &vars, VReg(user_idx as u32), val);
        }
    }

    let mut ctrl_stack: Vec<CtrlFrame> = Vec::new();

    for (op_idx, op) in func.body.iter().enumerate() {
        control::maybe_enter_loop_continue_region(builder, &ctrl_stack, op_idx)?;
        if control::emit_control(op, func, builder, &vars, &mut ctrl_stack, op_idx)? {
            continue;
        }
        if memory::emit_memory(op, func, builder, &vars, ctx)? {
            continue;
        }
        if call::emit_call(op, func, builder, &vars, ctx)? {
            continue;
        }
        if scalar::emit_scalar(op, func, builder, &vars, ctx)? {
            continue;
        }
        return Err(CompileError::unsupported(format!(
            "unsupported LPIR op: {op:?}",
        )));
    }

    if !ctrl_stack.is_empty() {
        return Err(CompileError::unsupported(
            "unclosed control-flow region at end of function",
        ));
    }

    builder.seal_all_blocks();
    Ok(())
}
