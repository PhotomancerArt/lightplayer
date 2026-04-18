//! RV32-specific linker helpers: call-site patching and ELF metadata.

use object::elf;

use crate::compile::NativeReloc;
use crate::error::NativeError;
use alloc::string::String;

/// Standard RISC-V `R_RISCV_CALL_PLT` relocation type (ELF / JIT).
pub const R_RISCV_CALL_PLT: u32 = 17;

/// `e_flags` value for the soft-float ABI used by ESP32-C6.
pub const EF_RISCV_FLOAT_ABI_SOFT: u32 = elf::EF_RISCV_FLOAT_ABI_SOFT;

/// Patch an RV32 `auipc + jalr` call sequence at `code[reloc.offset..]` so the
/// call resolves to `target_addr` (absolute runtime address).
pub fn patch_call_plt(
    code: &mut [u8],
    reloc: &NativeReloc,
    image_base: usize,
    target_addr: u32,
) -> Result<(), NativeError> {
    let off = reloc.offset;
    if off.saturating_add(8) > code.len() {
        return Err(NativeError::Internal(String::from(
            "relocation overruns code buffer",
        )));
    }

    let pc = image_base.wrapping_add(off) as u32;

    let auipc_word = u32::from_le_bytes(
        code[off..off + 4]
            .try_into()
            .map_err(|_| NativeError::Internal(String::from("auipc read")))?,
    );
    let jalr_word = u32::from_le_bytes(
        code[off + 4..off + 8]
            .try_into()
            .map_err(|_| NativeError::Internal(String::from("jalr read")))?,
    );

    // Verify auipc+jalr encoding
    if (auipc_word & 0x7f) != 0x17 || (jalr_word & 0x7f) != 0x67 {
        return Err(NativeError::Internal(alloc::format!(
            "expected auipc+jalr at offset {off}, got 0x{auipc_word:08x} 0x{jalr_word:08x}"
        )));
    }

    let pcrel = target_addr.wrapping_sub(pc);
    let new_hi20 = ((pcrel >> 12).wrapping_add(u32::from((pcrel & 0x800) != 0))) & 0xFFFFF;
    let new_lo12 = pcrel & 0xFFF;

    let new_auipc = (auipc_word & 0xFFF) | (new_hi20 << 12);
    let new_jalr = (jalr_word & 0xFFFFF) | (new_lo12 << 20);

    code[off..off + 4].copy_from_slice(&new_auipc.to_le_bytes());
    code[off + 4..off + 8].copy_from_slice(&new_jalr.to_le_bytes());

    Ok(())
}
