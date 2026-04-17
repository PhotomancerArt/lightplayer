//! Human-readable rendering of AllocOutput for snapshot tests.

use crate::abi::FuncAbi;
use crate::abi::classify::{ArgLoc, ReturnMethod};
use crate::regalloc::trace::TraceEntry;
use crate::regalloc::{
    Alloc, AllocOutput, Edit, EditPoint, append_entry_trace_metadata_lines, trace_by_vinst_or_empty,
};
use crate::rv32::gpr::reg_name;
use crate::vinst::{IcmpCond, ModuleSymbols, VInst, VReg};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lpir::{FuncId, IrFunction, LpirModule, LpirOp, print_module};

/// Indentation for LPIR body lines (after `; ` in the file).
const IND_LP: &str = "    ";
/// Indentation for VInst lines, read, and write annotations.
const IND_VI: &str = "        ";

/// First line of [`print_module`] for a copy of `func` with empty body (header only).
fn format_func_header_line(func: &IrFunction, module: &LpirModule, func_abi: &FuncAbi) -> String {
    let mut f = func.clone();
    f.body.clear();
    f.slots.clear();
    // Ensure vreg_types covers ABI params for proper printing
    let total_abi_slots = func_abi.param_locs().len(); // vmctx + user params
    if f.vreg_types.len() < total_abi_slots {
        while f.vreg_types.len() < total_abi_slots {
            f.vreg_types.push(lpir::IrType::I32);
        }
    }
    // Also update param_count to match ABI for printing
    f.param_count = (total_abi_slots.saturating_sub(1)) as u16; // exclude vmctx
    let id = module
        .functions
        .iter()
        .find(|(_, mf)| mf.name == func.name)
        .map(|(k, _)| *k)
        .unwrap_or(FuncId(0));
    let m = LpirModule {
        imports: module.imports.clone(),
        functions: BTreeMap::from([(id, f)]),
    };
    let s = print_module(&m);
    s.lines()
        .next()
        .unwrap_or("func @???")
        .trim_end()
        .to_string()
}

/// One body line for `op` using the canonical LPIR printer (matches `lpir::print`).
fn format_lpir_op_line(
    func: &IrFunction,
    module: &LpirModule,
    _op_idx: usize,
    op: &LpirOp,
) -> String {
    let mut f = func.clone();
    f.body = alloc::vec![op.clone()];
    f.slots.clear();
    // Replace the function in the module while preserving all other functions
    // so that CalleeRef indices remain valid.
    let mut functions = module.functions.clone();
    if let Some((k, _)) = functions.iter().find(|(_, mf)| mf.name == func.name) {
        let k = *k;
        functions.insert(k, f);
    } else {
        functions.insert(FuncId(0), f);
    }
    let m = LpirModule {
        imports: module.imports.clone(),
        functions,
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
    symbols: &ModuleSymbols,
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

    let trace_by_vinst = trace_by_vinst_or_empty(output);

    lines.push(format_func_header_line(func, module, func_abi));
    push_alloc_metadata_lines(&mut lines, func, output, func_abi);
    lines.push(String::new());

    for (i, op) in func.body.iter().enumerate() {
        if i > 0 {
            lines.push(String::new());
        }

        let lpir_line = format_lpir_op_line(func, module, i, op);
        lines.push(format!("{IND_LP}{lpir_line}"));

        if let Some(vinst_list) = vinsts_by_src_op.get(&(i as u32)) {
            for (vinst_idx, inst) in vinst_list.iter() {
                rendered_vinsts.insert(*vinst_idx);
                push_vinst_snapshot_block(
                    &mut lines,
                    *vinst_idx,
                    inst,
                    vinsts,
                    vreg_pool,
                    output,
                    &trace_by_vinst,
                    symbols,
                );
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
            &mut lines,
            inst_idx,
            inst,
            vreg_pool,
            output,
            &trace_by_vinst,
            symbols,
            IND_LP,
            IND_VI,
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
    trace_by_vinst: &alloc::collections::BTreeMap<usize, Vec<&TraceEntry>>,
    symbols: &ModuleSymbols,
) {
    push_vinst_snapshot_block_raw(
        lines,
        vinst_idx,
        inst,
        vreg_pool,
        output,
        trace_by_vinst,
        symbols,
        IND_VI,
        IND_VI,
    );
}

fn push_vinst_snapshot_block_raw(
    lines: &mut Vec<String>,
    vinst_idx: usize,
    inst: &VInst,
    vreg_pool: &[VReg],
    output: &AllocOutput,
    trace_by_vinst: &alloc::collections::BTreeMap<usize, Vec<&TraceEntry>>,
    symbols: &ModuleSymbols,
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

    let after_edits: Vec<_> = output
        .edits
        .iter()
        .filter(|(pt, _)| *pt == EditPoint::After(inst_idx_u16))
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
    lines.push(format!(
        "{indent_vinst}{}",
        format_inst(inst, vreg_pool, Some(symbols))
    ));
    for (vreg, alloc) in def_vregs.iter().zip(def_allocs.iter()) {
        lines.push(format!(
            "{indent_vinst}; write: i{} -> {}",
            vreg.0,
            format_alloc(*alloc)
        ));
    }
    if let Some(entries) = trace_by_vinst.get(&vinst_idx) {
        for entry in entries {
            lines.push(format!(
                "{indent_vinst}; trace: {}: {}",
                entry.vinst_mnemonic, entry.decision
            ));
        }
    }
    for edit in &after_edits {
        lines.push(format!("{indent_vinst}; {}", format_edit(edit)));
    }
}

/// Render AllocOutput as human-readable text for snapshot tests.
pub fn render_alloc_output(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
    symbols: Option<&ModuleSymbols>,
) -> String {
    let mut lines = Vec::new();

    let trace_by_vinst = trace_by_vinst_or_empty(output);

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
        lines.push(format_inst(inst, vreg_pool, symbols));

        // Render write (def) allocations
        for (_i, (vreg, alloc)) in def_vregs.iter().zip(def_allocs.iter()).enumerate() {
            lines.push(format!("; write: i{} -> {}", vreg.0, format_alloc(*alloc)));
        }

        if let Some(entries) = trace_by_vinst.get(&inst_idx) {
            for entry in entries {
                lines.push(format!(
                    "; trace: {}: {}",
                    entry.vinst_mnemonic, entry.decision
                ));
            }
        }

        let after_edits: Vec<_> = output
            .edits
            .iter()
            .filter(|(pt, _)| *pt == EditPoint::After(inst_idx_u16))
            .map(|(_, edit)| edit)
            .collect();
        for edit in &after_edits {
            lines.push(format!("; {}", format_edit(edit)));
        }
    }

    lines.join("\n")
}

/// Format an instruction as text (matches `debug::vinst` ireg style: `i0`, `i1`, …).
fn format_inst(inst: &VInst, vreg_pool: &[VReg], symbols: Option<&ModuleSymbols>) -> String {
    match inst {
        VInst::IConst32 { dst, val, .. } => {
            format!("i{} = IConst32 {}", dst.0, val)
        }
        VInst::AluRRR {
            op,
            dst,
            src1,
            src2,
            ..
        } => {
            format!("i{} = {} i{}, i{}", dst.0, op.mnemonic(), src1.0, src2.0)
        }
        VInst::AluRRI {
            op, dst, src, imm, ..
        } => {
            format!("i{} = {} i{}, {}", dst.0, op.mnemonic(), src.0, imm)
        }
        VInst::Neg { dst, src, .. } => {
            format!("i{} = Neg i{}", dst.0, src.0)
        }
        VInst::Bnot { dst, src, .. } => {
            format!("i{} = Bnot i{}", dst.0, src.0)
        }
        VInst::Icmp {
            dst,
            lhs,
            rhs,
            cond,
            ..
        } => {
            format!(
                "i{} = Icmp i{}, {} i{}",
                dst.0,
                lhs.0,
                icmp_cond_str(*cond),
                rhs.0
            )
        }
        VInst::IcmpImm {
            dst,
            src,
            imm,
            cond,
            ..
        } => {
            format!(
                "i{} = IcmpImm {}, i{}, {}",
                dst.0,
                icmp_cond_str(*cond),
                src.0,
                imm
            )
        }
        VInst::Select {
            dst,
            cond,
            if_true,
            if_false,
            ..
        } => {
            format!(
                "i{} = Select i{}, i{}, i{}",
                dst.0, cond.0, if_true.0, if_false.0
            )
        }
        VInst::Mov { dst, src, .. } => {
            format!("i{} = Mov i{}", dst.0, src.0)
        }
        VInst::Load32 {
            dst, base, offset, ..
        } => {
            format!("i{} = Load32 i{}{:+}", dst.0, base.0, offset)
        }
        VInst::Store32 {
            src, base, offset, ..
        } => {
            format!("Store32 i{}, i{}{:+}", src.0, base.0, offset)
        }
        VInst::Load8U {
            dst, base, offset, ..
        } => {
            format!("i{} = Load8U i{}{:+}", dst.0, base.0, offset)
        }
        VInst::Load8S {
            dst, base, offset, ..
        } => {
            format!("i{} = Load8S i{}{:+}", dst.0, base.0, offset)
        }
        VInst::Load16U {
            dst, base, offset, ..
        } => {
            format!("i{} = Load16U i{}{:+}", dst.0, base.0, offset)
        }
        VInst::Load16S {
            dst, base, offset, ..
        } => {
            format!("i{} = Load16S i{}{:+}", dst.0, base.0, offset)
        }
        VInst::Store8 {
            src, base, offset, ..
        } => {
            format!("Store8 i{}, i{}{:+}", src.0, base.0, offset)
        }
        VInst::Store16 {
            src, base, offset, ..
        } => {
            format!("Store16 i{}, i{}{:+}", src.0, base.0, offset)
        }
        VInst::SlotAddr { dst, slot, .. } => {
            format!("i{} = SlotAddr {}", dst.0, slot)
        }
        VInst::MemcpyWords {
            dst_base,
            src_base,
            size,
            ..
        } => {
            format!(
                "MemcpyWords i{}, i{}, {} words",
                dst_base.0,
                src_base.0,
                size / 4
            )
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
            format!("Br L{target}")
        }
        VInst::BrIf {
            cond,
            target,
            invert,
            ..
        } => {
            if *invert {
                format!("BrIf !i{}, L{}", cond.0, target)
            } else {
                format!("BrIf i{}, L{}", cond.0, target)
            }
        }
        VInst::Label(id, _) => {
            format!("Label L{id}")
        }
        VInst::Call {
            target,
            args,
            rets,
            callee_uses_sret,
            ..
        } => {
            let name = symbols
                .map(|s| s.name(*target).to_string())
                .unwrap_or_else(|| format!("sym{}", target.0));
            let args_s = args
                .vregs(vreg_pool)
                .iter()
                .map(|v| format!("i{}", v.0))
                .collect::<Vec<_>>()
                .join(", ");
            let rets_v = rets.vregs(vreg_pool);
            let mut s = String::new();
            if !rets_v.is_empty() {
                if rets_v.len() == 1 {
                    s.push_str(&format!("i{} = ", rets_v[0].0));
                } else {
                    s.push('(');
                    for (i, v) in rets_v.iter().enumerate() {
                        if i > 0 {
                            s.push_str(", ");
                        }
                        s.push_str(&format!("i{}", v.0));
                    }
                    s.push_str(") = ");
                }
            }
            s.push_str("Call ");
            s.push_str(&name);
            if *callee_uses_sret {
                s.push_str(" sret");
            }
            s.push_str(" (");
            s.push_str(&args_s);
            s.push(')');
            s
        }
    }
}

fn icmp_cond_str(cond: IcmpCond) -> &'static str {
    match cond {
        IcmpCond::Eq => "==",
        IcmpCond::Ne => "!=",
        IcmpCond::LtS => "<",
        IcmpCond::LeS => "<=",
        IcmpCond::GtS => ">",
        IcmpCond::GeS => ">=",
        IcmpCond::LtU => "<u",
        IcmpCond::LeU => "<=u",
        IcmpCond::GtU => ">u",
        IcmpCond::GeU => ">=u",
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
        Alloc::Stack(slot) => format!("slot{slot}"),
        Alloc::None => String::from("none"),
    }
}

/// Format an edit as a string.
fn format_edit(edit: &Edit) -> String {
    match edit {
        Edit::Move { from, to } => {
            format!("move: {} -> {}", format_alloc(*from), format_alloc(*to))
        }
        Edit::LoadIncomingArg { fp_offset, to } => {
            format!("load_arg: [fp+{}] -> {}", fp_offset, format_alloc(*to))
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
    _func: &IrFunction,
    output: &AllocOutput,
    func_abi: &FuncAbi,
) {
    lines.push(format!("{IND_LP}; spill_slots: {}", output.num_spill_slots));
    // param_locs[0] is vmctx; user params start at index 1
    let user_param_count = func_abi.param_locs().len().saturating_sub(1);
    for i in 0..user_param_count {
        // VRegs for params: v0=vmctx, v1=param0, v2=param1, ...
        let vreg_num = 1 + i;
        if let Some(loc) = func_abi.param_loc(i + 1) {
            // +1 to skip vmctx
            lines.push(format!(
                "{IND_LP}; arg v{}: {}",
                vreg_num,
                format_arg_loc(&loc)
            ));
        }
    }
    lines.push(format!(
        "{IND_LP}; ret: {}",
        format_return_method(func_abi.return_method())
    ));
    append_entry_trace_metadata_lines(lines, IND_LP, output);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::FuncAbi;
    use crate::debug::vinst;
    use crate::regalloc::walk;
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
        let rendered = render_alloc_output(&vinsts, &pool, &output, None);

        // Just check it doesn't panic and contains expected text
        assert!(rendered.contains("IConst32"));
        assert!(rendered.contains("Ret"));
    }
}
