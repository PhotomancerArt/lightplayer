//! Interpret LPIR modules (native `f32` / wrapping `i32`).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::module::{IrFunction, IrModule};
use crate::op::Op;
use crate::types::IrType;

/// Concrete runtime value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    F32(f32),
    I32(i32),
}

impl Value {
    pub fn as_f32(self) -> Option<f32> {
        match self {
            Value::F32(v) => Some(v),
            Value::I32(_) => None,
        }
    }

    pub fn as_i32(self) -> Option<i32> {
        match self {
            Value::I32(v) => Some(v),
            Value::F32(_) => None,
        }
    }
}

/// Import hook for `@module::name` calls.
pub trait ImportHandler {
    fn call(
        &mut self,
        module_name: &str,
        func_name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, InterpError>;
}

/// Interpreter error.
#[derive(Debug)]
pub enum InterpError {
    FunctionNotFound(String),
    Import(String),
    StackOverflow,
    Internal(String),
}

impl fmt::Display for InterpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InterpError::FunctionNotFound(name) => write!(f, "function not found: @{name}"),
            InterpError::Import(msg) => write!(f, "{msg}"),
            InterpError::StackOverflow => write!(f, "call stack overflow"),
            InterpError::Internal(msg) => write!(f, "internal interpreter error: {msg}"),
        }
    }
}

impl core::error::Error for InterpError {}

const DEFAULT_MAX_DEPTH: usize = 256;

/// Run `func_name` on `module` with `args`.
pub fn interpret(
    module: &IrModule,
    func_name: &str,
    args: &[Value],
    imports: &mut dyn ImportHandler,
) -> Result<Vec<Value>, InterpError> {
    interpret_with_depth(module, func_name, args, imports, DEFAULT_MAX_DEPTH)
}

pub fn interpret_with_depth(
    module: &IrModule,
    func_name: &str,
    args: &[Value],
    imports: &mut dyn ImportHandler,
    max_depth: usize,
) -> Result<Vec<Value>, InterpError> {
    let func = module
        .functions
        .iter()
        .find(|f| f.name == func_name)
        .ok_or_else(|| InterpError::FunctionNotFound(func_name.to_string()))?;
    exec_func(module, func, args, imports, 0, max_depth)
}

fn exec_func(
    module: &IrModule,
    func: &IrFunction,
    args: &[Value],
    imports: &mut dyn ImportHandler,
    depth: usize,
    max_depth: usize,
) -> Result<Vec<Value>, InterpError> {
    if depth > max_depth {
        return Err(InterpError::StackOverflow);
    }
    if args.len() != func.param_count as usize {
        return Err(InterpError::Internal(format!(
            "expected {} args, got {}",
            func.param_count,
            args.len()
        )));
    }

    let mut regs: Vec<Option<Value>> = alloc::vec![None; func.vreg_types.len()];
    for i in 0..func.param_count as usize {
        regs[i] = Some(args[i]);
    }

    let slot_off = slot_offsets(func);
    let slot_total: usize = func.slots.iter().map(|s| s.size as usize).sum();
    let mut slot_mem = alloc::vec![0u8; slot_total];

    let mut pc = 0usize;
    let mut ctrl: Vec<Ctrl> = Vec::new();

    while pc < func.body.len() {
        if let Some(Ctrl::SwitchArm { end, merge }) = ctrl.last() {
            let (e, m) = (*end, *merge);
            if pc == e {
                ctrl.pop();
                pc = m;
                continue;
            }
        }
        match &func.body[pc] {
            Op::IfStart {
                cond,
                else_offset,
                end_offset,
            } => {
                let c = get_reg(&regs, *cond)?;
                if cond_truthy(c)? {
                    ctrl.push(Ctrl::If {
                        merge: *end_offset as usize,
                    });
                    pc += 1;
                } else {
                    pc = *else_offset as usize;
                }
            }
            Op::Else => {
                // False-branch entry jumps here from `IfStart` without pushing `Ctrl::If`.
                // True-branch fall-through pushes `Ctrl::If` first; `Else` then skips the false arm.
                if matches!(ctrl.last(), Some(Ctrl::If { .. })) {
                    let merge = match ctrl.pop() {
                        Some(Ctrl::If { merge }) => merge,
                        _ => return Err(InterpError::Internal("else without if".into())),
                    };
                    pc = merge;
                } else {
                    pc += 1;
                }
            }
            Op::LoopStart {
                continuing_offset,
                end_offset,
            } => {
                ctrl.push(Ctrl::Loop {
                    head: pc,
                    continuing: *continuing_offset as usize,
                    exit: *end_offset as usize,
                });
                pc += 1;
            }
            Op::End => match ctrl.last() {
                Some(Ctrl::Loop { exit, head, .. }) if *exit == pc + 1 => {
                    pc = *head + 1;
                }
                Some(Ctrl::If { .. }) => {
                    ctrl.pop();
                    pc += 1;
                }
                _ => {
                    pc += 1;
                }
            },
            Op::Break => {
                let mut found = false;
                while let Some(c) = ctrl.pop() {
                    if let Ctrl::Loop { exit, .. } = c {
                        pc = exit;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(InterpError::Internal("break outside loop".into()));
                }
            }
            Op::Continue => {
                let mut target = None;
                while let Some(c) = ctrl.last() {
                    if let Ctrl::Loop { continuing, .. } = c {
                        target = Some(*continuing);
                        break;
                    }
                    ctrl.pop();
                }
                pc = target.ok_or_else(|| InterpError::Internal("continue".into()))?;
            }
            Op::BrIfNot { cond } => {
                let c = get_reg(&regs, *cond)?;
                if !cond_truthy(c)? {
                    let mut found = false;
                    while let Some(cf) = ctrl.pop() {
                        if let Ctrl::Loop { exit, .. } = cf {
                            pc = exit;
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(InterpError::Internal("br_if_not".into()));
                    }
                } else {
                    pc += 1;
                }
            }
            Op::SwitchStart {
                selector,
                end_offset,
            } => {
                let sel = val_i32(get_reg(&regs, *selector)?)?;
                let merge = *end_offset as usize;
                let (arm_pc, arm_end) = switch_pick_arm(func, pc + 1, sel, merge)
                    .ok_or_else(|| InterpError::Internal("switch dispatch".into()))?;
                if let Some(end) = arm_end {
                    ctrl.push(Ctrl::SwitchArm { end, merge });
                }
                pc = arm_pc;
            }
            Op::CaseStart { .. } | Op::DefaultStart { .. } => {
                pc += 1;
            }
            Op::Return { values } => {
                let slice = func.pool_slice(*values);
                let mut out = Vec::with_capacity(slice.len());
                for v in slice {
                    out.push(get_reg(&regs, *v)?);
                }
                return Ok(out);
            }
            Op::Call {
                callee,
                args: ar,
                results: rr,
            } => {
                let arg_vs = func.pool_slice(*ar);
                let mut call_args = Vec::with_capacity(arg_vs.len());
                for v in arg_vs {
                    call_args.push(get_reg(&regs, *v)?);
                }
                let res = if let Some(ii) = module.callee_as_import(*callee) {
                    let imp = &module.imports[ii];
                    imports.call(imp.module_name.as_str(), imp.func_name.as_str(), &call_args)?
                } else if let Some(fi) = module.callee_as_function(*callee) {
                    let callee_fn = &module.functions[fi];
                    exec_func(module, callee_fn, &call_args, imports, depth + 1, max_depth)?
                } else {
                    return Err(InterpError::Internal("bad callee".into()));
                };
                let res_vs = func.pool_slice(*rr);
                if res.len() != res_vs.len() {
                    return Err(InterpError::Internal("call result arity".into()));
                }
                for (dst, val) in res_vs.iter().zip(res.iter()) {
                    set_reg(&mut regs, *dst, *val)?;
                }
                pc += 1;
            }
            op => {
                eval_op(func, op, &mut regs, &slot_off, &mut slot_mem)?;
                pc += 1;
            }
        }
    }

    Ok(Vec::new())
}

enum Ctrl {
    If {
        merge: usize,
    },
    Loop {
        head: usize,
        continuing: usize,
        exit: usize,
    },
    SwitchArm {
        end: usize,
        merge: usize,
    },
}

fn get_reg(regs: &[Option<Value>], v: crate::types::VReg) -> Result<Value, InterpError> {
    regs[v.0 as usize].ok_or_else(|| InterpError::Internal(format!("undefined {v}")))
}

fn set_reg(
    regs: &mut [Option<Value>],
    v: crate::types::VReg,
    val: Value,
) -> Result<(), InterpError> {
    regs[v.0 as usize] = Some(val);
    Ok(())
}

fn slot_offsets(func: &IrFunction) -> Vec<u32> {
    let mut off = Vec::with_capacity(func.slots.len());
    let mut cur = 0u32;
    for s in &func.slots {
        off.push(cur);
        cur += s.size;
    }
    off
}

/// Returns `(first_op_of_arm, Some(case_end_pc))` or `(merge, None)` if no arm runs.
fn switch_pick_arm(
    func: &IrFunction,
    mut pc: usize,
    sel: i32,
    merge: usize,
) -> Option<(usize, Option<usize>)> {
    let mut default_arm: Option<(usize, u32)> = None;
    while pc < func.body.len() {
        match &func.body[pc] {
            Op::CaseStart { value, end_offset } => {
                if *value == sel {
                    return Some((pc + 1, Some(*end_offset as usize)));
                }
                pc = *end_offset as usize;
            }
            Op::DefaultStart { end_offset } => {
                default_arm = Some((pc + 1, *end_offset));
                pc = *end_offset as usize;
            }
            Op::End => {
                if let Some((start, end)) = default_arm {
                    return Some((start, Some(end as usize)));
                }
                return Some((merge, None));
            }
            _ => pc += 1,
        }
    }
    None
}

fn eval_op(
    func: &IrFunction,
    op: &Op,
    regs: &mut [Option<Value>],
    slot_off: &[u32],
    slot_mem: &mut [u8],
) -> Result<(), InterpError> {
    macro_rules! bin_f {
        ($dst:expr, $lhs:expr, $rhs:expr, $op:tt) => {{
            let a = val_f32(get_reg(regs, $lhs)?)?;
            let b = val_f32(get_reg(regs, $rhs)?)?;
            set_reg(regs, $dst, Value::F32(a $op b))?;
        }};
    }
    // Patterns bind references; macros receive VReg by value via * in match arms below.
    macro_rules! bin_i {
        ($dst:expr, $lhs:expr, $rhs:expr, add) => {{
            let a = val_i32(get_reg(regs, $lhs)?)?;
            let b = val_i32(get_reg(regs, $rhs)?)?;
            set_reg(regs, $dst, Value::I32(a.wrapping_add(b)))?;
        }};
        ($dst:expr, $lhs:expr, $rhs:expr, sub) => {{
            let a = val_i32(get_reg(regs, $lhs)?)?;
            let b = val_i32(get_reg(regs, $rhs)?)?;
            set_reg(regs, $dst, Value::I32(a.wrapping_sub(b)))?;
        }};
        ($dst:expr, $lhs:expr, $rhs:expr, mul) => {{
            let a = val_i32(get_reg(regs, $lhs)?)?;
            let b = val_i32(get_reg(regs, $rhs)?)?;
            set_reg(regs, $dst, Value::I32(a.wrapping_mul(b)))?;
        }};
    }

    match op {
        Op::Fadd { dst, lhs, rhs } => bin_f!(*dst, *lhs, *rhs, +),
        Op::Fsub { dst, lhs, rhs } => bin_f!(*dst, *lhs, *rhs, -),
        Op::Fmul { dst, lhs, rhs } => bin_f!(*dst, *lhs, *rhs, *),
        Op::Fdiv { dst, lhs, rhs } => bin_f!(*dst, *lhs, *rhs, /),
        Op::Fneg { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(-a))?;
        }
        Op::Fabs { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(a.abs()))?;
        }
        Op::Fsqrt { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(libm::sqrtf(a)))?;
        }
        Op::Fmin { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::F32(a.min(b)))?;
        }
        Op::Fmax { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::F32(a.max(b)))?;
        }
        Op::Ffloor { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(libm::floorf(a)))?;
        }
        Op::Fceil { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(libm::ceilf(a)))?;
        }
        Op::Ftrunc { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(libm::truncf(a)))?;
        }
        Op::Fnearest { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(round_even(a)))?;
        }
        Op::Iadd { dst, lhs, rhs } => bin_i!(*dst, *lhs, *rhs, add),
        Op::Isub { dst, lhs, rhs } => bin_i!(*dst, *lhs, *rhs, sub),
        Op::Imul { dst, lhs, rhs } => bin_i!(*dst, *lhs, *rhs, mul),
        Op::IdivS { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)?;
            let b = val_i32(get_reg(regs, *rhs)?)?;
            let v = if b == 0 { 0 } else { a.wrapping_div(b) };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::IdivU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            let v = if b == 0 { 0 } else { a.wrapping_div(b) };
            set_reg(regs, *dst, Value::I32(v as i32))?;
        }
        Op::IremS { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)?;
            let b = val_i32(get_reg(regs, *rhs)?)?;
            let v = if b == 0 { 0 } else { a.wrapping_rem(b) };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::IremU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            let v = if b == 0 { 0 } else { a.wrapping_rem(b) };
            set_reg(regs, *dst, Value::I32(v as i32))?;
        }
        Op::Ineg { dst, src } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(a.wrapping_neg()))?;
        }
        Op::Feq { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if a.is_nan() || b.is_nan() {
                0
            } else if a == b {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Fne { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if a != b || a.is_nan() || b.is_nan() {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Flt { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if !a.is_nan() && !b.is_nan() && a < b {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Fle { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if !a.is_nan() && !b.is_nan() && a <= b {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Fgt { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if !a.is_nan() && !b.is_nan() && a > b {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Fge { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if !a.is_nan() && !b.is_nan() && a >= b {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Ieq { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? == val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Ine { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? != val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::IltS { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? < val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::IleS { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? <= val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::IgtS { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? > val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::IgeS { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? >= val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::IltU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            set_reg(regs, *dst, Value::I32((a < b) as i32))?;
        }
        Op::IleU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            set_reg(regs, *dst, Value::I32((a <= b) as i32))?;
        }
        Op::IgtU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            set_reg(regs, *dst, Value::I32((a > b) as i32))?;
        }
        Op::IgeU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            set_reg(regs, *dst, Value::I32((a >= b) as i32))?;
        }
        Op::Iand { dst, lhs, rhs } => {
            let v = val_i32(get_reg(regs, *lhs)?)? & val_i32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Ior { dst, lhs, rhs } => {
            let v = val_i32(get_reg(regs, *lhs)?)? | val_i32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Ixor { dst, lhs, rhs } => {
            let v = val_i32(get_reg(regs, *lhs)?)? ^ val_i32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Ibnot { dst, src } => {
            let v = !val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::Ishl { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)?;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32 & 31;
            set_reg(regs, *dst, Value::I32(a.wrapping_shl(b)))?;
        }
        Op::IshrS { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)?;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32 & 31;
            set_reg(regs, *dst, Value::I32(a.wrapping_shr(b)))?;
        }
        Op::IshrU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32 & 31;
            set_reg(regs, *dst, Value::I32((a >> b) as i32))?;
        }
        Op::FconstF32 { dst, value } => set_reg(regs, *dst, Value::F32(*value))?,
        Op::IconstI32 { dst, value } => set_reg(regs, *dst, Value::I32(*value))?,
        Op::IaddImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(a.wrapping_add(*imm)))?;
        }
        Op::IsubImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(a.wrapping_sub(*imm)))?;
        }
        Op::ImulImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(a.wrapping_mul(*imm)))?;
        }
        Op::IshlImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            let b = (*imm as u32) & 31;
            set_reg(regs, *dst, Value::I32(a.wrapping_shl(b)))?;
        }
        Op::IshrSImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            let b = (*imm as u32) & 31;
            set_reg(regs, *dst, Value::I32(a.wrapping_shr(b)))?;
        }
        Op::IshrUImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)? as u32;
            let b = (*imm as u32) & 31;
            set_reg(regs, *dst, Value::I32((a >> b) as i32))?;
        }
        Op::IeqImm { dst, src, imm } => {
            let v = (val_i32(get_reg(regs, *src)?)? == *imm) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        Op::FtoiSatS { dst, src } => {
            let f = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(ftoi_sat_s(f)))?;
        }
        Op::FtoiSatU { dst, src } => {
            let f = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(ftoi_sat_u(f)))?;
        }
        Op::ItofS { dst, src } => {
            let v = val_i32(get_reg(regs, *src)?)? as f32;
            set_reg(regs, *dst, Value::F32(v))?;
        }
        Op::ItofU { dst, src } => {
            let v = val_i32(get_reg(regs, *src)?)? as u32 as f32;
            set_reg(regs, *dst, Value::F32(v))?;
        }
        Op::FfromI32Bits { dst, src } => {
            let v = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(f32::from_bits(v as u32)))?;
        }
        Op::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => {
            let v = if cond_truthy(get_reg(regs, *cond)?)? {
                get_reg(regs, *if_true)?
            } else {
                get_reg(regs, *if_false)?
            };
            set_reg(regs, *dst, v)?;
        }
        Op::Copy { dst, src } => {
            let v = get_reg(regs, *src)?;
            set_reg(regs, *dst, v)?;
        }
        Op::SlotAddr { dst, slot } => {
            let off = slot_off
                .get(slot.0 as usize)
                .copied()
                .ok_or_else(|| InterpError::Internal("slot".into()))?;
            set_reg(regs, *dst, Value::I32(off as i32))?;
        }
        Op::Load { dst, base, offset } => {
            let addr = val_i32(get_reg(regs, *base)?)? as usize;
            let addr = addr + *offset as usize;
            let ty = func.vreg_types[dst.0 as usize];
            match ty {
                IrType::F32 => {
                    let b = read_u32(slot_mem, addr)?;
                    set_reg(regs, *dst, Value::F32(f32::from_bits(b)))?;
                }
                IrType::I32 => {
                    let b = read_u32(slot_mem, addr)?;
                    set_reg(regs, *dst, Value::I32(b as i32))?;
                }
            }
        }
        Op::Store {
            base,
            offset,
            value,
        } => {
            let addr = val_i32(get_reg(regs, *base)?)? as usize + *offset as usize;
            match get_reg(regs, *value)? {
                Value::F32(f) => write_u32(slot_mem, addr, f.to_bits())?,
                Value::I32(i) => write_u32(slot_mem, addr, i as u32)?,
            }
        }
        Op::Memcpy {
            dst_addr,
            src_addr,
            size,
        } => {
            let d = val_i32(get_reg(regs, *dst_addr)?)? as usize;
            let s = val_i32(get_reg(regs, *src_addr)?)? as usize;
            let n = *size as usize;
            if d + n > slot_mem.len() || s + n > slot_mem.len() {
                return Err(InterpError::Internal("memcpy oob".into()));
            }
            slot_mem.copy_within(s..s + n, d);
        }
        _ => return Err(InterpError::Internal("unsupported op".into())),
    }
    Ok(())
}

fn val_f32(v: Value) -> Result<f32, InterpError> {
    v.as_f32()
        .ok_or_else(|| InterpError::Internal(String::from("expected f32 value in vreg")))
}

fn val_i32(v: Value) -> Result<i32, InterpError> {
    v.as_i32()
        .ok_or_else(|| InterpError::Internal(String::from("expected i32 value in vreg")))
}

fn cond_truthy(v: Value) -> Result<bool, InterpError> {
    Ok(val_i32(v)? != 0)
}

fn read_u32(mem: &[u8], addr: usize) -> Result<u32, InterpError> {
    if addr + 4 > mem.len() {
        return Err(InterpError::Internal("load oob".into()));
    }
    Ok(u32::from_le_bytes([
        mem[addr],
        mem[addr + 1],
        mem[addr + 2],
        mem[addr + 3],
    ]))
}

fn write_u32(mem: &mut [u8], addr: usize, v: u32) -> Result<(), InterpError> {
    if addr + 4 > mem.len() {
        return Err(InterpError::Internal("store oob".into()));
    }
    let b = v.to_le_bytes();
    mem[addr..addr + 4].copy_from_slice(&b);
    Ok(())
}

fn ftoi_sat_s(f: f32) -> i32 {
    if f.is_nan() {
        return 0;
    }
    if f < i32::MIN as f32 {
        return i32::MIN;
    }
    if f > i32::MAX as f32 {
        return i32::MAX;
    }
    f as i32
}

fn ftoi_sat_u(f: f32) -> i32 {
    if f.is_nan() || f <= 0.0 {
        return 0;
    }
    if f >= u32::MAX as f32 {
        return -1i32;
    }
    f as u32 as i32
}

/// Round to nearest integer, ties to even (matches typical WASM `f32.nearest`).
fn round_even(v: f32) -> f32 {
    let r = libm::roundf(v);
    if (v - r).abs() == 0.5 {
        let f = r as i64;
        if f % 2 != 0 { r - v.signum() } else { r }
    } else {
        r
    }
}
