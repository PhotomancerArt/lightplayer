//! Well-formedness checks for [`LpirModule`] and [`IrFunction`].

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::lpir_module::{IrFunction, LpirModule};
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, ImportId, IrType, VReg, VRegRange};

/// Validation issue.
#[derive(Debug)]
pub struct ValidationError {
    pub message: String,
    pub op_index: Option<usize>,
    pub func_name: Option<String>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(fn_) = &self.func_name {
            write!(f, "@{fn_}: ")?;
        }
        if let Some(i) = self.op_index {
            write!(f, "op {i}: ")?;
        }
        write!(f, "{}", self.message)
    }
}

impl core::error::Error for ValidationError {}

fn err_in_func(
    func_name: &str,
    op_index: Option<usize>,
    msg: impl Into<String>,
) -> ValidationError {
    ValidationError {
        message: msg.into(),
        op_index,
        func_name: Some(String::from(func_name)),
    }
}

fn err_module(msg: impl Into<String>) -> ValidationError {
    ValidationError {
        message: msg.into(),
        op_index: None,
        func_name: None,
    }
}

/// Validate a full module (imports, functions, calls between them).
pub fn validate_module(module: &LpirModule) -> Result<(), Vec<ValidationError>> {
    let mut errs = Vec::new();
    validate_imports(module, &mut errs);
    let mut entry = 0u32;
    let mut seen_names: Vec<&str> = Vec::new();
    for f in module.functions.values() {
        if f.is_entry {
            entry += 1;
        }
        if seen_names.iter().any(|n| *n == f.name.as_str()) {
            errs.push(err_in_func(
                f.name.as_str(),
                None,
                format!("duplicate function @{}", f.name),
            ));
        }
        seen_names.push(f.name.as_str());
        validate_function_inner(f, module, &mut errs);
    }
    if entry > 1 {
        errs.push(err_module("at most one entry func"));
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

/// Validate one function in the context of its module (calls, etc.).
pub fn validate_function(
    func: &IrFunction,
    module: &LpirModule,
) -> Result<(), Vec<ValidationError>> {
    let mut errs = Vec::new();
    validate_function_inner(func, module, &mut errs);
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

fn validate_imports(module: &LpirModule, errs: &mut Vec<ValidationError>) {
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for imp in &module.imports {
        let key = (imp.module_name.as_str(), imp.func_name.as_str());
        if !seen.insert(key) {
            errs.push(err_module(format!(
                "duplicate import @{}::{}",
                imp.module_name, imp.func_name
            )));
        }
    }
}

enum StackEntry {
    If,
    Else,
    Loop {
        loop_start: usize,
        continuing_offset: u32,
    },
    /// Paired with [`LpirOp::End`] that closes the `Block` region.
    Block,
    Switch {
        cases: BTreeSet<i32>,
        default_arm: bool,
    },
    Arm,
}

fn has_enclosing_block(stack: &[StackEntry]) -> bool {
    stack.iter().rev().any(|e| matches!(e, StackEntry::Block))
}

fn validate_function_inner(
    func: &IrFunction,
    module: &LpirModule,
    errs: &mut Vec<ValidationError>,
) {
    let fname = func.name.as_str();
    pool_bounds(func, fname, errs);

    let vm = func.vmctx_vreg.0 as usize;
    if vm < func.vreg_types.len() && func.vreg_types[vm] != IrType::Pointer {
        errs.push(err_in_func(
            fname,
            None,
            alloc::format!(
                "vmctx v{} must have type ptr, got {:?}",
                func.vmctx_vreg.0,
                func.vreg_types[vm]
            ),
        ));
    }

    let n = func.vreg_types.len();
    let mut defined = vec![false; n];
    if vm < n {
        defined[vm] = true;
    }
    for i in 0..func.param_count as usize {
        let j = vm + 1 + i;
        if j < n {
            defined[j] = true;
        }
    }

    let mut stack: Vec<StackEntry> = Vec::new();

    for (i, op) in func.body.iter().enumerate() {
        let op_i = Some(i);

        match op {
            LpirOp::ExitBlock => {
                if !has_enclosing_block(&stack) {
                    errs.push(err_in_func(fname, op_i, "exit_block outside block"));
                }
            }
            LpirOp::Break | LpirOp::Continue | LpirOp::BrIfNot { .. } => {
                let innermost_loop = stack
                    .iter()
                    .rev()
                    .find(|e| matches!(e, StackEntry::Loop { .. }));
                if innermost_loop.is_none() {
                    errs.push(err_in_func(
                        fname,
                        op_i,
                        "break/continue/br_if_not outside loop",
                    ));
                } else if matches!(op, LpirOp::Continue) {
                    if let Some(StackEntry::Loop {
                        loop_start,
                        continuing_offset,
                    }) = innermost_loop
                    {
                        let co = *continuing_offset as usize;
                        if co > *loop_start + 1 && i >= co {
                            errs.push(err_in_func(
                                fname,
                                op_i,
                                "continue inside continuing section",
                            ));
                        }
                    }
                }
            }
            _ => {}
        }

        check_op_operands_defined(func, fname, op_i, op, &defined, errs);
        check_slot_memory_ops(func, fname, op_i, op, errs);
        check_opcode_dst_types(func, fname, op_i, op, errs);

        match op {
            LpirOp::IfStart { .. } => stack.push(StackEntry::If),
            LpirOp::Else => match stack.pop() {
                Some(StackEntry::If) => stack.push(StackEntry::Else),
                _ => errs.push(err_in_func(fname, op_i, "`else` without matching `if`")),
            },
            LpirOp::LoopStart {
                continuing_offset,
                end_offset,
            } => {
                let co = *continuing_offset as usize;
                if co < i + 1 {
                    errs.push(err_in_func(
                        fname,
                        op_i,
                        "LoopStart continuing_offset before body start",
                    ));
                }
                if co != i + 1 {
                    match func.body.get(co) {
                        Some(LpirOp::Continuing) => {}
                        _ => errs.push(err_in_func(
                            fname,
                            op_i,
                            "LoopStart continuing_offset must point at `continuing:` marker unless it is the first body op (legacy)",
                        )),
                    }
                }
                if *end_offset > 0 && *continuing_offset >= *end_offset {
                    errs.push(err_in_func(
                        fname,
                        op_i,
                        "LoopStart continuing_offset >= end_offset",
                    ));
                }
                stack.push(StackEntry::Loop {
                    loop_start: i,
                    continuing_offset: *continuing_offset,
                });
            }
            LpirOp::Continuing => {
                if !matches!(stack.last(), Some(StackEntry::Loop { .. })) {
                    errs.push(err_in_func(
                        fname,
                        op_i,
                        "`continuing:` must be directly inside a loop body (not nested in if/switch/block/inner loop)",
                    ));
                }
            }
            LpirOp::Block { end_offset } => {
                if *end_offset == 0 {
                    errs.push(err_in_func(
                        fname,
                        op_i,
                        "Block end_offset is zero (not patched)",
                    ));
                } else {
                    let eo = *end_offset as usize;
                    if eo > func.body.len() {
                        errs.push(err_in_func(
                            fname,
                            op_i,
                            "Block end_offset past function body",
                        ));
                    } else if eo <= i + 1 {
                        errs.push(err_in_func(
                            fname,
                            op_i,
                            "Block end_offset must follow block header",
                        ));
                    }
                }
                stack.push(StackEntry::Block);
            }
            LpirOp::SwitchStart { .. } => stack.push(StackEntry::Switch {
                cases: BTreeSet::new(),
                default_arm: false,
            }),
            LpirOp::CaseStart { value, .. } => {
                match stack.last_mut() {
                    Some(StackEntry::Switch {
                        cases,
                        default_arm: _,
                    }) => {
                        if !cases.insert(*value) {
                            errs.push(err_in_func(
                                fname,
                                op_i,
                                format!("duplicate switch case value {value}"),
                            ));
                        }
                    }
                    _ => errs.push(err_in_func(fname, op_i, "`case` outside `switch`")),
                }
                stack.push(StackEntry::Arm);
            }
            LpirOp::DefaultStart { .. } => {
                let ok = match stack.last_mut() {
                    Some(StackEntry::Switch {
                        cases: _,
                        default_arm,
                    }) => {
                        if *default_arm {
                            errs.push(err_in_func(
                                fname,
                                op_i,
                                "duplicate `default` arm in switch",
                            ));
                            false
                        } else {
                            *default_arm = true;
                            true
                        }
                    }
                    _ => {
                        errs.push(err_in_func(fname, op_i, "`default` outside `switch`"));
                        false
                    }
                };
                if ok {
                    stack.push(StackEntry::Arm);
                }
            }
            LpirOp::End => {
                if stack.pop().is_none() {
                    errs.push(err_in_func(
                        fname,
                        op_i,
                        "`End` underflow (extra closing brace)",
                    ));
                }
            }
            LpirOp::Call {
                callee,
                args,
                results,
            } => {
                validate_call(func, module, fname, op_i, *callee, *args, *results, errs);
            }
            LpirOp::Return { values } => {
                check_return_value_types(func, fname, op_i, *values, errs);
            }
            _ => {}
        }

        mark_op_defs(func, op, &mut defined);
    }

    if !stack.is_empty() {
        errs.push(err_in_func(
            fname,
            None,
            format!("unclosed control structure (stack depth {})", stack.len()),
        ));
    }
}

fn check_return_value_types(
    func: &IrFunction,
    fname: &str,
    op_i: Option<usize>,
    range: VRegRange,
    errs: &mut Vec<ValidationError>,
) {
    let slice = func.pool_slice(range);
    if slice.len() != range.count as usize {
        return;
    }
    for (k, v) in slice.iter().enumerate() {
        let j = v.0 as usize;
        if j < func.vreg_types.len() && k < func.return_types.len() {
            if func.vreg_types[j] != func.return_types[k] {
                errs.push(err_in_func(
                    fname,
                    op_i,
                    format!(
                        "return value {} type {:?} != declared {:?}",
                        k, func.vreg_types[j], func.return_types[k]
                    ),
                ));
            }
        }
    }
}

fn validate_call(
    func: &IrFunction,
    module: &LpirModule,
    fname: &str,
    op_i: Option<usize>,
    callee: CalleeRef,
    args: VRegRange,
    results: VRegRange,
    errs: &mut Vec<ValidationError>,
) {
    let arg_slice = func.pool_slice(args);
    let res_slice = func.pool_slice(results);

    if arg_slice.len() != args.count as usize || res_slice.len() != results.count as usize {
        return;
    }

    let mut import_param_scratch: Vec<IrType> = Vec::new();
    let (param_tys, ret_tys): (&[IrType], &[IrType]) = match callee {
        CalleeRef::Import(ImportId(i)) => {
            let i = i as usize;
            if i >= module.imports.len() {
                errs.push(err_in_func(
                    fname,
                    op_i,
                    format!("callee import index {i} out of range"),
                ));
                return;
            }
            let imp = &module.imports[i];
            import_param_scratch.clear();
            if imp.needs_vmctx {
                import_param_scratch.push(IrType::Pointer);
            }
            import_param_scratch.extend_from_slice(&imp.param_types);
            (import_param_scratch.as_slice(), imp.return_types.as_slice())
        }
        CalleeRef::Local(id) => {
            let Some(fdef) = module.functions.get(&id) else {
                errs.push(err_in_func(
                    fname,
                    op_i,
                    format!("callee unknown local func {}", id.0),
                ));
                return;
            };
            let vm = fdef.vmctx_vreg.0 as usize;
            let end = vm + 1 + fdef.param_count as usize;
            if end > fdef.vreg_types.len() {
                errs.push(err_in_func(
                    fname,
                    op_i,
                    "internal: callee signature missing",
                ));
                return;
            }
            (&fdef.vreg_types[..end], fdef.return_types.as_slice())
        }
    };

    if param_tys.len() != arg_slice.len() {
        errs.push(err_in_func(
            fname,
            op_i,
            format!(
                "call arg count {} != callee params {}",
                arg_slice.len(),
                param_tys.len()
            ),
        ));
    }
    if ret_tys.len() != res_slice.len() {
        errs.push(err_in_func(
            fname,
            op_i,
            format!(
                "call result count {} != callee returns {}",
                res_slice.len(),
                ret_tys.len()
            ),
        ));
    }

    for (k, v) in arg_slice.iter().enumerate() {
        if k < param_tys.len() {
            let j = v.0 as usize;
            if j < func.vreg_types.len() && func.vreg_types[j] != param_tys[k] {
                errs.push(err_in_func(
                    fname,
                    op_i,
                    format!(
                        "call arg {k} type {:?} != param {:?}",
                        func.vreg_types[j], param_tys[k]
                    ),
                ));
            }
        }
    }
    for (k, v) in res_slice.iter().enumerate() {
        if k < ret_tys.len() {
            let j = v.0 as usize;
            if j < func.vreg_types.len() && func.vreg_types[j] != ret_tys[k] {
                errs.push(err_in_func(
                    fname,
                    op_i,
                    format!(
                        "call result {k} type {:?} != return {:?}",
                        func.vreg_types[j], ret_tys[k]
                    ),
                ));
            }
        }
    }
}

fn check_op_operands_defined(
    func: &IrFunction,
    fname: &str,
    op_i: Option<usize>,
    op: &LpirOp,
    defined: &[bool],
    errs: &mut Vec<ValidationError>,
) {
    let mut check = |v: VReg, ctx: &str| {
        let j = v.0 as usize;
        if j >= defined.len() {
            errs.push(err_in_func(
                fname,
                op_i,
                format!("{ctx}: vreg index {} out of range", v.0),
            ));
        } else if !defined[j] {
            errs.push(err_in_func(
                fname,
                op_i,
                format!("{ctx}: v{v} used before definition"),
            ));
        }
    };

    match op {
        LpirOp::Fadd { lhs, rhs, .. }
        | LpirOp::Fsub { lhs, rhs, .. }
        | LpirOp::Fmul { lhs, rhs, .. }
        | LpirOp::Fdiv { lhs, rhs, .. }
        | LpirOp::Fmin { lhs, rhs, .. }
        | LpirOp::Fmax { lhs, rhs, .. } => {
            check(*lhs, "lhs");
            check(*rhs, "rhs");
        }
        LpirOp::Fneg { src, .. }
        | LpirOp::Fabs { src, .. }
        | LpirOp::Fsqrt { src, .. }
        | LpirOp::Ffloor { src, .. }
        | LpirOp::Fceil { src, .. }
        | LpirOp::Ftrunc { src, .. }
        | LpirOp::Fnearest { src, .. }
        | LpirOp::Ineg { src, .. }
        | LpirOp::Ibnot { src, .. } => check(*src, "src"),
        LpirOp::Iadd { lhs, rhs, .. }
        | LpirOp::Isub { lhs, rhs, .. }
        | LpirOp::Imul { lhs, rhs, .. }
        | LpirOp::IdivS { lhs, rhs, .. }
        | LpirOp::IdivU { lhs, rhs, .. }
        | LpirOp::IremS { lhs, rhs, .. }
        | LpirOp::IremU { lhs, rhs, .. } => {
            check(*lhs, "lhs");
            check(*rhs, "rhs");
        }
        LpirOp::Feq { lhs, rhs, .. }
        | LpirOp::Fne { lhs, rhs, .. }
        | LpirOp::Flt { lhs, rhs, .. }
        | LpirOp::Fle { lhs, rhs, .. }
        | LpirOp::Fgt { lhs, rhs, .. }
        | LpirOp::Fge { lhs, rhs, .. }
        | LpirOp::Ieq { lhs, rhs, .. }
        | LpirOp::Ine { lhs, rhs, .. }
        | LpirOp::IltS { lhs, rhs, .. }
        | LpirOp::IleS { lhs, rhs, .. }
        | LpirOp::IgtS { lhs, rhs, .. }
        | LpirOp::IgeS { lhs, rhs, .. }
        | LpirOp::IltU { lhs, rhs, .. }
        | LpirOp::IleU { lhs, rhs, .. }
        | LpirOp::IgtU { lhs, rhs, .. }
        | LpirOp::IgeU { lhs, rhs, .. }
        | LpirOp::Iand { lhs, rhs, .. }
        | LpirOp::Ior { lhs, rhs, .. }
        | LpirOp::Ixor { lhs, rhs, .. }
        | LpirOp::Ishl { lhs, rhs, .. }
        | LpirOp::IshrS { lhs, rhs, .. }
        | LpirOp::IshrU { lhs, rhs, .. } => {
            check(*lhs, "lhs");
            check(*rhs, "rhs");
        }
        LpirOp::IaddImm { src, .. }
        | LpirOp::IsubImm { src, .. }
        | LpirOp::ImulImm { src, .. }
        | LpirOp::IshlImm { src, .. }
        | LpirOp::IshrSImm { src, .. }
        | LpirOp::IshrUImm { src, .. }
        | LpirOp::IeqImm { src, .. } => check(*src, "src"),
        LpirOp::FtoiSatS { src, .. }
        | LpirOp::FtoiSatU { src, .. }
        | LpirOp::FtoUnorm16 { src, .. }
        | LpirOp::FtoUnorm8 { src, .. } => check(*src, "src"),
        LpirOp::ItofS { src, .. }
        | LpirOp::ItofU { src, .. }
        | LpirOp::FfromI32Bits { src, .. } => check(*src, "src"),
        LpirOp::Unorm16toF { src, .. } | LpirOp::Unorm8toF { src, .. } => check(*src, "src"),
        LpirOp::Select {
            cond,
            if_true,
            if_false,
            ..
        } => {
            check(*cond, "select cond");
            check(*if_true, "select if_true");
            check(*if_false, "select if_false");
        }
        LpirOp::Copy { src, .. } => check(*src, "copy src"),
        LpirOp::Load { base, .. }
        | LpirOp::Load8U { base, .. }
        | LpirOp::Load8S { base, .. }
        | LpirOp::Load16U { base, .. }
        | LpirOp::Load16S { base, .. } => check(*base, "load base"),
        LpirOp::Store { base, value, .. }
        | LpirOp::Store8 { base, value, .. }
        | LpirOp::Store16 { base, value, .. } => {
            check(*base, "store base");
            check(*value, "store value");
        }
        LpirOp::Memcpy {
            dst_addr, src_addr, ..
        } => {
            check(*dst_addr, "memcpy dst");
            check(*src_addr, "memcpy src");
        }
        LpirOp::IfStart { cond, .. } => check(*cond, "if cond"),
        LpirOp::SwitchStart { selector, .. } => check(*selector, "switch selector"),
        LpirOp::BrIfNot { cond } => check(*cond, "br_if_not cond"),
        LpirOp::Call { args, .. } => {
            for v in func.pool_slice(*args) {
                check(*v, "call arg");
            }
        }
        LpirOp::Return { values } => {
            for v in func.pool_slice(*values) {
                check(*v, "return");
            }
        }
        LpirOp::SlotAddr { .. }
        | LpirOp::FconstF32 { .. }
        | LpirOp::IconstI32 { .. }
        | LpirOp::Else
        | LpirOp::Continuing
        | LpirOp::LoopStart { .. }
        | LpirOp::CaseStart { .. }
        | LpirOp::DefaultStart { .. }
        | LpirOp::End
        | LpirOp::Break
        | LpirOp::Continue
        | LpirOp::Block { .. }
        | LpirOp::ExitBlock => {}
    }
}

fn check_slot_memory_ops(
    func: &IrFunction,
    fname: &str,
    op_i: Option<usize>,
    op: &LpirOp,
    errs: &mut Vec<ValidationError>,
) {
    if let LpirOp::SlotAddr { slot, .. } = op {
        if slot.0 as usize >= func.slots.len() {
            errs.push(err_in_func(
                fname,
                op_i,
                format!("slot_addr references undeclared slot ss{}", slot.0),
            ));
        }
    }
}

fn check_opcode_dst_types(
    func: &IrFunction,
    fname: &str,
    op_i: Option<usize>,
    op: &LpirOp,
    errs: &mut Vec<ValidationError>,
) {
    let mut expect = |dst: VReg, ty: IrType, ctx: &str| {
        let j = dst.0 as usize;
        if j >= func.vreg_types.len() {
            return;
        }
        if func.vreg_types[j] != ty {
            errs.push(err_in_func(
                fname,
                op_i,
                format!(
                    "{ctx}: v{} has type {:?}, expected {:?}",
                    dst.0, func.vreg_types[j], ty
                ),
            ));
        }
    };

    match op {
        LpirOp::Fadd { dst, .. }
        | LpirOp::Fsub { dst, .. }
        | LpirOp::Fmul { dst, .. }
        | LpirOp::Fdiv { dst, .. }
        | LpirOp::Fneg { dst, .. }
        | LpirOp::Fabs { dst, .. }
        | LpirOp::Fsqrt { dst, .. }
        | LpirOp::Fmin { dst, .. }
        | LpirOp::Fmax { dst, .. }
        | LpirOp::Ffloor { dst, .. }
        | LpirOp::Fceil { dst, .. }
        | LpirOp::Ftrunc { dst, .. }
        | LpirOp::Fnearest { dst, .. }
        | LpirOp::FconstF32 { dst, .. }
        | LpirOp::ItofS { dst, .. }
        | LpirOp::ItofU { dst, .. }
        | LpirOp::FfromI32Bits { dst, .. }
        | LpirOp::Unorm16toF { dst, .. }
        | LpirOp::Unorm8toF { dst, .. } => expect(*dst, IrType::F32, "float op result"),

        LpirOp::Iadd { dst, .. }
        | LpirOp::Isub { dst, .. }
        | LpirOp::Imul { dst, .. }
        | LpirOp::IdivS { dst, .. }
        | LpirOp::IdivU { dst, .. }
        | LpirOp::IremS { dst, .. }
        | LpirOp::IremU { dst, .. }
        | LpirOp::Ineg { dst, .. }
        | LpirOp::Feq { dst, .. }
        | LpirOp::Fne { dst, .. }
        | LpirOp::Flt { dst, .. }
        | LpirOp::Fle { dst, .. }
        | LpirOp::Fgt { dst, .. }
        | LpirOp::Fge { dst, .. }
        | LpirOp::Ieq { dst, .. }
        | LpirOp::Ine { dst, .. }
        | LpirOp::IltS { dst, .. }
        | LpirOp::IleS { dst, .. }
        | LpirOp::IgtS { dst, .. }
        | LpirOp::IgeS { dst, .. }
        | LpirOp::IltU { dst, .. }
        | LpirOp::IleU { dst, .. }
        | LpirOp::IgtU { dst, .. }
        | LpirOp::IgeU { dst, .. }
        | LpirOp::Iand { dst, .. }
        | LpirOp::Ior { dst, .. }
        | LpirOp::Ixor { dst, .. }
        | LpirOp::Ibnot { dst, .. }
        | LpirOp::Ishl { dst, .. }
        | LpirOp::IshrS { dst, .. }
        | LpirOp::IshrU { dst, .. }
        | LpirOp::IconstI32 { dst, .. }
        | LpirOp::IaddImm { dst, .. }
        | LpirOp::IsubImm { dst, .. }
        | LpirOp::ImulImm { dst, .. }
        | LpirOp::IshlImm { dst, .. }
        | LpirOp::IshrSImm { dst, .. }
        | LpirOp::IshrUImm { dst, .. }
        | LpirOp::IeqImm { dst, .. }
        | LpirOp::FtoiSatS { dst, .. }
        | LpirOp::FtoiSatU { dst, .. }
        | LpirOp::FtoUnorm16 { dst, .. }
        | LpirOp::FtoUnorm8 { dst, .. } => expect(*dst, IrType::I32, "integer op result"),

        LpirOp::Select {
            dst,
            if_true,
            if_false,
            ..
        } => {
            let j = dst.0 as usize;
            let t = if_true.0 as usize;
            let f = if_false.0 as usize;
            if j < func.vreg_types.len() && t < func.vreg_types.len() && f < func.vreg_types.len() {
                if func.vreg_types[t] != func.vreg_types[f] {
                    errs.push(err_in_func(
                        fname,
                        op_i,
                        format!("select arms v{if_true} and v{if_false} have different types"),
                    ));
                } else if func.vreg_types[j] != func.vreg_types[t] {
                    errs.push(err_in_func(
                        fname,
                        op_i,
                        "select result type does not match arm types",
                    ));
                }
            }
        }
        LpirOp::Copy { dst, src } => {
            let j = dst.0 as usize;
            let s = src.0 as usize;
            if j < func.vreg_types.len()
                && s < func.vreg_types.len()
                && func.vreg_types[j] != func.vreg_types[s]
            {
                errs.push(err_in_func(
                    fname,
                    op_i,
                    format!("copy: v{dst} and v{src} must have the same type"),
                ));
            }
        }
        LpirOp::SlotAddr { dst, .. } => expect(*dst, IrType::I32, "slot_addr"),
        LpirOp::Load8U { dst, .. }
        | LpirOp::Load8S { dst, .. }
        | LpirOp::Load16U { dst, .. }
        | LpirOp::Load16S { dst, .. } => expect(*dst, IrType::I32, "narrow load result"),
        LpirOp::Store8 { value, .. } | LpirOp::Store16 { value, .. } => {
            expect(*value, IrType::I32, "narrow store value");
        }
        LpirOp::Load { .. }
        | LpirOp::Store { .. }
        | LpirOp::Memcpy { .. }
        | LpirOp::IfStart { .. }
        | LpirOp::Else
        | LpirOp::Continuing
        | LpirOp::LoopStart { .. }
        | LpirOp::SwitchStart { .. }
        | LpirOp::CaseStart { .. }
        | LpirOp::DefaultStart { .. }
        | LpirOp::End
        | LpirOp::Break
        | LpirOp::Continue
        | LpirOp::BrIfNot { .. }
        | LpirOp::Call { .. }
        | LpirOp::Return { .. }
        | LpirOp::Block { .. }
        | LpirOp::ExitBlock => {}
    }
}

fn mark_op_defs(func: &IrFunction, op: &LpirOp, defined: &mut [bool]) {
    let mark = |v: VReg, d: &mut [bool]| {
        let j = v.0 as usize;
        if j < d.len() {
            d[j] = true;
        }
    };

    match op {
        LpirOp::Fadd { dst, .. }
        | LpirOp::Fsub { dst, .. }
        | LpirOp::Fmul { dst, .. }
        | LpirOp::Fdiv { dst, .. }
        | LpirOp::Fneg { dst, .. }
        | LpirOp::Fabs { dst, .. }
        | LpirOp::Fsqrt { dst, .. }
        | LpirOp::Fmin { dst, .. }
        | LpirOp::Fmax { dst, .. }
        | LpirOp::Ffloor { dst, .. }
        | LpirOp::Fceil { dst, .. }
        | LpirOp::Ftrunc { dst, .. }
        | LpirOp::Fnearest { dst, .. } => mark(*dst, defined),
        LpirOp::Iadd { dst, .. }
        | LpirOp::Isub { dst, .. }
        | LpirOp::Imul { dst, .. }
        | LpirOp::IdivS { dst, .. }
        | LpirOp::IdivU { dst, .. }
        | LpirOp::IremS { dst, .. }
        | LpirOp::IremU { dst, .. }
        | LpirOp::Ineg { dst, .. } => mark(*dst, defined),
        LpirOp::Feq { dst, .. }
        | LpirOp::Fne { dst, .. }
        | LpirOp::Flt { dst, .. }
        | LpirOp::Fle { dst, .. }
        | LpirOp::Fgt { dst, .. }
        | LpirOp::Fge { dst, .. }
        | LpirOp::Ieq { dst, .. }
        | LpirOp::Ine { dst, .. }
        | LpirOp::IltS { dst, .. }
        | LpirOp::IleS { dst, .. }
        | LpirOp::IgtS { dst, .. }
        | LpirOp::IgeS { dst, .. }
        | LpirOp::IltU { dst, .. }
        | LpirOp::IleU { dst, .. }
        | LpirOp::IgtU { dst, .. }
        | LpirOp::IgeU { dst, .. }
        | LpirOp::Iand { dst, .. }
        | LpirOp::Ior { dst, .. }
        | LpirOp::Ixor { dst, .. }
        | LpirOp::Ibnot { dst, .. }
        | LpirOp::Ishl { dst, .. }
        | LpirOp::IshrS { dst, .. }
        | LpirOp::IshrU { dst, .. } => mark(*dst, defined),
        LpirOp::FconstF32 { dst, .. } | LpirOp::IconstI32 { dst, .. } => mark(*dst, defined),
        LpirOp::IaddImm { dst, .. }
        | LpirOp::IsubImm { dst, .. }
        | LpirOp::ImulImm { dst, .. }
        | LpirOp::IshlImm { dst, .. }
        | LpirOp::IshrSImm { dst, .. }
        | LpirOp::IshrUImm { dst, .. }
        | LpirOp::IeqImm { dst, .. } => mark(*dst, defined),
        LpirOp::FtoiSatS { dst, .. }
        | LpirOp::FtoiSatU { dst, .. }
        | LpirOp::FtoUnorm16 { dst, .. }
        | LpirOp::FtoUnorm8 { dst, .. }
        | LpirOp::ItofS { dst, .. }
        | LpirOp::ItofU { dst, .. }
        | LpirOp::FfromI32Bits { dst, .. }
        | LpirOp::Unorm16toF { dst, .. }
        | LpirOp::Unorm8toF { dst, .. } => mark(*dst, defined),
        LpirOp::Select { dst, .. } | LpirOp::Copy { dst, .. } => mark(*dst, defined),
        LpirOp::SlotAddr { dst, .. }
        | LpirOp::Load { dst, .. }
        | LpirOp::Load8U { dst, .. }
        | LpirOp::Load8S { dst, .. }
        | LpirOp::Load16U { dst, .. }
        | LpirOp::Load16S { dst, .. } => mark(*dst, defined),
        LpirOp::Call { results, .. } => {
            for v in func.pool_slice(*results) {
                mark(*v, defined);
            }
        }
        LpirOp::Store { .. }
        | LpirOp::Store8 { .. }
        | LpirOp::Store16 { .. }
        | LpirOp::Memcpy { .. }
        | LpirOp::IfStart { .. }
        | LpirOp::Else
        | LpirOp::Continuing
        | LpirOp::LoopStart { .. }
        | LpirOp::SwitchStart { .. }
        | LpirOp::CaseStart { .. }
        | LpirOp::DefaultStart { .. }
        | LpirOp::End
        | LpirOp::Break
        | LpirOp::Continue
        | LpirOp::BrIfNot { .. }
        | LpirOp::Return { .. }
        | LpirOp::Block { .. }
        | LpirOp::ExitBlock => {}
    }
}

fn pool_bounds(func: &IrFunction, fname: &str, errs: &mut Vec<ValidationError>) {
    let n = func.vreg_pool.len();
    for (i, op) in func.body.iter().enumerate() {
        if let LpirOp::Call { args, results, .. } = op {
            if args.start as usize + args.count as usize > n {
                errs.push(err_in_func(
                    fname,
                    Some(i),
                    format!("call args VRegRange out of pool (len {n})"),
                ));
            }
            if results.start as usize + results.count as usize > n {
                errs.push(err_in_func(
                    fname,
                    Some(i),
                    format!("call results VRegRange out of pool (len {n})"),
                ));
            }
        }
        if let LpirOp::Return { values } = op {
            if values.start as usize + values.count as usize > n {
                errs.push(err_in_func(
                    fname,
                    Some(i),
                    format!("return VRegRange out of pool (len {n})"),
                ));
            }
            if values.count as usize != func.return_types.len() {
                errs.push(err_in_func(
                    fname,
                    Some(i),
                    format!(
                        "return value count {} != function return arity {}",
                        values.count,
                        func.return_types.len()
                    ),
                ));
            }
        }
    }
}
