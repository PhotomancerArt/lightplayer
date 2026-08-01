//! Structured debug sections for [`crate::compile::compile_function`] (`feature = "debug"`).

use alloc::string::String;
use lp_collection::VecMap;

use lpir::{IrFunction, LpirModule};

use crate::abi::FuncAbi;
use crate::lower::LoweredFunction;
use crate::regalloc::AllocOutput;
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
) -> VecMap<String, String> {
    #[cfg(feature = "debug")]
    {
        let mut sections = VecMap::new();

        let interleaved = crate::regalloc::render::render_interleaved(
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
        // Advance by the decoded instruction length, not a fixed stride: RV32 is
        // uniformly 4 bytes but Xtensa mixes 24-bit and narrow 16-bit encodings.
        while let Some((text, len)) = func_abi.isa().format_instruction_at(&code[off..]) {
            // Encoded bytes as a little-endian word, sized by `len` so a narrow
            // (sub-4-byte) encoding never reads past its own instruction.
            let mut bytes = [0u8; 4];
            bytes[..len].copy_from_slice(&code[off..off + len]);
            let w = u32::from_le_bytes(bytes);
            disasm.push_str(&alloc::format!("{off:04x}\t{w:08x}\t{text}\n"));
            off += len;
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
        VecMap::new()
    }
}
