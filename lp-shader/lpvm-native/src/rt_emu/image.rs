//! [`GuestImage`] — the ISA-neutral loaded image `rt_emu` executes.
//!
//! `rt_emu` needs a handful of things from a linked image: the code bytes, the
//! RAM bytes, any further preloaded regions, a name→address symbol map to
//! resolve entry points with, and where the code ends (for `code_size_bytes`).
//! Nothing about those is ISA-specific, but the type that used to carry them —
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

/// One preloaded region of a guest image beyond `code`/`ram`: `bytes` are
/// written at `base`, which must be a region base in the emulator's map.
#[derive(Clone, Debug)]
pub struct ImageRegion {
    /// Guest address of `bytes[0]`.
    pub base: u32,
    /// Bytes to place. May be shorter than the region — the rest stays zero.
    pub bytes: Vec<u8>,
}

/// A linked guest image ready to execute: code, RAM, symbols.
///
/// Addresses are guest addresses in whatever map the target ISA's emulator
/// uses; this type carries no opinion about the layout.
#[derive(Clone, Debug, Default)]
pub struct GuestImage {
    /// Code region bytes: `code[0]` lives at [`code_base`](Self::code_base).
    pub code: Vec<u8>,
    /// Guest address of `code[0]`. Zero on rv32, whose code region starts at
    /// address 0; the Xtensa code region sits in SRAM1 and holds **only** the
    /// compiled shader — the builtins are in [`regions`](Self::regions).
    pub code_base: u32,
    /// RAM region bytes, starting at the RAM base address. Empty on Xtensa.
    pub ram: Vec<u8>,
    /// Further regions to preload before entry, each at its own base.
    ///
    /// Empty on rv32, which links one executable. Xtensa loads a
    /// **flash-resident** builtins image alongside the shader — its `.text` in
    /// the IROM window, its `.rodata` in DROM, its `.data`/`.bss` in internal
    /// SRAM — which is three regions the shader's code region is not.
    pub regions: Vec<ImageRegion>,
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
            regions: Vec::new(),
            symbol_map: load.symbol_map.into_iter().collect(),
            code_end: load.code_end,
        }
    }
}
