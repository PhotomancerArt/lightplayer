//! [`GuestImage`] — the ISA-neutral loaded image `rt_emu` executes.
//!
//! `rt_emu` needs exactly four things from a linked image: the code bytes, the
//! RAM bytes, a name→address symbol map to resolve entry points with, and where
//! the code ends (for `code_size_bytes`). Nothing about those four is
//! ISA-specific, but the type that used to carry them —
//! `lp_riscv_elf::ElfLoadInfo` — lives in an rv32 crate, which was the last
//! rv32-shaped thing in the module/instance plumbing.
//!
//! Promoting it here follows the precedent set for `NativeReloc` /
//! `IsaEmitOutput` / `DisasmOptions` in [`crate::isa::shared`]: the neutral
//! shape lives at the seam, and each ISA's loader converts into it.
//! `lp-riscv-elf` keeps producing `ElfLoadInfo` unchanged; [`From`] adapts it.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// A linked guest image ready to execute: code, RAM, symbols.
///
/// Addresses are guest addresses in whatever map the target ISA's emulator
/// uses; this type carries no opinion about the layout.
#[derive(Clone, Debug, Default)]
pub struct GuestImage {
    /// Code region bytes: `code[0]` lives at [`code_base`](Self::code_base).
    pub code: Vec<u8>,
    /// Guest address of `code[0]`. Zero on rv32, whose code region starts at
    /// address 0; the Xtensa code region sits in SRAM1.
    pub code_base: u32,
    /// RAM region bytes, starting at the RAM base address. Empty on Xtensa,
    /// whose data segments live inside the code region.
    pub ram: Vec<u8>,
    /// Symbol name → absolute guest address.
    pub symbol_map: BTreeMap<String, u32>,
    /// Address one past the last code byte, for `code_size_bytes` reporting.
    pub code_end: u32,
}

impl GuestImage {
    /// Resolve a function symbol to its entry address.
    pub fn symbol(&self, name: &str) -> Option<u32> {
        self.symbol_map.get(name).copied()
    }
}

#[cfg(feature = "isa-rv32")]
impl From<lp_riscv_elf::ElfLoadInfo> for GuestImage {
    fn from(load: lp_riscv_elf::ElfLoadInfo) -> Self {
        GuestImage {
            code: load.code,
            // lp-riscv-elf lays the code region out from address 0.
            code_base: 0,
            ram: load.ram,
            symbol_map: load.symbol_map.into_iter().collect(),
            code_end: load.code_end,
        }
    }
}
