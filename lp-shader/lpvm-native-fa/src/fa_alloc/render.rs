//! Human-readable rendering of AllocOutput for snapshot tests.

use crate::abi::FuncAbi;
use crate::abi::classify::{ArgLoc, ReturnMethod};
use crate::fa_alloc::{Alloc, AllocOutput, Edit, EditPoint};
use crate::rv32::gpr::reg_name;
use crate::vinst::{VInst, VReg};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lpir::{IrFunction, LpirModule, LpirOp, print_module};

/// Indentation for LPIR body lines (after `; ` in the file).
const IND_LP: &str = "    ";
/// Indentation for VInst lines, read, and write annotations.
const IND_VI: &str = "        ";

/// First line of [`print_module`] for a copy of `func` with empty body (header only).
fn format_func_header_line(func: &IrFunction, module: &LpirModule) -> String {
    let mut f = func.clone();
    f.body.clear();
    f.slots.clear();
    let m = LpirModule {
        imports: module.imports.clone(),
        functions: alloc::vec![f],
    };
    let s = print_module(&m);
    s.lines()
        .next()
        .unwrap_or("func @???")
        .trim_end()
        .to_string()
}

/// One body line for `op` using the canonical LPIR printer (matches `lpir::print`).
fn format_lpir_op_line(func: &IrFunction, module: &LpirModule, op: &LpirOp) -> String {
    let mut f = func.clone();
    f.body = alloc::vec![op.clone()];
    f.slots.clear();
    let m = LpirModule {
        imports: module.imports.clone(),
        functions: alloc::vec![f],
    };
    let s = print_module(&m);
    let mut in_body = false;
    let mut out = Vec::new();
    for line in s.lines() {
        if line.ends_with(" {") {
            in_body = true;
            continue;
        }
        if in_body {
            if line.trim_start() == "}" {
                break;
            }
            let t = line.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
        }
    }
    if out.len() == 1 {
        out.remove(0)
    } else if out.is_empty() {
        format!("{op:?}")
    } else {
        out.join("\n")
    }
}

/// Render AllocOutput as interleaved LPIR + VInsts + allocations.
///
/// Lines do **not** include the leading `; ` — the filetest runner adds that so each
/// snapshot line is valid LPIR comment syntax.
///
/// ```text
/// func @test() -> i32 {
///     ; spill_slots: 0
///     ; ret: void
///
///     v1:i32 = iconst.i32 10
///         i1 = IConst32 10
///             ; write: i1 -> t0
///
///     return v1
///         ; read: i1 <- t0
///         Ret i1
///         Br L0
/// }
///     Label L0
/// ```
pub fn render_interleaved(
    func: &IrFunction,
    module: &LpirModule,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
    func_abi: &FuncAbi,
) -> String {
    let mut lines = Vec::new();

    let mut vinsts_by_src_op: alloc::collections::BTreeMap<u32, Vec<(usize, &VInst)>> =
        alloc::collections::BTreeMap::new();

    for (inst_idx, inst) in vinsts.iter().enumerate() {
        if let Some(src_op) = inst.src_op() {
            vinsts_by_src_op
                .entry(src_op)
                .or_default()
                .push((inst_idx, inst));
        }
    }

    let mut rendered_vinsts = alloc::collections::BTreeSet::new();

    lines.push(format_func_header_line(func, module));
    push_alloc_metadata_lines(&mut lines, func, output, func_abi);
    lines.push(String::new());

    for (i, op) in func.body.iter().enumerate() {
        if i > 0 {
            lines.push(String::new());
        }

        let lpir_line = format_lpir_op_line(func, module, op);
        lines.push(format!("{IND_LP}{}", lpir_line));

        if let Some(vinst_list) = vinsts_by_src_op.get(&(i as u32)) {
            for (vinst_idx, inst) in vinst_list.iter() {
                rendered_vinsts.insert(*vinst_idx);
                push_vinst_snapshot_block(&mut lines, *vinst_idx, inst, vinsts, vreg_pool, output);
            }
        }
    }

    lines.push("}".to_string());

    let mut first_epilogue = true;
    for (inst_idx, inst) in vinsts.iter().enumerate() {
        if rendered_vinsts.contains(&inst_idx) {
            continue;
        }
        if !first_epilogue {
            lines.push(String::new());
        }
        first_epilogue = false;
        // Epilogue (e.g. Label): align with body LPIR lines, not nested under an op.
        // For epilogue (Label etc): use body indent for edits/reads/VInst, LP indent for writes
        push_vinst_snapshot_block_raw(
            &mut lines, inst_idx, inst, vreg_pool, output, IND_LP, IND_VI,
        );
    }

    lines.join("\n")
}

fn push_vinst_snapshot_block(
    lines: &mut Vec<String>,
    vinst_idx: usize,
    inst: &VInst,
    _vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
) {
    push_vinst_snapshot_block_raw(lines, vinst_idx, inst, vreg_pool, output, IND_VI, IND_VI);
}

fn push_vinst_snapshot_block_raw(
    lines: &mut Vec<String>,
    vinst_idx: usize,
    inst: &VInst,
    vreg_pool: &[VReg],
    output: &AllocOutput,
    indent_vinst: &str,
    _indent_edit_read: &str,
) {
    let inst_idx_u16 = vinst_idx as u16;
    let offset = output.inst_alloc_offsets[vinst_idx] as usize;
    let mut num_defs: usize = 0;
    let mut num_uses: usize = 0;
    inst.for_each_def(vreg_pool, |_def| num_defs += 1);
    inst.for_each_use(vreg_pool, |_use| num_uses += 1);
    let def_allocs: Vec<_> = (0..num_defs).map(|i| output.allocs[offset + i]).collect();
    let use_allocs: Vec<_> = (0..num_uses)
        .map(|i| output.allocs[offset + num_defs + i])
        .collect();
    let mut def_vregs: Vec<VReg> = Vec::new();
    inst.for_each_def(vreg_pool, |vreg| def_vregs.push(vreg));
    let mut use_vregs: Vec<VReg> = Vec::new();
    inst.for_each_use(vreg_pool, |vreg| use_vregs.push(vreg));

    let before_edits: Vec<_> = output
        .edits
        .iter()
        .filter(|(pt, _)| *pt == EditPoint::Before(inst_idx_u16))
        .map(|(_, edit)| edit)
        .collect();

    // Before-edits, reads, VInst, and writes all at same indentation (part of one op)
    for edit in &before_edits {
        lines.push(format!("{indent_vinst}; {}", format_edit(edit)));
    }
    for (vreg, alloc) in use_vregs.iter().zip(use_allocs.iter()) {
        lines.push(format!(
            "{indent_vinst}; read: i{} <- {}",
            vreg.0,
            format_alloc(*alloc)
        ));
    }
    lines.push(format!("{indent_vinst}{}", format_inst(inst, vreg_pool)));
    for (vreg, alloc) in def_vregs.iter().zip(def_allocs.iter()) {
        lines.push(format!(
            "{indent_vinst}; write: i{} -> {}",
            vreg.0,
            format_alloc(*alloc)
        ));
    }
}

/// Render AllocOutput as human-readable text for snapshot tests.
pub fn render_alloc_output(vinsts: &[VInst], vreg_pool: &[VReg], output: &AllocOutput) -> String {
    let mut lines = Vec::new();

    for (inst_idx, inst) in vinsts.iter().enumerate() {
        let inst_idx_u16 = inst_idx as u16;

        // Get allocations for this instruction
        let offset = output.inst_alloc_offsets[inst_idx] as usize;
        let mut num_defs: usize = 0;
        let mut num_uses: usize = 0;

        // Count defs and uses
        inst.for_each_def(vreg_pool, |_def| num_defs += 1);
        inst.for_each_use(vreg_pool, |_use| num_uses += 1);

        // Get def allocations (first `num_defs` operands)
        let def_allocs: Vec<_> = (0..num_defs).map(|i| output.allocs[offset + i]).collect();

        // Get use allocations (next `num_uses` operands)
        let use_allocs: Vec<_> = (0..num_uses)
            .map(|i| output.allocs[offset + num_defs + i])
            .collect();

        // Get def and use vregs
        let mut def_vregs: Vec<VReg> = Vec::new();
        inst.for_each_def(vreg_pool, |vreg| def_vregs.push(vreg));

        let mut use_vregs: Vec<VReg> = Vec::new();
        inst.for_each_use(vreg_pool, |vreg| use_vregs.push(vreg));

        // Add separator before each instruction (except first)
        if inst_idx > 0 {
            lines.push("; ---------------------------".to_string());
        }

        // Render edits Before instruction (reloads, evictions)
        let before_edits: Vec<_> = output
            .edits
            .iter()
            .filter(|(pt, _)| *pt == EditPoint::Before(inst_idx_u16))
            .map(|(_, edit)| edit)
            .collect();

        for edit in &before_edits {
            lines.push(format!("; {}", format_edit(edit)));
        }

        // Render read (use) allocations
        for (_i, (vreg, alloc)) in use_vregs.iter().zip(use_allocs.iter()).enumerate() {
            lines.push(format!("; read: i{} <- {}", vreg.0, format_alloc(*alloc)));
        }

        // Render the instruction
        lines.push(format_inst(inst, vreg_pool));

        // Render write (def) allocations
        for (_i, (vreg, alloc)) in def_vregs.iter().zip(def_allocs.iter()).enumerate() {
            lines.push(format!("; write: i{} -> {}", vreg.0, format_alloc(*alloc)));
        }
    }

    lines.join("\n")
}

/// Format an instruction as text (matches `debug::vinst` ireg style: `i0`, `i1`, …).
fn format_inst(inst: &VInst, vreg_pool: &[VReg]) -> String {
    match inst {
        VInst::IConst32 { dst, val, .. } => {
            format!("i{} = IConst32 {}", dst.0, val)
        }
        VInst::Add32 {
            dst, src1, src2, ..
        } => {
            format!("i{} = Add32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::Sub32 {
            dst, src1, src2, ..
        } => {
            format!("i{} = Sub32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::Mul32 {
            dst, src1, src2, ..
        } => {
            format!("i{} = Mul32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::Ret { vals, .. } => {
            let vals = vals.vregs(vreg_pool);
            if vals.is_empty() {
                "Ret".to_string()
            } else if vals.len() == 1 {
                format!("Ret i{}", vals[0].0)
            } else {
                let parts: Vec<_> = vals.iter().map(|v| format!("i{}", v.0)).collect();
                format!("Ret ({})", parts.join(", "))
            }
        }
        VInst::Br { target, .. } => {
            format!("Br L{}", target)
        }
        VInst::BrIf { target, invert, .. } => {
            if *invert {
                format!("BrIf L{} invert", target)
            } else {
                format!("BrIf L{}", target)
            }
        }
        VInst::Label(id, _) => {
            format!("Label L{}", id)
        }
        _ => {
            format!("{} (unformatted)", inst.mnemonic())
        }
    }
}

/// Format an allocation as a string.
fn format_alloc(alloc: Alloc) -> String {
    match alloc {
        Alloc::Reg(preg) => {
            // Map PReg to ABI name
            String::from(match preg {
                5 => "t0",
                6 => "t1",
                7 => "t2",
                8 => "s0",
                9 => "s1",
                10 => "a0",
                11 => "a1",
                12 => "a2",
                13 => "a3",
                14 => "a4",
                15 => "a5",
                16 => "a6",
                17 => "a7",
                18 => "s2",
                19 => "s3",
                20 => "s4",
                21 => "s5",
                22 => "s6",
                23 => "s7",
                24 => "s8",
                25 => "s9",
                26 => "s10",
                27 => "s11",
                28 => "t3",
                29 => "t4",
                30 => "t5",
                31 => "t6",
                _ => "??",
            })
        }
        Alloc::Stack(slot) => format!("slot{}", slot),
        Alloc::None => String::from("none"),
    }
}

/// Format an edit as a string.
fn format_edit(edit: &Edit) -> String {
    match edit {
        Edit::Move { from, to } => {
            format!("move: {} -> {}", format_alloc(*from), format_alloc(*to))
        }
    }
}

fn format_arg_loc(loc: &ArgLoc) -> String {
    match loc {
        ArgLoc::Reg(p) => reg_name(p.hw).to_string(),
        ArgLoc::Stack { offset, .. } => format!("stack+{offset}"),
    }
}

fn format_return_method(rm: &ReturnMethod) -> String {
    match rm {
        ReturnMethod::Void => String::from("void"),
        ReturnMethod::Direct { locs } => {
            if locs.is_empty() {
                String::from("void")
            } else {
                locs.iter()
                    .map(|l| format_arg_loc(l))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
        ReturnMethod::Sret {
            ptr_reg,
            preserved_reg,
            word_count,
        } => format!(
            "sret ({word_count} words, ptr={}, preserved={})",
            reg_name(ptr_reg.hw),
            reg_name(preserved_reg.hw)
        ),
    }
}

fn push_alloc_metadata_lines(
    lines: &mut Vec<String>,
    func: &IrFunction,
    output: &AllocOutput,
    func_abi: &FuncAbi,
) {
    lines.push(format!("{IND_LP}; spill_slots: {}", output.num_spill_slots));
    for i in 0..func.param_count as usize {
        let v = func.user_param_vreg(i as u16);
        if let Some(loc) = func_abi.param_loc(i) {
            lines.push(format!("{IND_LP}; arg v{}: {}", v.0, format_arg_loc(&loc)));
        }
    }
    lines.push(format!(
        "{IND_LP}; ret: {}",
        format_return_method(func_abi.return_method())
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::FuncAbi;
    use crate::debug::vinst;
    use crate::fa_alloc::walk;
    use crate::rv32::abi;
    use lps_shared::{LpsFnSig, LpsType};

    fn make_abi() -> FuncAbi {
        abi::func_abi_rv32(
            &LpsFnSig {
                name: alloc::string::String::from("test"),
                return_type: LpsType::Void,
                parameters: vec![],
            },
            0,
        )
    }

    #[test]
    fn render_simple_iconst() {
        let input = "i0 = IConst32 10\nRet i0";
        let (vinsts, _symbols, pool) = vinst::parse(input).unwrap();
        let output = walk::walk_linear(&vinsts, &pool, &make_abi()).unwrap();
        let rendered = render_alloc_output(&vinsts, &pool, &output);

        // Just check it doesn't panic and contains expected text
        assert!(rendered.contains("IConst32"));
        assert!(rendered.contains("Ret"));
    }
}
