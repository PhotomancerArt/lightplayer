//! Structured debug sections for [`crate::compile::compile_function`] (`feature = "debug"`).

use alloc::collections::BTreeMap;
use alloc::string::String;

use lpir::{IrFunction, LpirModule};

use crate::abi::FuncAbi;
use crate::fa_alloc::AllocOutput;
use crate::lower::LoweredFunction;
use crate::vinst::ModuleSymbols;

/// Build `FunctionDebugInfo` section map: interleaved LPIR/VInst/alloc, disasm, VInst listing.
pub fn build_debug_sections(
    func: &IrFunction,
    ir: &LpirModule,
    lowered: &LoweredFunction,
    code: &[u8],
    alloc_output: &AllocOutput,
    func_abi: &FuncAbi,
    symbols: &ModuleSymbols,
) -> BTreeMap<String, String> {
    #[cfg(feature = "debug")]
    {
        let mut sections = BTreeMap::new();

        let interleaved = crate::fa_alloc::render::render_interleaved(
            func,
            ir,
            &lowered.vinsts,
            &lowered.vreg_pool,
            alloc_output,
            func_abi,
            symbols,
        );
        sections.insert("interleaved".into(), interleaved);

        let mut disasm = String::new();
        let mut off = 0usize;
        while off + 4 <= code.len() {
            let w = u32::from_le_bytes(code[off..off + 4].try_into().expect("4 bytes"));
            disasm.push_str(&alloc::format!(
                "{:04x}\t{:08x}\t{}\n",
                off,
                w,
                lp_riscv_inst::format_instruction(w)
            ));
            off += 4;
        }
        sections.insert("disasm".into(), disasm);

        let mut vinst_text = String::new();
        for inst in &lowered.vinsts {
            vinst_text.push_str(&alloc::format!(
                "{} {}\n",
                inst.mnemonic(),
                inst.format_alloc_trace_detail(&lowered.vreg_pool, symbols)
            ));
        }
        sections.insert("vinst".into(), vinst_text);

        sections
    }
    #[cfg(not(feature = "debug"))]
    {
        let _ = (func, ir, lowered, code, alloc_output, func_abi, symbols);
        BTreeMap::new()
    }
}
