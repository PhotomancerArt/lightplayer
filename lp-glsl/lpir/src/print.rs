//! Text format printer (`IrModule` → `String`).

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::module::{ImportDecl, IrFunction, IrModule};
use crate::op::Op;
use crate::types::{IrType, VReg};

enum Block {
    If,
    Else,
    Loop { start_pc: usize },
    Switch,
    Case,
}

/// Print a full module in LPIR text form.
pub fn print_module(module: &IrModule) -> String {
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

fn print_function(out: &mut String, func: &IrFunction, module: &IrModule) {
    if func.is_entry {
        let _ = write!(out, "entry ");
    }
    let _ = write!(out, "func @{}", func.name);
    let _ = write!(out, "(");
    for i in 0..func.param_count as usize {
        if i > 0 {
            let _ = write!(out, ", ");
        }
        let _ = write!(out, "{}:{}", VReg(i as u32), func.vreg_types[i]);
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
    for i in 0..func.param_count as usize {
        st.defined[i] = true;
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
    module: &'a IrModule,
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
    body: &[Op],
    pc: &mut usize,
    depth: &mut usize,
) {
    if let Some(Block::Loop { start_pc }) = stack.last() {
        if let Op::LoopStart {
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
        Op::IfStart { cond, .. } => {
            let _ = write!(out, "{ind}if ");
            fmt_vreg(st, out, *cond);
            let _ = writeln!(out, " {{");
            stack.push(Block::If);
            *depth += 1;
            *pc += 1;
        }
        Op::Else => {
            if matches!(stack.last(), Some(Block::If)) {
                stack.pop();
                stack.push(Block::Else);
            }
            let _ = writeln!(out, "{}}} else {{", indent_str(*depth - 1));
            *pc += 1;
        }
        Op::LoopStart { .. } => {
            let _ = writeln!(out, "{ind}loop {{");
            stack.push(Block::Loop { start_pc: *pc });
            *depth += 1;
            *pc += 1;
        }
        Op::SwitchStart { selector, .. } => {
            let _ = write!(out, "{ind}switch ");
            fmt_vreg(st, out, *selector);
            let _ = writeln!(out, " {{");
            stack.push(Block::Switch);
            *depth += 1;
            *pc += 1;
        }
        Op::CaseStart { value, .. } => {
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
        Op::DefaultStart { .. } => {
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
        Op::End => {
            let _ = writeln!(out, "{}}}", indent_str(*depth - 1));
            *depth -= 1;
            let _ = stack.pop();
            *pc += 1;
        }
        Op::Break => {
            let _ = writeln!(out, "{ind}break");
            *pc += 1;
        }
        Op::Continue => {
            let _ = writeln!(out, "{ind}continue");
            *pc += 1;
        }
        Op::BrIfNot { cond } => {
            let _ = write!(out, "{ind}br_if_not ");
            fmt_vreg(st, out, *cond);
            let _ = writeln!(out);
            *pc += 1;
        }
        Op::Return { values } => {
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
        Op::Call {
            callee,
            args,
            results,
        } => {
            let args_s = st.func.pool_slice(*args);
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
            for (i, v) in args_s.iter().enumerate() {
                if i > 0 {
                    let _ = write!(out, ", ");
                }
                fmt_vreg(st, out, *v);
            }
            let _ = writeln!(out, ")");
            *pc += 1;
        }
        Op::Store {
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
        Op::Memcpy {
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

fn callee_name(module: &IrModule, callee: crate::types::CalleeRef) -> (&str, &str) {
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

fn print_simple_op(out: &mut String, st: &mut PrintState<'_>, ind: &str, op: &Op) {
    match op {
        Op::Fadd { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = fadd ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::Fsub { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = fsub ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::Fmul { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = fmul ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::Fdiv { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = fdiv ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::Fneg { dst, src } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = fneg ");
            fmt_vreg(st, out, *src);
            let _ = writeln!(out);
        }
        Op::Iadd { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = iadd ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::Isub { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = isub ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::Imul { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = imul ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::IdivS { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = idiv_s ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::IdivU { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = idiv_u ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::IremS { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = irem_s ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::IremU { dst, lhs, rhs } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = irem_u ");
            fmt_vreg(st, out, *lhs);
            let _ = write!(out, ", ");
            fmt_vreg(st, out, *rhs);
            let _ = writeln!(out);
        }
        Op::Ineg { dst, src } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = ineg ");
            fmt_vreg(st, out, *src);
            let _ = writeln!(out);
        }
        Op::Feq { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "feq", *dst, *lhs, *rhs),
        Op::Fne { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fne", *dst, *lhs, *rhs),
        Op::Flt { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "flt", *dst, *lhs, *rhs),
        Op::Fle { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fle", *dst, *lhs, *rhs),
        Op::Fgt { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fgt", *dst, *lhs, *rhs),
        Op::Fge { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fge", *dst, *lhs, *rhs),
        Op::Ieq { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ieq", *dst, *lhs, *rhs),
        Op::Ine { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ine", *dst, *lhs, *rhs),
        Op::IltS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ilt_s", *dst, *lhs, *rhs),
        Op::IleS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ile_s", *dst, *lhs, *rhs),
        Op::IgtS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "igt_s", *dst, *lhs, *rhs),
        Op::IgeS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ige_s", *dst, *lhs, *rhs),
        Op::IltU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ilt_u", *dst, *lhs, *rhs),
        Op::IleU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ile_u", *dst, *lhs, *rhs),
        Op::IgtU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "igt_u", *dst, *lhs, *rhs),
        Op::IgeU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ige_u", *dst, *lhs, *rhs),
        Op::Iand { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "iand", *dst, *lhs, *rhs),
        Op::Ior { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ior", *dst, *lhs, *rhs),
        Op::Ixor { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ixor", *dst, *lhs, *rhs),
        Op::Ibnot { dst, src } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = ibnot ");
            fmt_vreg(st, out, *src);
            let _ = writeln!(out);
        }
        Op::Ishl { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ishl", *dst, *lhs, *rhs),
        Op::IshrS { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ishr_s", *dst, *lhs, *rhs),
        Op::IshrU { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "ishr_u", *dst, *lhs, *rhs),
        Op::FconstF32 { dst, value } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = writeln!(out, " = fconst.f32 {}", fmt_f32(*value));
        }
        Op::IconstI32 { dst, value } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = writeln!(out, " = iconst.i32 {value}");
        }
        Op::IaddImm { dst, src, imm } => imm_op(out, st, ind, "iadd_imm", *dst, *src, *imm),
        Op::IsubImm { dst, src, imm } => imm_op(out, st, ind, "isub_imm", *dst, *src, *imm),
        Op::ImulImm { dst, src, imm } => imm_op(out, st, ind, "imul_imm", *dst, *src, *imm),
        Op::IshlImm { dst, src, imm } => imm_op(out, st, ind, "ishl_imm", *dst, *src, *imm),
        Op::IshrSImm { dst, src, imm } => imm_op(out, st, ind, "ishr_s_imm", *dst, *src, *imm),
        Op::IshrUImm { dst, src, imm } => imm_op(out, st, ind, "ishr_u_imm", *dst, *src, *imm),
        Op::IeqImm { dst, src, imm } => imm_op(out, st, ind, "ieq_imm", *dst, *src, *imm),
        Op::FtoiSatS { dst, src } => unary(out, st, ind, "ftoi_sat_s", *dst, *src),
        Op::FtoiSatU { dst, src } => unary(out, st, ind, "ftoi_sat_u", *dst, *src),
        Op::ItofS { dst, src } => unary(out, st, ind, "itof_s", *dst, *src),
        Op::ItofU { dst, src } => unary(out, st, ind, "itof_u", *dst, *src),
        Op::Select {
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
        Op::Copy { dst, src } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = write!(out, " = copy ");
            fmt_vreg(st, out, *src);
            let _ = writeln!(out);
        }
        Op::SlotAddr { dst, slot } => {
            let _ = write!(out, "{ind}");
            fmt_vreg(st, out, *dst);
            let _ = writeln!(out, " = slot_addr {slot}");
        }
        Op::Load { dst, base, offset } => {
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
