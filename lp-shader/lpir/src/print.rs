//! Text format printer (`IrModule` → `String`).

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::lpir_module::{ImportDecl, IrFunction, LpirModule, VMCTX_VREG};
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, IrType, VReg};

fn callee_needs_vmctx_operand(module: &LpirModule, callee: CalleeRef) -> bool {
    let import_count = module.imports.len() as u32;
    if callee.0 >= import_count {
        true
    } else {
        module.imports[callee.0 as usize].needs_vmctx
    }
}

fn visible_call_arg_regs<'a>(
    module: &LpirModule,
    callee: CalleeRef,
    args: &'a [VReg],
) -> &'a [VReg] {
    if callee_needs_vmctx_operand(module, callee) && args.first().copied() == Some(VMCTX_VREG) {
        &args[1..]
    } else {
        args
    }
}

enum Block {
    If,
    Else,
    Loop { start_pc: usize },
    Switch,
    Case,
}

/// Print a full module in LPIR text form.
pub fn print_module(module: &LpirModule) -> String {
    let mut out = String::new();
    for imp in &module.imports {
        print_import(&mut out, imp);
        let _ = writeln!(out);
    }
    if !module.imports.is_empty() && !module.functions.is_empty() {
        let _ = writeln!(out);
    }
    for (i, f) in module.functions.iter().enumerate() {
        if i > 0 {
            let _ = writeln!(out);
        }
        print_function(&mut out, f, module);
    }
    out
}

fn print_import(out: &mut String, imp: &ImportDecl) {
    let _ = write!(out, "import @{}::{}", imp.module_name, imp.func_name);
    print_param_types(out, &imp.param_types);
    if !imp.return_types.is_empty() {
        let _ = write!(out, " -> ");
        print_return_types(out, &imp.return_types);
    }
}

fn print_param_types(out: &mut String, types: &[IrType]) {
    let _ = write!(out, "(");
    for (i, t) in types.iter().enumerate() {
        if i > 0 {
            let _ = write!(out, ", ");
        }
        let _ = write!(out, "{t}");
    }
    let _ = write!(out, ")");
}

fn print_return_types(out: &mut String, types: &[IrType]) {
    if types.len() == 1 {
        let _ = write!(out, "{}", types[0]);
    } else {
        let _ = write!(out, "(");
        for (i, t) in types.iter().enumerate() {
            if i > 0 {
                let _ = write!(out, ", ");
            }
            let _ = write!(out, "{t}");
        }
        let _ = write!(out, ")");
    }
}

fn print_function(out: &mut String, func: &IrFunction, module: &LpirModule) {
    if func.is_entry {
        let _ = write!(out, "entry ");
    }
    let _ = write!(out, "func @{}", func.name);
    let _ = write!(out, "(");
    let vm = func.vmctx_vreg.0 as usize;
    for i in 0..func.param_count as usize {
        if i > 0 {
            let _ = write!(out, ", ");
        }
        let j = vm + 1 + i;
        let _ = write!(out, "{}:{}", VReg(j as u32), func.vreg_types[j]);
    }
    let _ = write!(out, ")");
    if !func.return_types.is_empty() {
        let _ = write!(out, " -> ");
        print_return_types(out, &func.return_types);
    }
    let _ = writeln!(out, " {{");
    for (si, slot) in func.slots.iter().enumerate() {
        let _ = writeln!(out, "  slot ss{}, {}", si, slot.size);
    }
    let mut st = PrintState {
        func,
        module,
        defined: vec![false; func.vreg_types.len()],
    };
    st.defined[vm] = true;
    for i in 0..func.param_count as usize {
        st.defined[vm + 1 + i] = true;
    }
    let mut stack: Vec<Block> = Vec::new();
    let mut pc = 0usize;
    let mut depth = 1usize;
    while pc < func.body.len() {
        print_op_at(out, &mut st, &mut stack, &func.body, &mut pc, &mut depth);
    }
    let _ = writeln!(out, "}}");
}

struct PrintState<'a> {
    func: &'a IrFunction,
    module: &'a LpirModule,
    defined: Vec<bool>,
}

fn vreg_ty(st: &PrintState<'_>, v: VReg) -> IrType {
    st.func.vreg_types[v.0 as usize]
}

fn fmt_vreg(st: &mut PrintState<'_>, out: &mut String, v: VReg) {
    let i = v.0 as usize;
    if st.defined[i] {
        let _ = write!(out, "{v}");
    } else {
        let ty = vreg_ty(st, v);
        let _ = write!(out, "{v}:{ty}");
        st.defined[i] = true;
    }
}

fn indent_str(depth: usize) -> &'static str {
    const SPACES: &str =
        "                                                                                ";
    let n = depth.saturating_mul(2).min(SPACES.len());
    &SPACES[..n]
}

fn print_op_at(
    out: &mut String,
    st: &mut PrintState<'_>,
    stack: &mut Vec<Block>,
    body: &[LpirOp],
    pc: &mut usize,
    depth: &mut usize,
) {
    if let Some(Block::Loop { start_pc }) = stack.last() {
        if let LpirOp::LoopStart {
            continuing_offset, ..
        } = &body[*start_pc]
        {
            let co = *continuing_offset as usize;
            if co != *start_pc + 1 && *pc == co {
                let _ = writeln!(out, "{}continuing:", indent_str(*depth));
            }
        }
    }
    let ind = indent_str(*depth);
    match &body[*pc] {
        LpirOp::IfStart { cond, .. } => {
            let _ = write!(out, "{ind}if ");
            fmt_vreg(st, out, *cond);
            let _ = writeln!(out, " {{");
            stack.push(Block::If);
            *depth += 1;
            *pc += 1;
        }
        LpirOp::Else => {
            if matches!(stack.last(), Some(Block::If)) {
                stack.pop();
                stack.push(Block::Else);
            }
            let _ = writeln!(out, "{}}} else {{", indent_str(*depth - 1));
            *pc += 1;
        }
        LpirOp::LoopStart { .. } => {
            let _ = writeln!(out, "{ind}loop {{");
            stack.push(Block::Loop { start_pc: *pc });
            *depth += 1;
            *pc += 1;
        }
        LpirOp::SwitchStart { selector, .. } => {
            let _ = write!(out, "{ind}switch ");
            fmt_vreg(st, out, *selector);
            let _ = writeln!(out, " {{");
            stack.push(Block::Switch);
            *depth += 1;
            *pc += 1;
        }
        LpirOp::CaseStart { value, .. } => {
            if matches!(stack.last(), Some(Block::Case)) {
                stack.pop();
                let _ = writeln!(out, "{}}}", indent_str(*depth - 1));
                *depth -= 1;
            }
            let _ = writeln!(out, "{}case {value} {{", indent_str(*depth));
            stack.push(Block::Case);
            *depth += 1;
            *pc += 1;
        }
        LpirOp::DefaultStart { .. } => {
            if matches!(stack.last(), Some(Block::Case)) {
                stack.pop();
                let _ = writeln!(out, "{}}}", indent_str(*depth - 1));
                *depth -= 1;
            }
            let _ = writeln!(out, "{}default {{", indent_str(*depth));
            stack.push(Block::Case);
            *depth += 1;
            *pc += 1;
        }
        LpirOp::End => {
            let _ = writeln!(out, "{}}}", indent_str(*depth - 1));
            *depth -= 1;
            let _ = stack.pop();
            *pc += 1;
        }
        LpirOp::Break => {
            let _ = writeln!(out, "{ind}break");
            *pc += 1;
        }
        LpirOp::Continue => {
            let _ = writeln!(out, "{ind}continue");
            *pc += 1;
        }
        LpirOp::BrIfNot { cond } => {
            let _ = write!(out, "{ind}br_if_not ");
            fmt_vreg(st, out, *cond);
            let _ = writeln!(out);
            *pc += 1;
        }
        LpirOp::Return { values } => {
            let _ = write!(out, "{ind}return");
            if !values.is_empty() {
                let slice = st.func.pool_slice(*values);
                for (i, v) in slice.iter().enumerate() {
                    let _ = write!(out, "{}", if i == 0 { " " } else { ", " });
                    fmt_vreg(st, out, *v);
                }
            }
            let _ = writeln!(out);
            *pc += 1;
        }
        LpirOp::Call {
            callee,
            args,
            results,
        } => {
            let args_s = st.func.pool_slice(*args);
            let visible = visible_call_arg_regs(st.module, *callee, args_s);
            let res_s = st.func.pool_slice(*results);
            let (mod_n, fn_n) = callee_name(st.module, *callee);
            let _ = write!(out, "{ind}");
            if !res_s.is_empty() {
                for (i, v) in res_s.iter().enumerate() {
                    if i > 0 {
                        let _ = write!(out, ", ");
                    }
                    fmt_vreg(st, out, *v);
                }
                let _ = write!(out, " = ");
            }
            if mod_n.is_empty() {
                let _ = write!(out, "call @{fn_n}(");
            } else {
                let _ = write!(out, "call @{mod_n}::{fn_n}(");
            }
            for (i, v) in visible.iter().enumerate() {
                if i > 0 {
                    let _ = write!(out, ", ");
                }
                fmt_vreg(st, out, *v);
            }
            let _ = writeln!(out, ")");
            *pc += 1;
        }
        LpirOp::Store {
            base,
            offset,
            value,
        } => {
            let _ = write!(out, "{ind}store ");
            fmt_vreg(st, out, *base);
            let _ = write!(out, ", {offset}, ");
            fmt_vreg(st, out, *value);
            let _ = writeln!(out);
            *pc += 1;
        }
        LpirOp::Memcpy {
            dst_addr,
            src_addr,
            size,
        } => {
            let _ = write!(out, "{ind}memcpy ");
            fmt_vreg(st, out, *dst_addr);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *src_addr);
            let _ = writeln!(out, ", {size}");
            *pc += 1;
        }
        op => {
            print_simple_op(out, st, ind, op);
            *pc += 1;
        }
    }
}

fn callee_name(module: &LpirModule, callee: crate::types::CalleeRef) -> (&str, &str) {
    if let Some(i) = module.callee_as_import(callee) {
        let imp = &module.imports[i];
        (imp.module_name.as_str(), imp.func_name.as_str())
    } else if let Some(i) = module.callee_as_function(callee) {
        let f = &module.functions[i];
        ("", f.name.as_str())
    } else {
        ("?", "?")
    }
}

fn print_simple_op(out: &mut String, st: &mut PrintState<'_>, ind: &str, op: &LpirOp) {
    match op {
        LpirOp::Fadd { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = fadd ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::Fsub { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = fsub ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::Fmul { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = fmul ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::Fdiv { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = fdiv ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::Fneg { dst, src } => unary(out, st, ind, "fneg", *dst, *src),
        LpirOp::Fabs { dst, src } => unary(out, st, ind, "fabs", *dst, *src),
        LpirOp::Fsqrt { dst, src } => unary(out, st, ind, "fsqrt", *dst, *src),
        LpirOp::Fmin { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fmin", *dst, *lhs, *rhs),
        LpirOp::Fmax { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fmax", *dst, *lhs, *rhs),
        LpirOp::Ffloor { dst, src } => unary(out, st, ind, "ffloor", *dst, *src),
        LpirOp::Fceil { dst, src } => unary(out, st, ind, "fceil", *dst, *src),
        LpirOp::Ftrunc { dst, src } => unary(out, st, ind, "ftrunc", *dst, *src),
        LpirOp::Fnearest { dst, src } => unary(out, st, ind, "fnearest", *dst, *src),
        LpirOp::Iadd { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = iadd ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::Isub { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = isub ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::Imul { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = imul ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::IdivS { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = idiv_s ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::IdivU { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = idiv_u ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::IremS { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = irem_s ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::IremU { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = irem_u ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        LpirOp::Ineg { dst, src } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = ineg ");
            fmt_vreg(st, out, *src);
            let _ = writeln!(out);
        }
        LpirOp::Feq { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "feq", *dst, *lhs, *rhs),
        LpirOp::Fne { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fne", *dst, *lhs, *rhs),
        LpirOp::Flt { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "flt", *dst, *lhs, *rhs),
        LpirOp::Fle { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fle", *dst, *lhs, *rhs),
        LpirOp::Fgt { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fgt", *dst, *lhs, *rhs),
        LpirOp::Fge { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fge", *dst, *lhs, *rhs),
        LpirOp::Ieq { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ieq", *dst, *lhs, *rhs),
        LpirOp::Ine { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ine", *dst, *lhs, *rhs),
        LpirOp::IltS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ilt_s", *dst, *lhs, *rhs),
        LpirOp::IleS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ile_s", *dst, *lhs, *rhs),
        LpirOp::IgtS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "igt_s", *dst, *lhs, *rhs),
        LpirOp::IgeS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ige_s", *dst, *lhs, *rhs),
        LpirOp::IltU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ilt_u", *dst, *lhs, *rhs),
        LpirOp::IleU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ile_u", *dst, *lhs, *rhs),
        LpirOp::IgtU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "igt_u", *dst, *lhs, *rhs),
        LpirOp::IgeU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ige_u", *dst, *lhs, *rhs),
        LpirOp::Iand { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "iand", *dst, *lhs, *rhs),
        LpirOp::Ior { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ior", *dst, *lhs, *rhs),
        LpirOp::Ixor { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ixor", *dst, *lhs, *rhs),
        LpirOp::Ibnot { dst, src } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = ibnot ");
            fmt_vreg(st, out, *src);
            let _ = writeln!(out);
        }
        LpirOp::Ishl { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ishl", *dst, *lhs, *rhs),
        LpirOp::IshrS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ishr_s", *dst, *lhs, *rhs),
        LpirOp::IshrU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ishr_u", *dst, *lhs, *rhs),
        LpirOp::FconstF32 { dst, value } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = writeln!(out, " = fconst.f32 {}", fmt_f32(*value));
        }
        LpirOp::IconstI32 { dst, value } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = writeln!(out, " = iconst.i32 {value}");
        }
        LpirOp::IaddImm { dst, src, imm } => imm_op(out, st, ind, "iadd_imm", *dst, *src, *imm),
        LpirOp::IsubImm { dst, src, imm } => imm_op(out, st, ind, "isub_imm", *dst, *src, *imm),
        LpirOp::ImulImm { dst, src, imm } => imm_op(out, st, ind, "imul_imm", *dst, *src, *imm),
        LpirOp::IshlImm { dst, src, imm } => imm_op(out, st, ind, "ishl_imm", *dst, *src, *imm),
        LpirOp::IshrSImm { dst, src, imm } => imm_op(out, st, ind, "ishr_s_imm", *dst, *src, *imm),
        LpirOp::IshrUImm { dst, src, imm } => imm_op(out, st, ind, "ishr_u_imm", *dst, *src, *imm),
        LpirOp::IeqImm { dst, src, imm } => imm_op(out, st, ind, "ieq_imm", *dst, *src, *imm),
        LpirOp::FtoiSatS { dst, src } => unary(out, st, ind, "ftoi_sat_s", *dst, *src),
        LpirOp::FtoiSatU { dst, src } => unary(out, st, ind, "ftoi_sat_u", *dst, *src),
        LpirOp::ItofS { dst, src } => unary(out, st, ind, "itof_s", *dst, *src),
        LpirOp::ItofU { dst, src } => unary(out, st, ind, "itof_u", *dst, *src),
        LpirOp::FfromI32Bits { dst, src } => unary(out, st, ind, "ffrom_i32_bits", *dst, *src),
        LpirOp::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = select ");
            fmt_vreg(st, out, *cond);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *if_true);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *if_false);
            let _ = writeln!(out);
        }
        LpirOp::Copy { dst, src } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = copy ");
            fmt_vreg(st, out, *src);
            let _ = writeln!(out);
        }
        LpirOp::SlotAddr { dst, slot } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = writeln!(out, " = slot_addr {slot}");
        }
        LpirOp::Load { dst, base, offset } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = load ");
            fmt_vreg(st, out, *base);
            let _ = writeln!(out, ", {offset}");
        }
        _ => {
            let _ = writeln!(out, "{ind}; unprintable op");
        }
    }
}

fn bin_int_cmp(
    out: &mut String,
    st: &mut PrintState<'_>,
    ind: &str,
    name: &str,
    dst: VReg,
    lhs: VReg,
    rhs: VReg,
) {
    let _ = write!(out, "{ind}");
    fmt_vreg(st, out, dst);
    let _ = write!(out, " = {name} ");
    fmt_vreg(st, out, lhs);
    let _ = write!(out, ", ");
    fmt_vreg(st, out, rhs);
    let _ = writeln!(out);
}

fn imm_op(
    out: &mut String,
    st: &mut PrintState<'_>,
    ind: &str,
    name: &str,
    dst: VReg,
    src: VReg,
    imm: i32,
) {
    let _ = write!(out, "{ind}");
    fmt_vreg(st, out, dst);
    let _ = write!(out, " = {name} ");
    fmt_vreg(st, out, src);
    let _ = writeln!(out, ", {imm}");
}

fn unary(out: &mut String, st: &mut PrintState<'_>, ind: &str, name: &str, dst: VReg, src: VReg) {
    let _ = write!(out, "{ind}");
    fmt_vreg(st, out, dst);
    let _ = write!(out, " = {name} ");
    fmt_vreg(st, out, src);
    let _ = writeln!(out);
}

fn fmt_f32(v: f32) -> String {
    if v.is_nan() {
        return String::from("nan");
    }
    if v.is_infinite() {
        return if v.is_sign_negative() {
            String::from("-inf")
        } else {
            String::from("inf")
        };
    }
    format!("{v:?}")
}
