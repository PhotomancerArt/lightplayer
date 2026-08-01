//! Building the Xtensa [`GuestImage`]: the flash-resident builtins base image,
//! compiled shader code in SRAM, and call relocations resolved between them.
//!
//! ## Why this shape
//!
//! rv32 links a shader object *into* a builtins executable with
//! `lpvm_cranelift::link_object_with_builtins`. Xtensa takes the route the M3b
//! spike verified as the lower-risk one: load the **already-linked** builtins
//! executable with `lp-xt-elf`'s proven base loader, place the shader's compiled
//! functions in the code region, and patch each call relocation against the
//! merged symbol map. That is the flow `lpvm-native/tests/xt_pipeline.rs` proves
//! for shader-to-shader calls, extended with the builtins symbols.
//!
//! `lp_xt_elf::reloc::link_objects` — the relocatable-object linker driver — is
//! deliberately *not* used: its own docs call it a stretch prototype, and it is
//! reserved for the on-device builtins-link path.
//!
//! ## The layout: firmware in flash, JIT output in SRAM
//!
//! This mirrors the device. `lps-builtins-xt-app` is firmware, so it links into
//! the flash windows the cache maps (`.text` at IROM, `.rodata` at DROM,
//! `.data`/`.bss` in internal SRAM); the shader is JIT output, so it gets the
//! **whole** SRAM code region — 128 KiB on the S3, 92 KiB on classic — with
//! nothing resident in front of it.
//!
//! Until 2026-08-01 both lived in the one SRAM code region, shader code
//! starting wherever the image's `.text` happened to end. That coupled the
//! largest compilable shader to the size of the builtins: turning on the f32
//! builtin family left 931 bytes for shader code and took the `xtn.q32`
//! filetest suite from 849/849 files to 522/849. It also made a
//! shader→builtin call look like a short local hop when on silicon it spans
//! SRAM→flash, far outside any direct-call displacement.
//!
//! Nothing here hardcodes the split. Every segment is placed by classifying its
//! `p_vaddr` against [`BoardProfile`], so moving a section in
//! `lps-builtins-xt-app/link.ld` moves it here.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use lp_xt_elf::XtensaElf;
use lp_xt_emu::board::BoardProfile;
use lp_xt_emu::memory::AliasRule;

use crate::compile::CompiledModule;
use crate::error::NativeError;
use crate::isa::IsaTarget;

use super::{GuestImage, ImageRegion};

/// Build the Xtensa guest image for `compiled` against the builtins base image
/// `builtins_elf`, laid out for `profile`'s memory map.
///
/// Returns an image whose `symbol_map` holds **execute** addresses for every
/// builtin (in flash) and every compiled function (in SRAM), so `rt_emu` can
/// use a resolved symbol directly as an entry PC.
pub fn build_xt_image(
    compiled: &CompiledModule,
    builtins_elf: &[u8],
    profile: &BoardProfile,
) -> Result<GuestImage, NativeError> {
    if builtins_elf.is_empty() {
        return Err(NativeError::Internal(String::from(
            "Xtensa builtins image is empty or was not found at build time — \
             run scripts/build-builtins-xt.sh (needs the esp toolchain)",
        )));
    }
    // A contiguous D-bus write is only a contiguous I-bus image under an offset
    // alias. Classic ESP32's SRAM1 alias is word-mirrored, so this flat-buffer
    // layout would need reworking there; fail loudly rather than silently
    // scrambling code. (The filetest path is S3-only by design.)
    if !matches!(profile.alias, AliasRule::Offset(_)) {
        return Err(NativeError::Internal(format!(
            "board profile `{}` does not use an offset I-bus alias; the flat \
             code-region image is offset-alias only",
            profile.name
        )));
    }

    let elf = XtensaElf::parse(builtins_elf)
        .map_err(|e| NativeError::Internal(format!("Xtensa builtins image: {e}")))?;

    // --- the builtins image: one buffer per region it occupies ---
    //
    // Each buffer is trimmed to the bytes actually used, not to the region
    // length: `rt_emu` reloads them into a fresh emulator on every host call,
    // so the copy is on the hot path.
    let mut irom = SegmentBuffer::new(profile.irom_base, profile.irom_len, "IROM (flash .text)");
    let mut drom = SegmentBuffer::new(profile.drom_base, profile.drom_len, "DROM (flash .rodata)");
    let mut data = SegmentBuffer::new(
        profile.image_data_base,
        profile.image_data_len,
        "DRAM (image .data/.bss)",
    );

    for seg in elf
        .segments()
        .map_err(|e| NativeError::Internal(format!("Xtensa builtins segments: {e}")))?
    {
        let target = if let Some(off) = profile.irom_offset(seg.vaddr) {
            (&mut irom, off)
        } else if let Some(off) = profile.drom_offset(seg.vaddr) {
            (&mut drom, off)
        } else if let Some(off) = profile.image_data_offset(seg.vaddr) {
            (&mut data, off)
        } else {
            return Err(NativeError::Internal(format!(
                "builtins segment at {:#010x} ({} bytes) is in none of the {} \
                 image regions: IROM {:#010x}+{:#x}, DROM {:#010x}+{:#x}, \
                 DRAM {:#010x}+{:#x}. The image must link as flash-resident \
                 firmware — see lp-xt/lps-builtins-xt-app/link.ld.",
                seg.vaddr,
                seg.memsz,
                profile.name,
                profile.irom_base,
                profile.irom_len,
                profile.drom_base,
                profile.drom_len,
                profile.image_data_base,
                profile.image_data_len,
            )));
        };
        let (buf, offset) = target;
        buf.place(seg.vaddr, offset, seg.data, seg.memsz as usize)?;
    }

    // --- the shader: the whole SRAM code region, nothing resident in front ---
    let region_base = profile.code_dbus_base;
    let region_len = profile.code_region_len;
    let mut code = vec![0u8; region_len];

    let alias = profile.alias;
    let ibus_of = |offset: usize| alias.dbus_to_ibus(region_base + offset as u32);

    let mut symbol_map: alloc::collections::BTreeMap<String, u32> =
        elf.symbols().into_iter().collect();

    let mut offsets = Vec::with_capacity(compiled.functions.len());
    let mut cursor = 0usize;
    for f in &compiled.functions {
        offsets.push(cursor);
        symbol_map.insert(f.name.to_string(), ibus_of(cursor));
        cursor += f.code.len();
        cursor = cursor.next_multiple_of(4);
    }
    if cursor > region_len {
        return Err(NativeError::Internal(format!(
            "compiled shader code does not fit the Xtensa code region: {} bytes \
             of shader code, but the {} SRAM code region is {} bytes \
             ({:#010x}..{:#010x}). The builtins image is not in this region — \
             it is flash-resident — so this is the shader's own size.",
            cursor,
            profile.name,
            region_len,
            region_base,
            region_base + region_len as u32,
        )));
    }
    code.truncate(cursor);

    // Copy the function bodies in, then resolve every call relocation. Patching
    // happens after placement so a forward call to a later function resolves.
    for (f, &at) in compiled.functions.iter().zip(&offsets) {
        code[at..at + f.code.len()].copy_from_slice(&f.code);
    }
    for (f, &at) in compiled.functions.iter().zip(&offsets) {
        for reloc in &f.relocs {
            if reloc.r_type != IsaTarget::Xtensa.call_reloc_type() {
                return Err(NativeError::Internal(format!(
                    "unexpected Xtensa relocation type {} for symbol `{}`",
                    reloc.r_type, reloc.symbol
                )));
            }
            let target = *symbol_map.get(&reloc.symbol).ok_or_else(|| {
                NativeError::Internal(format!(
                    "unresolved symbol `{}` in shader function `{}` — not a builtin \
                     in the base image nor a function in this module",
                    reloc.symbol, f.name
                ))
            })?;
            let slot = at + reloc.offset;
            crate::isa::xt::link::patch_call_literal(&mut code[at..], reloc, target).map_err(
                |e| NativeError::Internal(format!("relocating `{}`: {e}", reloc.symbol)),
            )?;
            debug_assert_eq!(
                u32::from_le_bytes(code[slot..slot + 4].try_into().unwrap()),
                target
            );
        }
    }

    Ok(GuestImage {
        code,
        code_base: region_base,
        ram: Vec::new(),
        regions: [irom, drom, data]
            .into_iter()
            .filter_map(SegmentBuffer::into_region)
            .collect(),
        symbol_map,
        code_end: ibus_of(cursor),
    })
}

/// Accumulates the PT_LOAD segments that land in one modeled region.
struct SegmentBuffer {
    base: u32,
    len: usize,
    what: &'static str,
    bytes: Vec<u8>,
}

impl SegmentBuffer {
    fn new(base: u32, len: usize, what: &'static str) -> SegmentBuffer {
        SegmentBuffer {
            base,
            len,
            what,
            bytes: Vec::new(),
        }
    }

    /// Place one segment's `data` at `offset`, growing the buffer to cover it.
    /// `memsz` beyond `data` (the `.bss` tail) is left zero — the emulator's
    /// regions start zeroed, so it costs nothing to carry.
    fn place(
        &mut self,
        vaddr: u32,
        offset: usize,
        data: &[u8],
        memsz: usize,
    ) -> Result<(), NativeError> {
        if offset + memsz > self.len {
            return Err(NativeError::Internal(format!(
                "builtins segment at {vaddr:#010x} ({memsz} bytes) overruns the \
                 modeled {} window ({} bytes) by {} bytes",
                self.what,
                self.len,
                offset + memsz - self.len,
            )));
        }
        let end = offset + data.len();
        if self.bytes.len() < end {
            self.bytes.resize(end, 0);
        }
        self.bytes[offset..end].copy_from_slice(data);
        Ok(())
    }

    fn into_region(self) -> Option<ImageRegion> {
        (!self.bytes.is_empty()).then_some(ImageRegion {
            base: self.base,
            bytes: self.bytes,
        })
    }
}
