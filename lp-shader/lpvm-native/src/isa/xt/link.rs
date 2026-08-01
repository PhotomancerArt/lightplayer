//! Xtensa-specific linker helpers: call-site patching and ELF metadata.
//!
//! Mirrors [`crate::isa::rv32::link`]. The Xtensa emitter reaches every call
//! target through a literal-pool slot (`l32r` + `callx8` — see
//! [`super::emit`]), so a call relocation names the **4-byte pool slot** that
//! must receive the callee's absolute runtime address — the standard
//! `R_XTENSA_32` absolute-word relocation (numbering per the Xtensa ELF psABI,
//! the same scheme `lp-xt/lp-xt-elf/src/reloc.rs` applies), not a
//! PC-relative instruction patch like rv32's `auipc`+`jalr` pair.

use object::elf;

use crate::compile::NativeReloc;
use crate::error::NativeError;
use alloc::string::String;

/// Standard Xtensa `R_XTENSA_32` relocation type (ELF / JIT): absolute 32-bit
/// word at the relocation offset (`*loc = S + A`; the emitter leaves the slot
/// zeroed, so the addend is 0).
///
/// Reached from outside `isa::` only via [`crate::isa::IsaTarget::call_reloc_type`].
pub(crate) const R_XTENSA_32: u32 = elf::R_XTENSA_32;

/// `e_flags` value for Xtensa ELF objects. The architecture defines only the
/// `EF_XTENSA_MACHINE` field (bits 0–3) with the single value
/// `E_XTENSA_MACH_NONE` = 0; the esp toolchains emit 0 for both LX6 and LX7
/// objects.
pub const EF_XTENSA_NONE: u32 = 0;

/// Patch the literal-pool slot at `code[reloc.offset..]` with `target_addr`
/// (the callee's absolute runtime address), resolving the `l32r`+`callx8`
/// call sequence that reads the slot.
///
/// Unlike rv32's [`crate::isa::rv32::link::patch_call_plt`] this is an
/// absolute patch, so no image base is needed — but the *slot content* is
/// position-dependent: the patch must be (re)applied whenever the callee's
/// load address changes.
pub fn patch_call_literal(
    code: &mut [u8],
    reloc: &NativeReloc,
    target_addr: u32,
) -> Result<(), NativeError> {
    let off = reloc.offset;
    if off % 4 != 0 {
        return Err(NativeError::Internal(alloc::format!(
            "R_XTENSA_32 slot offset {off} not 4-aligned"
        )));
    }
    let slot = code
        .get_mut(off..off.saturating_add(4))
        .ok_or_else(|| NativeError::Internal(String::from("relocation overruns code buffer")))?;
    slot.copy_from_slice(&target_addr.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use super::*;

    fn reloc(offset: usize) -> NativeReloc {
        NativeReloc {
            offset,
            symbol: String::from("callee"),
            r_type: R_XTENSA_32,
        }
    }

    #[test]
    fn patches_slot_little_endian() {
        let mut code = vec![0u8; 12];
        patch_call_literal(&mut code, &reloc(4), 0x4037_1234).unwrap();
        assert_eq!(&code[4..8], &0x4037_1234u32.to_le_bytes());
        assert_eq!(&code[0..4], &[0; 4], "bytes before the slot untouched");
        assert_eq!(&code[8..12], &[0; 4], "bytes after the slot untouched");
    }

    #[test]
    fn rejects_out_of_bounds_slot() {
        let mut code = vec![0u8; 8];
        assert!(patch_call_literal(&mut code, &reloc(8), 0x4000_0000).is_err());
    }

    #[test]
    fn rejects_misaligned_slot() {
        let mut code = vec![0u8; 8];
        assert!(patch_call_literal(&mut code, &reloc(2), 0x4000_0000).is_err());
    }

    #[test]
    fn reloc_type_matches_the_psabi_number() {
        assert_eq!(R_XTENSA_32, 1);
    }
}
