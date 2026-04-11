//! Concatenate emitted functions, record relocations, patch auipc+jalr at finalize.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::abi::ModuleAbi;
use crate::error::NativeError;
use crate::isa::rv32::emit::{NativeReloc, emit_function_bytes};

use super::buffer::JitBuffer;
use super::builtins::BuiltinTable;

/// In-progress JIT image: raw code + relocations before fixup.
pub struct JitEmitContext<'a> {
    builtin_table: &'a BuiltinTable,
    code: Vec<u8>,
    relocs: Vec<NativeReloc>,
    entries: BTreeMap<String, usize>,
}

impl<'a> JitEmitContext<'a> {
    #[must_use]
    pub fn new(builtin_table: &'a BuiltinTable) -> Self {
        Self {
            builtin_table,
            code: Vec::new(),
            relocs: Vec::new(),
            entries: BTreeMap::new(),
        }
    }

    pub fn emit_function(
        &mut self,
        func: &lpir::IrFunction,
        ir: &LpirModule,
        module_abi: &ModuleAbi,
        fn_sig: &lps_shared::LpsFnSig,
        float_mode: lpir::FloatMode,
        alloc_trace: bool,
    ) -> Result<(), NativeError> {
        let base = self.code.len();
        let emitted =
            emit_function_bytes(func, ir, module_abi, fn_sig, float_mode, false, alloc_trace)?;
        for mut r in emitted.relocs {
            r.offset = r.offset.saturating_add(base);
            self.relocs.push(r);
        }
        self.code.extend_from_slice(&emitted.code);
        self.entries.insert(func.name.clone(), base);
        Ok(())
    }

    /// Resolve relocations and produce an executable [`JitBuffer`].
    pub fn finalize(mut self) -> Result<(JitBuffer, BTreeMap<String, usize>), NativeError> {
        let image_base = self.code.as_ptr() as usize;
        for r in &self.relocs {
            let target = self.resolve_symbol(&r.symbol, image_base).ok_or_else(|| {
                NativeError::JitLink(format!(
                    "unresolved symbol `{}` for JIT relocation at offset {}",
                    r.symbol, r.offset
                ))
            })?;
            patch_call_plt(&mut self.code, r.offset, image_base, target)?;
        }
        let entries = self.entries;
        Ok((JitBuffer::from_code(self.code), entries))
    }

    fn resolve_symbol(&self, sym: &str, image_base: usize) -> Option<u32> {
        if let Some(addr) = self.builtin_table.lookup(sym) {
            return Some(addr as u32);
        }
        self.entries
            .get(sym)
            .map(|off| image_base.wrapping_add(*off) as u32)
    }
}

/// RISC-V ELF `R_RISCV_CALL_PLT` style fixup (matches `lp-riscv-elf` `handle_call_plt`).
fn patch_call_plt(
    code: &mut [u8],
    auipc_offset: usize,
    image_base: usize,
    target_addr: u32,
) -> Result<(), NativeError> {
    let off = auipc_offset;
    if off.saturating_add(8) > code.len() {
        return Err(NativeError::JitLink(String::from(
            "relocation overruns code buffer",
        )));
    }
    let pc = image_base.wrapping_add(off) as u32;
    let auipc_word = u32::from_le_bytes(
        code[off..off + 4]
            .try_into()
            .map_err(|_| NativeError::JitLink(String::from("auipc read")))?,
    );
    let jalr_word = u32::from_le_bytes(
        code[off + 4..off + 8]
            .try_into()
            .map_err(|_| NativeError::JitLink(String::from("jalr read")))?,
    );
    if (auipc_word & 0x7f) != 0x17 || (jalr_word & 0x7f) != 0x67 {
        return Err(NativeError::JitLink(format!(
            "expected auipc+jalr at offset {off}, got 0x{auipc_word:08x} 0x{jalr_word:08x}"
        )));
    }
    let pcrel = target_addr.wrapping_sub(pc);
    let new_hi20 = ((pcrel >> 12) + u32::from((pcrel & 0x800) != 0)) & 0xFFFFF;
    let new_lo12 = pcrel & 0xFFF;
    let new_auipc = (auipc_word & 0xFFF) | (new_hi20 << 12);
    let new_jalr = (jalr_word & 0xFFFFF) | (new_lo12 << 20);
    code[off..off + 4].copy_from_slice(&new_auipc.to_le_bytes());
    code[off + 4..off + 8].copy_from_slice(&new_jalr.to_le_bytes());
    Ok(())
}

/// Emit and link a full module.
pub fn compile_module_jit(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    builtin_table: &BuiltinTable,
    float_mode: lpir::FloatMode,
    alloc_trace: bool,
) -> Result<(JitBuffer, BTreeMap<String, usize>), NativeError> {
    if ir.functions.is_empty() {
        return Err(NativeError::EmptyModule);
    }
    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);
    let sig_map: BTreeMap<&str, &lps_shared::LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut ctx = JitEmitContext::new(builtin_table);
    for func in &ir.functions {
        let default_sig = lps_shared::LpsFnSig {
            name: func.name.clone(),
            return_type: lps_shared::LpsType::Void,
            parameters: Vec::new(),
        };
        let fn_sig = sig_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or(&default_sig);
        ctx.emit_function(func, ir, &module_abi, fn_sig, float_mode, alloc_trace)?;
    }
    ctx.finalize()
}
