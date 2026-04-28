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
    /// `IrFunction::sret_arg` is set (Cranelift `StructReturn` on the sret pointer param).
    pub uses_struct_return: bool,
    /// Per local function (IR rank, BTreeMap key order): does the callee's Cranelift signature
    /// use `StructReturn`? Needed at call sites to allocate a buffer for implicit multi-scalar
    /// sret callees (e.g. RV32 `vec3 foo()` callee with no `sret_arg` LPIR vreg) and load
    /// results back from the buffer.
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

/// True when [`signature_for_ir_func`] adds a `StructReturn` parameter for this `func` and ISA.
///
/// **Single source of truth** for “does this IR+ISA use a struct-return buffer?” — callers
/// must not re-derive this from [`IrFunction::sret_arg`] alone (implicit scalar sret has no
/// `sret_arg`) or by scanning `Signature::params`.
///
/// Two cases:
/// 1. **Explicit LPIR sret:** [`IrFunction::sret_arg`] is set (M1 aggregate, any target).
/// 2. **Implicit ABI sret:** the ISA cannot return `func.return_types.len()` scalars in
///    registers (RV32: more than 2 `i32` returns; other hosts: more than 4).
///
/// `lpvm-native`'s RV32 backend classifies vec3+/mat scalar returns the same way (see
/// `classify_return`); Cranelift signatures must match or the emulator and callee disagree
/// on argument slots and where return values live.
pub fn signature_uses_struct_return(isa: &dyn TargetIsa, func: &IrFunction) -> bool {
    if func.sret_arg.is_some() {
        return true;
    }
    let n = func.return_types.len();
    if n == 0 {
        return false;
    }
    use target_lexicon::Architecture;
    match isa.triple().architecture {
        Architecture::Riscv32(_) => n > 2,
        _ => n > 4,
    }
}

/// Build the Cranelift [`Signature`] for `func`, including StructReturn for explicit
/// (M1 `sret_arg`) and implicit multi-scalar (RV32 vec3+/mat, host >4 scalars) returns.
///
/// Param order is `[sret?, vmctx, user_params…]` — sret comes first (RV32 SystemV / x86-64
/// SysV / Apple AArch64 `x8` convention), so positional ABIs (e.g. `lp-riscv-emu`'s
/// `compute_arg_locations_for_emulator`) place the sret pointer in `a0` / `rdi` where
/// `lpvm-native`'s `classify_return.ptr_reg = A0` expects it. LPIR vreg ordering
/// (`vmctx_vreg=0, sret_arg=1` then user vregs) is independent of this Cranelift block-
/// param order; [`translate_function`] maps them by name.
///
/// When sret is present, returns live in the buffer and `Signature::returns` is empty.
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
        let mut sret = AbiParam::new(pointer_type);
        sret.purpose = ArgumentPurpose::StructReturn;
        sig.params.push(sret);
    }
    sig.params.push(AbiParam::new(pointer_type));
    // User params: vregs start at vmctx + hidden_param_slots (vmctx + optional sret).
    let vm = func.vmctx_vreg.0 as usize;
    let h = func.hidden_param_slots() as usize;
    for i in 0..func.param_count as usize {
        let ty = func.vreg_types[vm + h + i];
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
    // Block params: [sret? (StructReturn), vmctx, user1, …]. LPIR vreg numbering is independent:
    // vmctx_vreg = 0, sret_arg = 1 (when present), then user_param_vreg(0..).
    if let Some(sv) = func.sret_arg {
        if pi < params.len() {
            def_v(builder, &vars, sv, params[pi]);
            pi += 1;
        }
    } else if ctx.uses_struct_return {
        // Implicit multi-scalar sret: the signature has a StructReturn param, but LPIR has no
        // sret vreg (returns are still scalars in `func.return_types`). Skip it; the back end
        // writes returns through a backend-private path (e.g. `lpvm-native`'s
        // `ReturnMethod::Sret`) rather than through an LPIR-visible pointer.
        if pi < params.len() {
            pi += 1;
        }
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
    use lpir::types::VReg;
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

    /// `invoke_sysv_struct_return_buf` and `lpvm-native`'s `ReturnMethod::Sret { ptr_reg: A0 }`
    /// expect: sret buffer pointer FIRST (RV32 `a0`, x86-64 `rdi`), then vmctx, then user scalars.
    #[test]
    fn riscv32_sret_marker_places_struct_return_first() {
        let isa = riscv32_isa();
        let ptr_ty = isa.pointer_type();
        let func = IrFunction {
            name: String::from("ret_arr"),
            is_entry: true,
            vmctx_vreg: VMCTX_VREG,
            param_count: 1,
            return_types: vec![],
            sret_arg: Some(VReg(1)),
            vreg_types: vec![IrType::Pointer, IrType::Pointer, IrType::I32],
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
        assert_eq!(sig.params[0].purpose, ArgumentPurpose::StructReturn, "sret");
        assert_eq!(sig.params[1].purpose, ArgumentPurpose::Normal, "vmctx");
        assert_eq!(sig.params[2].purpose, ArgumentPurpose::Normal, "user");
        assert!(
            sig.returns.is_empty(),
            "StructReturn ABI: returns live in the buffer, not in Signature::returns"
        );
    }

    /// Regression: vec3+ scalar returns on RV32 must still use StructReturn so `lp-riscv-emu`
    /// matches `lpvm-native`'s `classify_return` (implicit ABI sret when `sret_arg` is unset).
    #[test]
    fn riscv32_vec3_implicit_abi_uses_struct_return_without_sret_arg() {
        let isa = riscv32_isa();
        let ptr_ty = isa.pointer_type();
        let func = IrFunction {
            name: String::from("ret_vec3"),
            is_entry: false,
            vmctx_vreg: VMCTX_VREG,
            param_count: 0,
            return_types: vec![IrType::I32, IrType::I32, IrType::I32],
            sret_arg: None,
            vreg_types: vec![IrType::Pointer],
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
        assert_eq!(sig.params.len(), 2, "sret + vmctx");
        assert_eq!(sig.params[0].purpose, ArgumentPurpose::StructReturn, "sret");
        assert_eq!(sig.params[1].purpose, ArgumentPurpose::Normal, "vmctx");
        assert!(
            sig.returns.is_empty(),
            "Legacy multi-scalar sret: returns live in the buffer"
        );
    }
}
