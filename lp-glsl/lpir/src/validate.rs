//! Well-formedness checks for [`IrModule`] and [`IrFunction`].

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::module::{IrFunction, IrModule};
use crate::op::Op;
use crate::types::{CalleeRef, IrType, VReg, VRegRange};

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
pub fn validate_module(module: &IrModule) -> Result<(), Vec<ValidationError>> {
    let mut errs = Vec::new();
    validate_imports(module, &mut errs);
    let mut entry = 0u32;
    let mut seen_names: Vec<&str> = Vec::new();
    for f in &module.functions {
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
pub fn validate_function(func: &IrFunction, module: &IrModule) -> Result<(), Vec<ValidationError>> {
    let mut errs = Vec::new();
    validate_function_inner(func, module, &mut errs);
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

fn validate_imports(module: &IrModule, errs: &mut Vec<ValidationError>) {
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
    Switch {
        cases: BTreeSet<i32>,
        default_arm: bool,
    },
    Arm,
}

fn validate_function_inner(func: &IrFunction, module: &IrModule, errs: &mut Vec<ValidationError>) {
    let fname = func.name.as_str();
    pool_bounds(func, fname, errs);

    let n = func.vreg_types.len();
    let mut defined = vec![false; n];
    for i in 0..(func.param_count as usize).min(n) {
        defined[i] = true;
    }

    let mut stack: Vec<StackEntry> = Vec::new();

    for (i, op) in func.body.iter().enumerate() {
        let op_i = Some(i);

        match op {
            Op::Break | Op::Continue | Op::BrIfNot { .. } => {
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
                } else if matches!(op, Op::Continue) {
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
            Op::IfStart { .. } => stack.push(StackEntry::If),
            Op::Else => match stack.pop() {
                Some(StackEntry::If) => stack.push(StackEntry::Else),
                _ => errs.push(err_in_func(fname, op_i, "`else` without matching `if`")),
            },
            Op::LoopStart {
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
            Op::SwitchStart { .. } => stack.push(StackEntry::Switch {
                cases: BTreeSet::new(),
                default_arm: false,
            }),
            Op::CaseStart { value, .. } => {
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
            Op::DefaultStart { .. } => {
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
            Op::End => {
                if stack.pop().is_none() {
                    errs.push(err_in_func(
                        fname,
                        op_i,
                        "`End` underflow (extra closing brace)",
                    ));
                }
            }
            Op::Call {
                callee,
                args,
                results,
            } => {
                validate_call(func, module, fname, op_i, *callee, *args, *results, errs);
            }
            Op::Return { values } => {
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
    module: &IrModule,
    fname: &str,
    op_i: Option<usize>,
    callee: CalleeRef,
    args: VRegRange,
    results: VRegRange,
    errs: &mut Vec<ValidationError>,
) {
    let total = module.imports.len() + module.functions.len();
    if callee.0 as usize >= total {
        errs.push(err_in_func(
            fname,
            op_i,
            format!("callee index {} out of range", callee.0),
        ));
        return;
    }

    let Some((param_tys, ret_tys)) = callee_signature(module, callee) else {
        errs.push(err_in_func(
            fname,
            op_i,
            "internal: callee signature missing",
        ));
        return;
    };

    let arg_slice = func.pool_slice(args);
    let res_slice = func.pool_slice(results);

    if arg_slice.len() != args.count as usize || res_slice.len() != results.count as usize {
        return;
    }

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

fn callee_signature<'a>(
    module: &'a IrModule,
    callee: CalleeRef,
) -> Option<(&'a [IrType], &'a [IrType])> {
    if let Some(i) = module.callee_as_import(callee) {
        let imp = &module.imports[i];
        Some((&imp.param_types, &imp.return_types))
    } else if let Some(i) = module.callee_as_function(callee) {
        let f = &module.functions[i];
        let n = f.param_count as usize;
        if n <= f.vreg_types.len() {
            Some((&f.vreg_types[..n], &f.return_types))
        } else {
            None
        }
    } else {
        None
    }
}

fn check_op_operands_defined(
    func: &IrFunction,
    fname: &str,
    op_i: Option<usize>,
    op: &Op,
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
        Op::Fadd { lhs, rhs, .. }
        | Op::Fsub { lhs, rhs, .. }
        | Op::Fmul { lhs, rhs, .. }
        | Op::Fdiv { lhs, rhs, .. } => {
            check(*lhs, "lhs");
            check(*rhs, "rhs");
        }
        Op::Fneg { src, .. } | Op::Ineg { src, .. } | Op::Ibnot { src, .. } => check(*src, "src"),
        Op::Iadd { lhs, rhs, .. }
        | Op::Isub { lhs, rhs, .. }
        | Op::Imul { lhs, rhs, .. }
        | Op::IdivS { lhs, rhs, .. }
        | Op::IdivU { lhs, rhs, .. }
        | Op::IremS { lhs, rhs, .. }
        | Op::IremU { lhs, rhs, .. } => {
            check(*lhs, "lhs");
            check(*rhs, "rhs");
        }
        Op::Feq { lhs, rhs, .. }
        | Op::Fne { lhs, rhs, .. }
        | Op::Flt { lhs, rhs, .. }
        | Op::Fle { lhs, rhs, .. }
        | Op::Fgt { lhs, rhs, .. }
        | Op::Fge { lhs, rhs, .. }
        | Op::Ieq { lhs, rhs, .. }
        | Op::Ine { lhs, rhs, .. }
        | Op::IltS { lhs, rhs, .. }
        | Op::IleS { lhs, rhs, .. }
        | Op::IgtS { lhs, rhs, .. }
        | Op::IgeS { lhs, rhs, .. }
        | Op::IltU { lhs, rhs, .. }
        | Op::IleU { lhs, rhs, .. }
        | Op::IgtU { lhs, rhs, .. }
        | Op::IgeU { lhs, rhs, .. }
        | Op::Iand { lhs, rhs, .. }
        | Op::Ior { lhs, rhs, .. }
        | Op::Ixor { lhs, rhs, .. }
        | Op::Ishl { lhs, rhs, .. }
        | Op::IshrS { lhs, rhs, .. }
        | Op::IshrU { lhs, rhs, .. } => {
            check(*lhs, "lhs");
            check(*rhs, "rhs");
        }
        Op::IaddImm { src, .. }
        | Op::IsubImm { src, .. }
        | Op::ImulImm { src, .. }
        | Op::IshlImm { src, .. }
        | Op::IshrSImm { src, .. }
        | Op::IshrUImm { src, .. }
        | Op::IeqImm { src, .. } => check(*src, "src"),
        Op::FtoiSatS { src, .. }
        | Op::FtoiSatU { src, .. }
        | Op::ItofS { src, .. }
        | Op::ItofU { src, .. } => check(*src, "src"),
        Op::Select {
            cond,
            if_true,
            if_false,
            ..
        } => {
            check(*cond, "select cond");
            check(*if_true, "select if_true");
            check(*if_false, "select if_false");
        }
        Op::Copy { src, .. } => check(*src, "copy src"),
        Op::Load { base, .. } => check(*base, "load base"),
        Op::Store { base, value, .. } => {
            check(*base, "store base");
            check(*value, "store value");
        }
        Op::Memcpy {
            dst_addr, src_addr, ..
        } => {
            check(*dst_addr, "memcpy dst");
            check(*src_addr, "memcpy src");
        }
        Op::IfStart { cond, .. } => check(*cond, "if cond"),
        Op::SwitchStart { selector, .. } => check(*selector, "switch selector"),
        Op::BrIfNot { cond } => check(*cond, "br_if_not cond"),
        Op::Call { args, .. } => {
            for v in func.pool_slice(*args) {
                check(*v, "call arg");
            }
        }
        Op::Return { values } => {
            for v in func.pool_slice(*values) {
                check(*v, "return");
            }
        }
        Op::SlotAddr { .. }
        | Op::FconstF32 { .. }
        | Op::IconstI32 { .. }
        | Op::Else
        | Op::LoopStart { .. }
        | Op::CaseStart { .. }
        | Op::DefaultStart { .. }
        | Op::End
        | Op::Break
        | Op::Continue => {}
    }
}

fn check_slot_memory_ops(
    func: &IrFunction,
    fname: &str,
    op_i: Option<usize>,
    op: &Op,
    errs: &mut Vec<ValidationError>,
) {
    if let Op::SlotAddr { slot, .. } = op {
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
    op: &Op,
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
        Op::Fadd { dst, .. }
        | Op::Fsub { dst, .. }
        | Op::Fmul { dst, .. }
        | Op::Fdiv { dst, .. }
        | Op::Fneg { dst, .. }
        | Op::FconstF32 { dst, .. }
        | Op::ItofS { dst, .. }
        | Op::ItofU { dst, .. } => expect(*dst, IrType::F32, "float op result"),

        Op::Iadd { dst, .. }
        | Op::Isub { dst, .. }
        | Op::Imul { dst, .. }
        | Op::IdivS { dst, .. }
        | Op::IdivU { dst, .. }
        | Op::IremS { dst, .. }
        | Op::IremU { dst, .. }
        | Op::Ineg { dst, .. }
        | Op::Feq { dst, .. }
        | Op::Fne { dst, .. }
        | Op::Flt { dst, .. }
        | Op::Fle { dst, .. }
        | Op::Fgt { dst, .. }
        | Op::Fge { dst, .. }
        | Op::Ieq { dst, .. }
        | Op::Ine { dst, .. }
        | Op::IltS { dst, .. }
        | Op::IleS { dst, .. }
        | Op::IgtS { dst, .. }
        | Op::IgeS { dst, .. }
        | Op::IltU { dst, .. }
        | Op::IleU { dst, .. }
        | Op::IgtU { dst, .. }
        | Op::IgeU { dst, .. }
        | Op::Iand { dst, .. }
        | Op::Ior { dst, .. }
        | Op::Ixor { dst, .. }
        | Op::Ibnot { dst, .. }
        | Op::Ishl { dst, .. }
        | Op::IshrS { dst, .. }
        | Op::IshrU { dst, .. }
        | Op::IconstI32 { dst, .. }
        | Op::IaddImm { dst, .. }
        | Op::IsubImm { dst, .. }
        | Op::ImulImm { dst, .. }
        | Op::IshlImm { dst, .. }
        | Op::IshrSImm { dst, .. }
        | Op::IshrUImm { dst, .. }
        | Op::IeqImm { dst, .. }
        | Op::FtoiSatS { dst, .. }
        | Op::FtoiSatU { dst, .. } => expect(*dst, IrType::I32, "integer op result"),

        Op::Select {
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
        Op::Copy { dst, src } => {
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
        Op::SlotAddr { dst, .. } => expect(*dst, IrType::I32, "slot_addr"),
        Op::Load { .. }
        | Op::Store { .. }
        | Op::Memcpy { .. }
        | Op::IfStart { .. }
        | Op::Else
        | Op::LoopStart { .. }
        | Op::SwitchStart { .. }
        | Op::CaseStart { .. }
        | Op::DefaultStart { .. }
        | Op::End
        | Op::Break
        | Op::Continue
        | Op::BrIfNot { .. }
        | Op::Call { .. }
        | Op::Return { .. } => {}
    }
}

fn mark_op_defs(func: &IrFunction, op: &Op, defined: &mut [bool]) {
    let mark = |v: VReg, d: &mut [bool]| {
        let j = v.0 as usize;
        if j < d.len() {
            d[j] = true;
        }
    };

    match op {
        Op::Fadd { dst, .. }
        | Op::Fsub { dst, .. }
        | Op::Fmul { dst, .. }
        | Op::Fdiv { dst, .. }
        | Op::Fneg { dst, .. } => mark(*dst, defined),
        Op::Iadd { dst, .. }
        | Op::Isub { dst, .. }
        | Op::Imul { dst, .. }
        | Op::IdivS { dst, .. }
        | Op::IdivU { dst, .. }
        | Op::IremS { dst, .. }
        | Op::IremU { dst, .. }
        | Op::Ineg { dst, .. } => mark(*dst, defined),
        Op::Feq { dst, .. }
        | Op::Fne { dst, .. }
        | Op::Flt { dst, .. }
        | Op::Fle { dst, .. }
        | Op::Fgt { dst, .. }
        | Op::Fge { dst, .. }
        | Op::Ieq { dst, .. }
        | Op::Ine { dst, .. }
        | Op::IltS { dst, .. }
        | Op::IleS { dst, .. }
        | Op::IgtS { dst, .. }
        | Op::IgeS { dst, .. }
        | Op::IltU { dst, .. }
        | Op::IleU { dst, .. }
        | Op::IgtU { dst, .. }
        | Op::IgeU { dst, .. }
        | Op::Iand { dst, .. }
        | Op::Ior { dst, .. }
        | Op::Ixor { dst, .. }
        | Op::Ibnot { dst, .. }
        | Op::Ishl { dst, .. }
        | Op::IshrS { dst, .. }
        | Op::IshrU { dst, .. } => mark(*dst, defined),
        Op::FconstF32 { dst, .. } | Op::IconstI32 { dst, .. } => mark(*dst, defined),
        Op::IaddImm { dst, .. }
        | Op::IsubImm { dst, .. }
        | Op::ImulImm { dst, .. }
        | Op::IshlImm { dst, .. }
        | Op::IshrSImm { dst, .. }
        | Op::IshrUImm { dst, .. }
        | Op::IeqImm { dst, .. } => mark(*dst, defined),
        Op::FtoiSatS { dst, .. }
        | Op::FtoiSatU { dst, .. }
        | Op::ItofS { dst, .. }
        | Op::ItofU { dst, .. } => mark(*dst, defined),
        Op::Select { dst, .. } | Op::Copy { dst, .. } => mark(*dst, defined),
        Op::SlotAddr { dst, .. } | Op::Load { dst, .. } => mark(*dst, defined),
        Op::Call { results, .. } => {
            for v in func.pool_slice(*results) {
                mark(*v, defined);
            }
        }
        Op::Store { .. }
        | Op::Memcpy { .. }
        | Op::IfStart { .. }
        | Op::Else
        | Op::LoopStart { .. }
        | Op::SwitchStart { .. }
        | Op::CaseStart { .. }
        | Op::DefaultStart { .. }
        | Op::End
        | Op::Break
        | Op::Continue
        | Op::BrIfNot { .. }
        | Op::Return { .. } => {}
    }
}

fn pool_bounds(func: &IrFunction, fname: &str, errs: &mut Vec<ValidationError>) {
    let n = func.vreg_pool.len();
    for (i, op) in func.body.iter().enumerate() {
        if let Op::Call { args, results, .. } = op {
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
        if let Op::Return { values } = op {
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
