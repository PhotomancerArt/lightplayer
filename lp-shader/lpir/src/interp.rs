//! Interpret LPIR modules (native `f32` / wrapping `i32`).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::lpir_module::{IrFunction, LpirModule};
use crate::lpir_op::LpirOp;
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

pub const DEFAULT_MAX_DEPTH: usize = 256;

/// Run `func_name` on `module` with `args`.
pub fn interpret(
    module: &LpirModule,
    func_name: &str,
    args: &[Value],
    imports: &mut dyn ImportHandler,
) -> Result<Vec<Value>, InterpError> {
    interpret_with_depth(module, func_name, args, imports, DEFAULT_MAX_DEPTH)
}

pub fn interpret_with_depth(
    module: &LpirModule,
    func_name: &str,
    args: &[Value],
    imports: &mut dyn ImportHandler,
    max_depth: usize,
) -> Result<Vec<Value>, InterpError> {
    let func = module
        .functions
        .values()
        .find(|f| f.name == func_name)
        .ok_or_else(|| InterpError::FunctionNotFound(func_name.to_string()))?;
    let mut full = alloc::vec![Value::I32(0); 1];
    if func.sret_arg.is_some() {
        full.push(Value::I32(0));
    }
    full.extend_from_slice(args);
    // One linear stack shared by all frames so slot addresses stay valid
    // across calls (out-parameters, sret pointers).
    let mut stack: Vec<u8> = Vec::new();
    exec_func(module, func, &full, imports, 0, max_depth, &mut stack)
}

/// Result of an [`interpret_entry`] run.
pub struct EntryOutput {
    /// Scalar return values (empty for sret functions — read `sret_bytes`).
    pub values: Vec<Value>,
    /// Read-back of the sret destination buffer (`sret_size` bytes, std430
    /// layout); empty when the entry function does not use sret.
    pub sret_bytes: Vec<u8>,
}

/// Full-featured host entry point: run `func_name` against a real VMContext
/// image and (for aggregate-returning functions) a real sret destination.
///
/// The shared interpreter stack is laid out `[vmctx image][sret buffer]
/// [call frames…]`:
/// - `vmctx_image` is copied to the stack base and the VMContext pointer
///   (hidden arg 0) is `0`, so vmctx-relative uniform/global loads and
///   stores (offsets baked in by the frontend, region sized by
///   `LpsModuleSig::vmctx_buffer_size`) hit real memory. Pass a zeroed
///   image for default-initialized uniforms/globals, or pre-write uniform
///   values into it (`encode_uniform_write` offsets are vmctx-relative).
/// - For a callee with `sret_arg`, `sret_size` bytes are reserved directly
///   above the image and the hidden sret pointer points at them; after the
///   run they are returned in [`EntryOutput::sret_bytes`] for the caller to
///   decode (the callee's aggregate std430 layout).
///
/// Errors if `sret_size` disagrees with whether the function actually uses
/// sret — that is a caller-side signature confusion worth surfacing.
pub fn interpret_entry(
    module: &LpirModule,
    func_name: &str,
    args: &[Value],
    imports: &mut dyn ImportHandler,
    vmctx_image: &[u8],
    sret_size: usize,
    max_depth: usize,
) -> Result<EntryOutput, InterpError> {
    let func = module
        .functions
        .values()
        .find(|f| f.name == func_name)
        .ok_or_else(|| InterpError::FunctionNotFound(func_name.to_string()))?;
    if func.sret_arg.is_some() != (sret_size > 0) {
        return Err(InterpError::Internal(format!(
            "sret mismatch for @{func_name}: function {} sret but sret_size is {sret_size}",
            if func.sret_arg.is_some() {
                "uses"
            } else {
                "does not use"
            },
        )));
    }
    let sret_base = vmctx_image.len();
    let mut full = alloc::vec![Value::I32(0); 1];
    if func.sret_arg.is_some() {
        full.push(Value::I32(sret_base as i32));
    }
    full.extend_from_slice(args);
    let mut stack: Vec<u8> = Vec::with_capacity(sret_base + sret_size);
    stack.extend_from_slice(vmctx_image);
    stack.resize(sret_base + sret_size, 0);
    let values = exec_func(module, func, &full, imports, 0, max_depth, &mut stack)?;
    let sret_bytes = stack[sret_base..sret_base + sret_size].to_vec();
    Ok(EntryOutput { values, sret_bytes })
}

fn exec_func(
    module: &LpirModule,
    func: &IrFunction,
    args: &[Value],
    imports: &mut dyn ImportHandler,
    depth: usize,
    max_depth: usize,
    stack: &mut Vec<u8>,
) -> Result<Vec<Value>, InterpError> {
    if depth > max_depth {
        return Err(InterpError::StackOverflow);
    }
    let h = func.hidden_param_slots() as usize;
    let expected = h + func.param_count as usize;
    if args.len() != expected {
        return Err(InterpError::Internal(format!(
            "expected {} args (vmctx + {} hidden incl. sret + {} user), got {}",
            expected,
            h,
            func.param_count,
            args.len()
        )));
    }

    // VRegs read before their first definition yield the type's zero. This is
    // the semantics the wasm backend already ships (wasm locals are
    // zero-initialized), and it is load-bearing for GLSL out-parameters: the
    // caller-side copy-in of a never-assigned local (`float v; f(v);`) reads
    // the local's vreg before any store defines it. Structural use-before-def
    // strictness lives in the validator, not here.
    let mut regs: Vec<Option<Value>> = func
        .vreg_types
        .iter()
        .map(|ty| {
            Some(match ty {
                IrType::F32 => Value::F32(0.0),
                IrType::I32 | IrType::Pointer => Value::I32(0),
            })
        })
        .collect();
    let vm = func.vmctx_vreg.0 as usize;
    regs[vm] = Some(args[0]);
    for k in 1..h {
        regs[vm + k] = Some(args[k]);
    }
    for i in 0..func.param_count as usize {
        regs[vm + h + i] = Some(args[h + i]);
    }

    // This frame's slots live at [frame_base, frame_base + slot_total) in
    // the shared stack; slot addresses are absolute stack offsets so
    // callees can dereference caller pointers (out-parameters).
    let frame_base = stack.len();
    let slot_off = slot_offsets(func, frame_base as u32);
    let slot_total: usize = func.slots.iter().map(|s| s.size as usize).sum();
    stack.resize(frame_base + slot_total, 0);

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
            LpirOp::IfStart {
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
                } else if matches!(func.body.get(*else_offset as usize), Some(LpirOp::Else)) {
                    // Enter the else arm just past its `Else` marker, with a
                    // frame so the closing `End` pops it (keeps nesting
                    // balanced — `Else`/`End` never guess whose frame is on
                    // top).
                    ctrl.push(Ctrl::If {
                        merge: *end_offset as usize,
                    });
                    pc = *else_offset as usize + 1;
                } else {
                    // No else arm: skip the whole construct (else_offset
                    // points at the `End`, end_offset just past it).
                    pc = *end_offset as usize;
                }
            }
            LpirOp::Else => {
                // Only reachable by fall-through from the then-arm (the
                // false branch enters *past* this marker); skip the else arm.
                match ctrl.pop() {
                    Some(Ctrl::If { merge }) => pc = merge,
                    _ => return Err(InterpError::Internal("else without if".into())),
                }
            }
            LpirOp::LoopStart {
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
            LpirOp::Block { end_offset } => {
                ctrl.push(Ctrl::Block {
                    exit: *end_offset as usize,
                });
                pc += 1;
            }
            LpirOp::ExitBlock => {
                let mut found = false;
                while let Some(c) = ctrl.pop() {
                    if let Ctrl::Block { exit } = c {
                        pc = exit;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(InterpError::Internal("exit_block outside block".into()));
                }
            }
            LpirOp::End => match ctrl.last() {
                Some(Ctrl::Loop { exit, head, .. }) if *exit == pc + 1 => {
                    pc = *head + 1;
                }
                Some(Ctrl::If { .. }) => {
                    ctrl.pop();
                    pc += 1;
                }
                Some(Ctrl::Block { .. }) => {
                    ctrl.pop();
                    pc += 1;
                }
                _ => {
                    pc += 1;
                }
            },
            LpirOp::Break => {
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
            LpirOp::Continue => {
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
            LpirOp::BrIfNot { cond } => {
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
            LpirOp::SwitchStart {
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
            LpirOp::CaseStart { .. } | LpirOp::DefaultStart { .. } => {
                pc += 1;
            }
            LpirOp::Return { values } => {
                let slice = func.pool_slice(*values);
                let mut out = Vec::with_capacity(slice.len());
                for v in slice {
                    out.push(get_reg(&regs, *v)?);
                }
                stack.truncate(frame_base);
                return Ok(out);
            }
            LpirOp::Call {
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
                } else if let Some(callee_fn) = module.callee_as_function(*callee) {
                    let ch = callee_fn.hidden_param_slots() as usize;
                    let want = ch + callee_fn.param_count as usize;
                    if call_args.len() != want {
                        return Err(InterpError::Internal(format!(
                            "local call arg count {} != callee total param slots {}",
                            call_args.len(),
                            want
                        )));
                    }
                    exec_func(
                        module,
                        callee_fn,
                        &call_args,
                        imports,
                        depth + 1,
                        max_depth,
                        stack,
                    )?
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
                eval_op(func, op, &mut regs, &slot_off, stack)?;
                pc += 1;
            }
        }
    }

    stack.truncate(frame_base);
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
    /// `exit` is the merge PC (first instruction after the closing [`LpirOp::End`]).
    Block {
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

fn slot_offsets(func: &IrFunction, frame_base: u32) -> Vec<u32> {
    let mut off = Vec::with_capacity(func.slots.len());
    let mut cur = frame_base;
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
            LpirOp::CaseStart { value, end_offset } => {
                if *value == sel {
                    return Some((pc + 1, Some(*end_offset as usize)));
                }
                pc = *end_offset as usize;
            }
            LpirOp::DefaultStart { end_offset } => {
                default_arm = Some((pc + 1, *end_offset));
                pc = *end_offset as usize;
            }
            LpirOp::End => {
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
    op: &LpirOp,
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
        LpirOp::Fadd { dst, lhs, rhs } => bin_f!(*dst, *lhs, *rhs, +),
        LpirOp::Fsub { dst, lhs, rhs } => bin_f!(*dst, *lhs, *rhs, -),
        LpirOp::Fmul { dst, lhs, rhs } => bin_f!(*dst, *lhs, *rhs, *),
        LpirOp::Fdiv { dst, lhs, rhs } => bin_f!(*dst, *lhs, *rhs, /),
        LpirOp::FdivConstF32 { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            set_reg(regs, *dst, Value::F32(a / *rhs))?;
        }
        LpirOp::Fneg { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(-a))?;
        }
        LpirOp::Fabs { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(a.abs()))?;
        }
        LpirOp::Fsqrt { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(libm::sqrtf(a)))?;
        }
        LpirOp::Fmin { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::F32(a.min(b)))?;
        }
        LpirOp::Fmax { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::F32(a.max(b)))?;
        }
        LpirOp::Ffloor { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(libm::floorf(a)))?;
        }
        LpirOp::Fceil { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(libm::ceilf(a)))?;
        }
        LpirOp::Ftrunc { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(libm::truncf(a)))?;
        }
        LpirOp::Fnearest { dst, src } => {
            let a = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(round_even(a)))?;
        }
        LpirOp::Iadd { dst, lhs, rhs } => bin_i!(*dst, *lhs, *rhs, add),
        LpirOp::Isub { dst, lhs, rhs } => bin_i!(*dst, *lhs, *rhs, sub),
        LpirOp::Imul { dst, lhs, rhs } => bin_i!(*dst, *lhs, *rhs, mul),
        LpirOp::IdivS { dst, lhs, rhs } => {
            // RV32 semantics: x / 0 = -1, i32::MIN / -1 = i32::MIN.
            let a = val_i32(get_reg(regs, *lhs)?)?;
            let b = val_i32(get_reg(regs, *rhs)?)?;
            let v = if b == 0 { -1 } else { a.wrapping_div(b) };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::IdivU { dst, lhs, rhs } => {
            // RV32 semantics: x / 0 = all ones.
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            let v = if b == 0 { u32::MAX } else { a.wrapping_div(b) };
            set_reg(regs, *dst, Value::I32(v as i32))?;
        }
        LpirOp::IremS { dst, lhs, rhs } => {
            // RV32 semantics: x % 0 = x, i32::MIN % -1 = 0.
            let a = val_i32(get_reg(regs, *lhs)?)?;
            let b = val_i32(get_reg(regs, *rhs)?)?;
            let v = if b == 0 { a } else { a.wrapping_rem(b) };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::IremU { dst, lhs, rhs } => {
            // RV32 semantics: x % 0 = x.
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            let v = if b == 0 { a } else { a.wrapping_rem(b) };
            set_reg(regs, *dst, Value::I32(v as i32))?;
        }
        LpirOp::Ineg { dst, src } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(a.wrapping_neg()))?;
        }
        LpirOp::Feq { dst, lhs, rhs } => {
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
        LpirOp::Fne { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if a != b || a.is_nan() || b.is_nan() {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Flt { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if !a.is_nan() && !b.is_nan() && a < b {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Fle { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if !a.is_nan() && !b.is_nan() && a <= b {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Fgt { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if !a.is_nan() && !b.is_nan() && a > b {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Fge { dst, lhs, rhs } => {
            let a = val_f32(get_reg(regs, *lhs)?)?;
            let b = val_f32(get_reg(regs, *rhs)?)?;
            let v = if !a.is_nan() && !b.is_nan() && a >= b {
                1
            } else {
                0
            };
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Ieq { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? == val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Ine { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? != val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::IltS { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? < val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::IleS { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? <= val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::IgtS { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? > val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::IgeS { dst, lhs, rhs } => {
            let v = (val_i32(get_reg(regs, *lhs)?)? >= val_i32(get_reg(regs, *rhs)?)?) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::IltU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            set_reg(regs, *dst, Value::I32((a < b) as i32))?;
        }
        LpirOp::IleU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            set_reg(regs, *dst, Value::I32((a <= b) as i32))?;
        }
        LpirOp::IgtU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            set_reg(regs, *dst, Value::I32((a > b) as i32))?;
        }
        LpirOp::IgeU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32;
            set_reg(regs, *dst, Value::I32((a >= b) as i32))?;
        }
        LpirOp::Iand { dst, lhs, rhs } => {
            let v = val_i32(get_reg(regs, *lhs)?)? & val_i32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Ior { dst, lhs, rhs } => {
            let v = val_i32(get_reg(regs, *lhs)?)? | val_i32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Ixor { dst, lhs, rhs } => {
            let v = val_i32(get_reg(regs, *lhs)?)? ^ val_i32(get_reg(regs, *rhs)?)?;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Ibnot { dst, src } => {
            let v = !val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::Ishl { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)?;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32 & 31;
            set_reg(regs, *dst, Value::I32(a.wrapping_shl(b)))?;
        }
        LpirOp::IshrS { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)?;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32 & 31;
            set_reg(regs, *dst, Value::I32(a.wrapping_shr(b)))?;
        }
        LpirOp::IshrU { dst, lhs, rhs } => {
            let a = val_i32(get_reg(regs, *lhs)?)? as u32;
            let b = val_i32(get_reg(regs, *rhs)?)? as u32 & 31;
            set_reg(regs, *dst, Value::I32((a >> b) as i32))?;
        }
        LpirOp::FconstF32 { dst, value } => set_reg(regs, *dst, Value::F32(*value))?,
        LpirOp::IconstI32 { dst, value } => set_reg(regs, *dst, Value::I32(*value))?,
        LpirOp::IaddImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(a.wrapping_add(*imm)))?;
        }
        LpirOp::IsubImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(a.wrapping_sub(*imm)))?;
        }
        LpirOp::ImulImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(a.wrapping_mul(*imm)))?;
        }
        LpirOp::IshlImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            let b = (*imm as u32) & 31;
            set_reg(regs, *dst, Value::I32(a.wrapping_shl(b)))?;
        }
        LpirOp::IshrSImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)?;
            let b = (*imm as u32) & 31;
            set_reg(regs, *dst, Value::I32(a.wrapping_shr(b)))?;
        }
        LpirOp::IshrUImm { dst, src, imm } => {
            let a = val_i32(get_reg(regs, *src)?)? as u32;
            let b = (*imm as u32) & 31;
            set_reg(regs, *dst, Value::I32((a >> b) as i32))?;
        }
        LpirOp::IeqImm { dst, src, imm } => {
            let v = (val_i32(get_reg(regs, *src)?)? == *imm) as i32;
            set_reg(regs, *dst, Value::I32(v))?;
        }
        LpirOp::FtoiSatS { dst, src } => {
            let f = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(ftoi_sat_s(f)))?;
        }
        LpirOp::FtoiSatU { dst, src } => {
            let f = val_f32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::I32(ftoi_sat_u(f)))?;
        }
        LpirOp::ItofS { dst, src } => {
            let v = val_i32(get_reg(regs, *src)?)? as f32;
            set_reg(regs, *dst, Value::F32(v))?;
        }
        LpirOp::ItofU { dst, src } => {
            let v = val_i32(get_reg(regs, *src)?)? as u32 as f32;
            set_reg(regs, *dst, Value::F32(v))?;
        }
        LpirOp::FfromI32Bits { dst, src } => {
            let v = val_i32(get_reg(regs, *src)?)?;
            set_reg(regs, *dst, Value::F32(f32::from_bits(v as u32)))?;
        }
        LpirOp::FtoUnorm16 { dst, src } => {
            let f = val_f32(get_reg(regs, *src)?)?;
            let q = f.to_bits() as i32;
            let out = q.max(0).min(65535);
            set_reg(regs, *dst, Value::I32(out))?;
        }
        LpirOp::FtoUnorm8 { dst, src } => {
            let f = val_f32(get_reg(regs, *src)?)?;
            let q = f.to_bits() as i32;
            let out = (q >> 8).max(0).min(255);
            set_reg(regs, *dst, Value::I32(out))?;
        }
        LpirOp::Unorm16toF { dst, src } => {
            let i = val_i32(get_reg(regs, *src)?)?;
            let bits = (i & 0xFFFF) as u32;
            set_reg(regs, *dst, Value::F32(f32::from_bits(bits)))?;
        }
        LpirOp::Unorm8toF { dst, src } => {
            let i = val_i32(get_reg(regs, *src)?)?;
            let bits = ((i & 0xFF) << 8) as u32;
            set_reg(regs, *dst, Value::F32(f32::from_bits(bits)))?;
        }
        LpirOp::Select {
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
        LpirOp::Copy { dst, src } => {
            let v = get_reg(regs, *src)?;
            set_reg(regs, *dst, v)?;
        }
        LpirOp::SlotAddr { dst, slot } => {
            let off = slot_off
                .get(slot.0 as usize)
                .copied()
                .ok_or_else(|| InterpError::Internal("slot".into()))?;
            set_reg(regs, *dst, Value::I32(off as i32))?;
        }
        LpirOp::Load { dst, base, offset } => {
            let addr = val_i32(get_reg(regs, *base)?)? as usize;
            let addr = addr + *offset as usize;
            let ty = func.vreg_types[dst.0 as usize];
            match ty {
                IrType::F32 => {
                    let b = read_u32(slot_mem, addr)?;
                    set_reg(regs, *dst, Value::F32(f32::from_bits(b)))?;
                }
                IrType::I32 | IrType::Pointer => {
                    let b = read_u32(slot_mem, addr)?;
                    set_reg(regs, *dst, Value::I32(b as i32))?;
                }
            }
        }
        LpirOp::Store {
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
        LpirOp::Store8 {
            base,
            offset,
            value,
        } => {
            let addr = val_i32(get_reg(regs, *base)?)? as usize + *offset as usize;
            let i = val_i32(get_reg(regs, *value)?)?;
            write_u8(slot_mem, addr, i as u8)?;
        }
        LpirOp::Store16 {
            base,
            offset,
            value,
        } => {
            let addr = val_i32(get_reg(regs, *base)?)? as usize + *offset as usize;
            let i = val_i32(get_reg(regs, *value)?)?;
            write_u16(slot_mem, addr, i as u16)?;
        }
        LpirOp::Load8U { dst, base, offset } => {
            let addr = val_i32(get_reg(regs, *base)?)? as usize + *offset as usize;
            let b = read_u8(slot_mem, addr)?;
            set_reg(regs, *dst, Value::I32(b as i32))?;
        }
        LpirOp::Load8S { dst, base, offset } => {
            let addr = val_i32(get_reg(regs, *base)?)? as usize + *offset as usize;
            let b = read_u8(slot_mem, addr)?;
            set_reg(regs, *dst, Value::I32(b as i8 as i32))?;
        }
        LpirOp::Load16U { dst, base, offset } => {
            let addr = val_i32(get_reg(regs, *base)?)? as usize + *offset as usize;
            let w = read_u16(slot_mem, addr)?;
            set_reg(regs, *dst, Value::I32(w as i32))?;
        }
        LpirOp::Load16S { dst, base, offset } => {
            let addr = val_i32(get_reg(regs, *base)?)? as usize + *offset as usize;
            let w = read_u16(slot_mem, addr)?;
            set_reg(regs, *dst, Value::I32(w as i16 as i32))?;
        }
        LpirOp::Memcpy {
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

fn read_u8(mem: &[u8], addr: usize) -> Result<u8, InterpError> {
    if addr + 1 > mem.len() {
        return Err(InterpError::Internal("load oob".into()));
    }
    Ok(mem[addr])
}

fn read_u16(mem: &[u8], addr: usize) -> Result<u16, InterpError> {
    if addr + 2 > mem.len() {
        return Err(InterpError::Internal("load oob".into()));
    }
    Ok(u16::from_le_bytes([mem[addr], mem[addr + 1]]))
}

fn write_u32(mem: &mut [u8], addr: usize, v: u32) -> Result<(), InterpError> {
    if addr + 4 > mem.len() {
        return Err(InterpError::Internal("store oob".into()));
    }
    let b = v.to_le_bytes();
    mem[addr..addr + 4].copy_from_slice(&b);
    Ok(())
}

fn write_u8(mem: &mut [u8], addr: usize, v: u8) -> Result<(), InterpError> {
    if addr + 1 > mem.len() {
        return Err(InterpError::Internal("store oob".into()));
    }
    mem[addr] = v;
    Ok(())
}

fn write_u16(mem: &mut [u8], addr: usize, v: u16) -> Result<(), InterpError> {
    if addr + 2 > mem.len() {
        return Err(InterpError::Internal("store oob".into()));
    }
    let b = v.to_le_bytes();
    mem[addr..addr + 2].copy_from_slice(&b);
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
