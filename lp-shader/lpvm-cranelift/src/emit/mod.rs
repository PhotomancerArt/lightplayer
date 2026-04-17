//! LPIR → CLIF translation: scalar ops, structured control flow, memory, and local calls.

use alloc::vec::Vec;

use alloc::collections::BTreeMap;
use cranelift_codegen::ir::{AbiParam, ArgumentPurpose, Signature, types};
use cranelift_codegen::ir::{Block, FuncRef, InstBuilder, StackSlot, TrapCode, Value};
use cranelift_codegen::isa::{CallConv, TargetIsa};
use cranelift_frontend::{FunctionBuilder, Variable};

use lpir::FloatMode;
use lpir::lpir_module::{IrFunction, LpirModule};
use lpir::lpir_op::LpirOp;
use lpir::types::FuncId as LpirFuncId;
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
    pub ir: &'a LpirModule,
    /// Rank `0..functions.len()-1` for each [`LpirFuncId`] (BTreeMap key order).
    pub func_id_to_ir_rank: &'a BTreeMap<LpirFuncId, usize>,
    pub pointer_type: types::Type,
    /// `SlotAddr` definition and transitive `Iadd` results use native pointer SSA type (see `vreg_wide_addr_chain`).
    pub vreg_wide_addr: Vec<bool>,
    pub float_mode: FloatMode,
    pub lpir_builtins: Option<LpirBuiltinRefs>,
    /// This function uses Cranelift `StructReturn` (RISC-V32: >2 scalar returns).
    pub uses_struct_return: bool,
    /// Per user function: same as [`signature_uses_struct_return`].
    pub callee_struct_return: &'a [bool],
}

pub(crate) enum CtrlFrame {
    If {
        else_block: Block,
        merge_block: Block,
    },
    Else {
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
        merge_block: Block,
        next_case_block: Block,
    },
    Default {
        merge_block: Block,
    },
    Block {
        merge_block: Block,
    },
}

/// RISC-V32 cannot return >2 scalars in registers; use a hidden StructReturn pointer (Cranelift #9510).
///
/// On host ISAs, the Level-3 invoke shim only models up to four GPR returns, so larger multi-return
/// (e.g. `mat3`/`mat4`) also uses `StructReturn` with a caller-allocated buffer.
pub(crate) fn signature_uses_struct_return(isa: &dyn TargetIsa, func: &IrFunction) -> bool {
    use target_lexicon::Architecture;
    let n = func.return_types.len();
    if n == 0 {
        return false;
    }
    match isa.triple().architecture {
        Architecture::Riscv32(_) => n > 2,
        _ => n > 4,
    }
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
    // NOTE: When enable_multi_ret_implicit_sret is enabled, the backend treats the first
    // pointer arg as the implicit sret pointer. So we MUST put sret before vmctx.
    if sr {
        sig.params.push(AbiParam::special(
            pointer_type,
            ArgumentPurpose::StructReturn,
        ));
    }
    // vmctx uses the ISA pointer type (I32 on RV32, I64 on x86-64). StructReturn (when present)
    // is a separate special parameter; vmctx is a normal pointer argument.
    sig.params.push(AbiParam::new(pointer_type));
    let vm = func.vmctx_vreg.0 as usize;
    for i in 0..func.param_count as usize {
        let ty = func.vreg_types[vm + 1 + i];
        let ct = ir_type_for_mode(ty, mode, pointer_type);
        sig.params.push(AbiParam::new(ct));
    }
    if !sr {
        for t in &func.return_types {
            sig.returns
                .push(AbiParam::new(ir_type_for_mode(*t, mode, pointer_type)));
        }
    }
    sig
}

pub(crate) fn ir_type_for_mode(
    t: IrType,
    mode: FloatMode,
    pointer_type: types::Type,
) -> types::Type {
    match (t, mode) {
        (IrType::Pointer, _) => pointer_type,
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

/// Marks vregs whose SSA type is [`EmitCtx::pointer_type`]: stack slot addresses and `base + offset` chains.
pub(crate) fn vreg_wide_addr_chain(func: &IrFunction) -> Vec<bool> {
    let mut wide = vec![false; func.vreg_types.len()];
    for (i, ty) in func.vreg_types.iter().enumerate() {
        if matches!(ty, IrType::Pointer) {
            wide[i] = true;
        }
    }
    for op in &func.body {
        match op {
            LpirOp::SlotAddr { dst, .. } => wide[dst.0 as usize] = true,
            LpirOp::Iadd { dst, lhs, rhs } | LpirOp::Isub { dst, lhs, rhs } => {
                if wide[lhs.0 as usize] || wide[rhs.0 as usize] {
                    wide[dst.0 as usize] = true;
                }
            }
            LpirOp::IaddImm { dst, src, .. } | LpirOp::IsubImm { dst, src, .. } => {
                if wide[src.0 as usize] {
                    wide[dst.0 as usize] = true;
                }
            }
            _ => {}
        }
    }
    wide
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
        let wide = ctx.vreg_wide_addr.get(i).copied().unwrap_or(false);
        let ct = if wide || matches!(*ty, IrType::Pointer) {
            ctx.pointer_type
        } else {
            ir_type_for_mode(*ty, ctx.float_mode, ctx.pointer_type)
        };
        vars.push(builder.declare_var(ct));
    }

    let entry = builder.current_block().expect("entry block");
    let params: Vec<Value> = builder.block_params(entry).to_vec();
    let mut pi = 0usize;
    // NOTE: Signature order is: [sret (if present), vmctx, user1, user2, ...]
    if ctx.uses_struct_return {
        // Skip sret pointer (first param when struct return is used)
        pi += 1;
    }
    if pi < params.len() {
        def_v(builder, &vars, func.vmctx_vreg, params[pi]);
        pi += 1;
    }
    for user_i in 0..func.param_count as usize {
        if pi < params.len() {
            def_v(
                builder,
                &vars,
                func.user_param_vreg(user_i as u16),
                params[pi],
            );
            pi += 1;
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

#[cfg(test)]
mod struct_return_signature_tests {
    use alloc::string::String;
    use alloc::vec::Vec;

    use cranelift_codegen::ir::ArgumentPurpose;
    use cranelift_codegen::isa::CallConv;
    use cranelift_codegen::settings;
    use cranelift_codegen::settings::Configurable;
    use lpir::{FloatMode, IrFunction, IrType, VMCTX_VREG};
    use target_lexicon::Triple;

    use super::signature_for_ir_func;

    fn riscv32_isa() -> cranelift_codegen::isa::OwnedTargetIsa {
        let triple: Triple = "riscv32imac-unknown-none-elf".parse().unwrap();
        let mut b = settings::builder();
        b.set("regalloc_algorithm", "single_pass").unwrap();
        b.set("is_pic", "false").unwrap();
        let flags = settings::Flags::new(b);
        cranelift_codegen::isa::lookup(triple)
            .unwrap()
            .finish(flags)
            .unwrap()
    }

    /// `invoke_sysv_struct_return_buf` must pass arguments in the same order as here: sret, vmctx,
    /// then user scalars (see `signature_for_ir_func` when `enable_multi_ret_implicit_sret` applies).
    #[test]
    fn riscv32_vec4_return_struct_return_param_before_vmctx() {
        let isa = riscv32_isa();
        let ptr_ty = isa.pointer_type();
        let func = IrFunction {
            name: String::from("render"),
            is_entry: true,
            vmctx_vreg: VMCTX_VREG,
            param_count: 1,
            return_types: vec![IrType::I32, IrType::I32, IrType::I32, IrType::I32],
            vreg_types: vec![IrType::Pointer, IrType::I32],
            slots: Vec::new(),
            body: Vec::new(),
            vreg_pool: Vec::new(),
        };
        let sig = signature_for_ir_func(
            &func,
            CallConv::SystemV,
            FloatMode::Q32,
            ptr_ty,
            isa.as_ref(),
        );
        assert_eq!(sig.params.len(), 3, "sret + vmctx + one user i32");
        assert_eq!(sig.params[0].purpose, ArgumentPurpose::StructReturn);
        assert_eq!(sig.params[1].purpose, ArgumentPurpose::Normal);
        assert_eq!(sig.params[2].purpose, ArgumentPurpose::Normal);
        assert!(
            sig.returns.is_empty(),
            "StructReturn ABI: returns live in the buffer, not in Signature::returns"
        );
    }
}
