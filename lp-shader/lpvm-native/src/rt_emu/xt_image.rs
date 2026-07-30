//! Building the Xtensa [`GuestImage`]: the builtins base image with compiled
//! shader code placed after it and call relocations resolved.
//!
//! ## Why this shape
//!
//! rv32 links a shader object *into* a builtins executable with
//! `lpvm_cranelift::link_object_with_builtins`. Xtensa takes the route the M3b
//! spike verified as the lower-risk one: load the **already-linked** builtins
//! executable with `lp-xt-elf`'s proven base loader, place the shader's compiled
//! functions after its `.text`, and patch each call relocation against the
//! merged symbol map. That is the flow `lpvm-native/tests/xt_pipeline.rs` proves
//! for shader-to-shader calls, extended with the builtins symbols.
//!
//! `lp_xt_elf::reloc::link_objects` — the relocatable-object linker driver — is
//! deliberately *not* used: its own docs call it a stretch prototype, and it is
//! reserved for the on-device builtins-link path.
//!
//! ## The single-region layout
//!
//! `lps-builtins-xt-app`'s linker script puts both of its segments inside the
//! emulator's 128 KiB code region: `.text` in the low 112 KiB (I-bus
//! `0x4037_8000`) and `.rodata`/`.data`/`.bss` in the top 16 KiB (D-bus
//! `0x3FCA_4000`). So one flat buffer covering the whole region reproduces the
//! image exactly, and a run costs one `load_bytes` — the same shape as rv32's
//! per-call `code`/`ram` clone.
//!
//! The IRAM/DRAM split is **not** hardcoded here. It is derived from the image:
//! the shader-code limit is the lowest data-segment offset, so changing the
//! linker script moves the limit automatically.

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

use super::GuestImage;

/// Build the Xtensa guest image for `compiled` against the builtins base image
/// `builtins_elf`, laid out for `profile`'s code region.
///
/// Returns an image whose `symbol_map` holds **I-bus execute** addresses for
/// every builtin and every compiled function, so `rt_emu` can use a resolved
/// symbol directly as an entry PC.
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

    let region_base = profile.code_dbus_base;
    let region_len = profile.code_region_len;
    let mut code = vec![0u8; region_len];

    // Place every PT_LOAD segment at its own address, tracking where executable
    // bytes end and where data begins.
    let mut text_end = 0usize;
    let mut data_start = region_len;
    for seg in elf
        .segments()
        .map_err(|e| NativeError::Internal(format!("Xtensa builtins segments: {e}")))?
    {
        let (offset, executable) = region_offset_of(seg.vaddr, profile).ok_or_else(|| {
            NativeError::Internal(format!(
                "builtins segment at {:#010x} falls outside the {} code region \
                 {region_base:#010x}..{:#010x}",
                seg.vaddr,
                profile.name,
                region_base + region_len as u32
            ))
        })?;
        let end = offset + seg.memsz as usize;
        if end > region_len {
            return Err(NativeError::Internal(format!(
                "builtins segment at {:#010x} ({} bytes) overruns the code region by {} bytes",
                seg.vaddr,
                seg.memsz,
                end - region_len
            )));
        }
        code[offset..offset + seg.data.len()].copy_from_slice(seg.data);
        // The p_memsz tail (.bss) stays zero — the buffer started zeroed.
        if executable {
            text_end = text_end.max(end);
        } else {
            data_start = data_start.min(offset);
        }
    }

    let alias = profile.alias;
    let ibus_of = |offset: usize| alias.dbus_to_ibus(region_base + offset as u32);

    // Shader code starts 4-aligned after the builtins text and must stay below
    // the image's own data segments.
    let shader_start = text_end.next_multiple_of(4);
    let mut symbol_map: alloc::collections::BTreeMap<String, u32> =
        elf.symbols().into_iter().collect();

    let mut offsets = Vec::with_capacity(compiled.functions.len());
    let mut cursor = shader_start;
    for f in &compiled.functions {
        offsets.push(cursor);
        symbol_map.insert(f.name.to_string(), ibus_of(cursor));
        cursor += f.code.len();
        cursor = cursor.next_multiple_of(4);
    }
    if cursor > data_start {
        return Err(NativeError::Internal(format!(
            "compiled shader code does not fit the Xtensa code region: {} bytes of \
             shader after {} bytes of builtins needs {} bytes, but only {} are free \
             before the image's data segments at region offset {:#x} \
             (see lp-xt/lps-builtins-xt-app/link.ld)",
            cursor - shader_start,
            text_end,
            cursor - shader_start,
            data_start - shader_start,
            data_start
        )));
    }

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
        symbol_map,
        code_end: ibus_of(cursor),
    })
}

/// Offset of `vaddr` within the profile's code region, and whether it was named
/// through the executable (I-bus) view.
///
/// The builtins image addresses `.text` at its I-bus alias and its data segments
/// at their D-bus addresses — both inside the same region — so both views must
/// resolve to the one flat buffer.
fn region_offset_of(vaddr: u32, profile: &BoardProfile) -> Option<(usize, bool)> {
    let base = profile.code_dbus_base;
    let len = profile.code_region_len as u32;
    if vaddr >= base && vaddr < base + len {
        return Some(((vaddr - base) as usize, false));
    }
    let dbus = profile.alias.ibus_to_dbus(vaddr);
    if dbus >= base && dbus < base + len {
        return Some(((dbus - base) as usize, true));
    }
    None
}
