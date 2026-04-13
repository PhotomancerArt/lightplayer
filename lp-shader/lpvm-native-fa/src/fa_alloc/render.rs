//! Human-readable rendering of AllocOutput for snapshot tests.

use crate::fa_alloc::{Alloc, AllocOutput, Edit, EditPoint};
use crate::vinst::{VInst, VReg};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

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

        // Format the instruction text
        let inst_text = format_inst(inst, vreg_pool);

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
        lines.push(inst_text);

        // Render write (def) allocations
        for (_i, (vreg, alloc)) in def_vregs.iter().zip(def_allocs.iter()).enumerate() {
            lines.push(format!("; write: i{} -> {}", vreg.0, format_alloc(*alloc)));
        }
    }

    lines.join("\n")
}

/// Format an instruction as text.
fn format_inst(inst: &VInst, _vreg_pool: &[VReg]) -> String {
    // For now, use a simple mnemonic-based format
    match inst {
        VInst::IConst32 { dst, val, .. } => {
            format!("i{} = IConst32 {}", dst.0, val)
        }
        VInst::Add32 {
            dst, src1, src2, ..
        } => {
            format!("i{} = Add32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::Ret { vals, .. } => {
            if vals.len() == 0 {
                "Ret".to_string()
            } else {
                let mut s = String::from("Ret ");
                s.push_str("...");
                s
            }
        }
        _ => {
            format!("; {} (unformatted)", inst.mnemonic())
        }
    }
}

/// Format an allocation as a string.
fn format_alloc(alloc: Alloc) -> &'static str {
    match alloc {
        Alloc::Reg(preg) => {
            // Map PReg to ABI name
            match preg {
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
            }
        }
        Alloc::Stack(_slot) => "slot",
        Alloc::None => "none",
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
