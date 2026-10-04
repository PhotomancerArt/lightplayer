//! Cutting the pass-2 ELF into its two flashed pieces, with no binutils.
//!
//! - `engine.bin` is the bytes of `.engine_rodata` and `.engine_text` as
//!   they lie from the engine's base: the first section's address to the
//!   last one's end, any gap between them zero.
//! - The core is an ESP application image of the same ELF **without** the
//!   engine's sections, made by espflash's own image builder (the pinned
//!   3.3). The engine's two section headers are re-typed `SHT_NOBITS` in a
//!   copy of the ELF, so the builder, which takes its segments from
//!   `PROGBITS` sections, never sees them; nothing else about the ELF
//!   changes.

use anyhow::{Context, Result, bail};
use espflash::elf::ElfFirmwareImage;
use espflash::flasher::{FlashData, FlashSettings, FlashSize};
use espflash::targets::{Chip, XtalFrequency};
use object::Endianness;
use object::read::elf::{FileHeader, SectionHeader};

/// The engine's output sections, in link order.
pub const ENGINE_SECTIONS: [&str; 2] = [".engine_rodata", ".engine_text"];

/// One section's place in an ELF32 file.
#[derive(Clone, Debug)]
pub struct SectionSpan {
    pub name: String,
    /// Index into the section header table.
    pub index: usize,
    pub addr: u32,
    pub file_offset: u32,
    pub size: u32,
}

/// The named sections of an ELF32 file, by name.
pub fn section_spans(elf: &[u8], names: &[&str]) -> Result<Vec<SectionSpan>> {
    let header = object::elf::FileHeader32::<Endianness>::parse(elf)?;
    let endian = header.endian()?;
    let sections = header.sections(endian, elf)?;
    let mut spans = Vec::new();
    for (index, s) in sections.iter().enumerate() {
        let name = String::from_utf8_lossy(sections.section_name(endian, s)?).into_owned();
        if names.contains(&name.as_str()) {
            spans.push(SectionSpan {
                name,
                index,
                addr: s.sh_addr(endian),
                file_offset: s.sh_offset(endian),
                size: s.sh_size(endian),
            });
        }
    }
    Ok(spans)
}

/// `engine.bin`: the engine's sections laid out from the lowest address.
pub fn engine_bin(elf: &[u8]) -> Result<(u32, Vec<u8>)> {
    let mut spans = section_spans(elf, &ENGINE_SECTIONS)?;
    if spans.len() != ENGINE_SECTIONS.len() {
        bail!("the ELF has no engine sections (was it linked with the generated engine.x?)");
    }
    spans.sort_by_key(|s| s.addr);
    let base = spans[0].addr;
    let end = spans.iter().map(|s| s.addr + s.size).max().unwrap_or(base);
    let mut out = vec![0u8; (end - base) as usize];
    for s in &spans {
        let src = elf
            .get(s.file_offset as usize..(s.file_offset + s.size) as usize)
            .with_context(|| format!("{} lies outside the file", s.name))?;
        let at = (s.addr - base) as usize;
        out[at..at + src.len()].copy_from_slice(src);
    }
    Ok((base, out))
}

/// A copy of `elf` whose engine sections are `SHT_NOBITS`: what the core's
/// image is made from.
pub fn core_elf(elf: &[u8]) -> Result<Vec<u8>> {
    let header = object::elf::FileHeader32::<Endianness>::parse(elf)?;
    let endian = header.endian()?;
    if endian != Endianness::Little {
        bail!("expected a little-endian ELF");
    }
    let shoff = header.e_shoff(endian) as usize;
    let shentsize = header.e_shentsize(endian) as usize;
    let mut out = elf.to_vec();
    for span in section_spans(elf, &ENGINE_SECTIONS)? {
        // sh_type is the second word of an ELF32 section header.
        let at = shoff + span.index * shentsize + 4;
        out[at..at + 4].copy_from_slice(&object::elf::SHT_NOBITS.to_le_bytes());
    }
    Ok(out)
}

/// The ESP application image espflash makes of `elf` for the C6, as
/// `espflash save-image --chip esp32c6 --flash-size 4mb` writes it.
pub fn app_image(elf: &[u8]) -> Result<Vec<u8>> {
    let image = ElfFirmwareImage::try_from(elf).map_err(|e| anyhow::anyhow!("{e}"))?;
    let flash_data = FlashData::new(
        None,
        None,
        None,
        None,
        FlashSettings::new(None, Some(FlashSize::_4Mb), None),
        0,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let built = Chip::Esp32c6
        .into_target()
        .get_flash_image(&image, flash_data, None, XtalFrequency::_40Mhz)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let parts: Vec<_> = built.ota_segments().collect();
    match parts.as_slice() {
        [single] => Ok(single.data.to_vec()),
        _ => bail!("espflash made {} app segments, expected one", parts.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ELF32 with a section-name table and the named PROGBITS
    /// sections at `(name, addr, bytes)`.
    fn tiny_elf(sections: &[(&str, u32, &[u8])]) -> Vec<u8> {
        let mut shstr = vec![0u8];
        let mut name_off = Vec::new();
        for (name, _, _) in sections {
            name_off.push(shstr.len() as u32);
            shstr.extend_from_slice(name.as_bytes());
            shstr.push(0);
        }
        let shstr_name = shstr.len() as u32;
        shstr.extend_from_slice(b".shstrtab\0");
        let mut body = Vec::new();
        let mut offsets = Vec::new();
        for (_, _, bytes) in sections {
            offsets.push(52 + body.len() as u32);
            body.extend_from_slice(bytes);
        }
        let shstr_off = 52 + body.len() as u32;
        body.extend_from_slice(&shstr);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        let shoff = 52 + body.len() as u32;
        let shnum = sections.len() as u16 + 2;
        let mut e = Vec::new();
        e.extend_from_slice(b"\x7fELF\x01\x01\x01\0\0\0\0\0\0\0\0\0");
        for v in [2u16, 0xF3] {
            e.extend_from_slice(&v.to_le_bytes());
        }
        for v in [1u32, 0, 0, shoff, 0] {
            e.extend_from_slice(&v.to_le_bytes());
        }
        for v in [52u16, 32, 0, 40, shnum, shnum - 1] {
            e.extend_from_slice(&v.to_le_bytes());
        }
        e.extend_from_slice(&body);
        let sh = |e: &mut Vec<u8>, w: [u32; 10]| {
            for v in w {
                e.extend_from_slice(&v.to_le_bytes());
            }
        };
        sh(&mut e, [0; 10]);
        for (k, (_, addr, bytes)) in sections.iter().enumerate() {
            sh(
                &mut e,
                [
                    name_off[k],
                    1,
                    2 | 4,
                    *addr,
                    offsets[k],
                    bytes.len() as u32,
                    0,
                    0,
                    4,
                    0,
                ],
            );
        }
        sh(
            &mut e,
            [
                shstr_name,
                3,
                0,
                0,
                shstr_off,
                shstr.len() as u32,
                0,
                0,
                1,
                0,
            ],
        );
        e
    }

    #[test]
    fn engine_bin_is_the_two_sections_from_the_base_with_the_gap_zeroed() {
        let elf = tiny_elf(&[
            (".text", 0x4200_0000, &[9; 8]),
            (".engine_text", 0x4240_0010, &[2; 4]),
            (".engine_rodata", 0x4240_0000, &[1; 8]),
        ]);
        let (base, bin) = engine_bin(&elf).unwrap();
        assert_eq!(base, 0x4240_0000);
        assert_eq!(
            bin,
            [1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 2]
        );
    }

    #[test]
    fn the_core_elf_hides_only_the_engine_sections() {
        let elf = tiny_elf(&[
            (".text", 0x4200_0000, &[9; 8]),
            (".engine_rodata", 0x4240_0000, &[1; 8]),
            (".engine_text", 0x4240_0010, &[2; 4]),
        ]);
        let core = core_elf(&elf).unwrap();
        let header = object::elf::FileHeader32::<Endianness>::parse(&*core).unwrap();
        let sections = header.sections(Endianness::Little, &*core).unwrap();
        let types: Vec<(String, u32)> = sections
            .iter()
            .map(|s| {
                (
                    String::from_utf8_lossy(sections.section_name(Endianness::Little, s).unwrap())
                        .into_owned(),
                    s.sh_type(Endianness::Little),
                )
            })
            .collect();
        assert!(types.contains(&(".text".into(), object::elf::SHT_PROGBITS)));
        assert!(types.contains(&(".engine_rodata".into(), object::elf::SHT_NOBITS)));
        assert!(types.contains(&(".engine_text".into(), object::elf::SHT_NOBITS)));
        assert_eq!(core.len(), elf.len());
    }
}
