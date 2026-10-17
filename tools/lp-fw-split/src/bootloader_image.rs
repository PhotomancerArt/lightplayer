//! `bootloader.bin`: LightPlayer's own second-stage bootloader as the mask
//! ROM loads it from flash `0x0`, written here byte by byte — no espflash
//! image builder, no bundled binary.
//!
//! The format is the ESP application image the ROM reads (ESP32-C6 TRM,
//! "Boot"; the header layout is the one `lp-emu-esp32c6/src/image.rs` reads
//! as the ROM does):
//!
//! ```text
//!  0  0xE9, segment count, SPI mode (2 = DIO), size/speed (0x20: 4 MB, 40 MHz)
//!  4  entry point
//!  8  WP pin 0xEE, three drive bytes, chip id u16 (0x000D: the C6),
//!     min revision u8, min/max full revision u16 (0, 99), 4 reserved,
//!     hash appended u8 (1)
//! 24  per segment: load address u32, length u32, bytes
//!     zero padding to 15 mod 16, then the checksum byte: XOR of every
//!     segment byte, seeded 0xEF
//!     SHA-256 of everything before it
//! ```
//!
//! The header's flash fields are the ones every C6 image has carried (the
//! silicon transcript's `mode:DIO, clock div:2`). Every segment must be RAM
//! below the ROM's own stack: the ROM loads them before this code runs and
//! maps nothing for it. The whole image must end before the partition table
//! at `0x8000`. What the ROM checks of it was measured on the emulator, which
//! runs the real mask ROM (ADR
//! `docs/adr/2026-10-17-the-c6-boots-through-our-own-bootloader.md`).

use anyhow::{Context, Result, bail};
use object::Endianness;
use object::read::elf::{FileHeader, ProgramHeader};
use sha2::{Digest, Sha256};

/// Where the partition table starts: the image must end before it.
pub const BOOTLOADER_MAX_LEN: usize = 0x8000;
/// RAM the ROM may load into: HP SRAM below its own stack and buffers.
pub const LOADABLE_RAM: std::ops::Range<u32> = 0x4080_0000..0x4086_AD08;

const MAGIC: u8 = 0xE9;
const SPI_MODE_DIO: u8 = 2;
/// High nibble: 4 MB (2); low nibble: 40 MHz (0).
const SIZE_SPEED_4MB_40MHZ: u8 = 0x20;
const CHIP_ID_C6: u16 = 0x000D;
const MAX_FULL_REVISION: u16 = 99;
const CHECKSUM_SEED: u8 = 0xEF;

/// One segment the ROM copies to RAM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RamSegment {
    pub addr: u32,
    pub bytes: Vec<u8>,
}

/// The bootloader image of the bootloader-flavour loader ELF.
pub fn image_from_elf(elf: &[u8]) -> Result<Vec<u8>> {
    let (entry, segments) = ram_segments(elf)?;
    image(entry, &segments)
}

/// The ELF's entry point and its loadable bytes, one segment per `PT_LOAD`
/// with file bytes.
pub fn ram_segments(elf: &[u8]) -> Result<(u32, Vec<RamSegment>)> {
    let header = object::elf::FileHeader32::<Endianness>::parse(elf)?;
    let endian = header.endian()?;
    let mut out = Vec::new();
    for ph in header.program_headers(endian, elf)? {
        if ph.p_type(endian) != object::elf::PT_LOAD || ph.p_filesz(endian) == 0 {
            continue;
        }
        let bytes = ph
            .data(endian, elf)
            .map_err(|_| anyhow::anyhow!("a PT_LOAD segment lies outside the file"))?;
        out.push(RamSegment {
            addr: ph.p_paddr(endian),
            bytes: bytes.to_vec(),
        });
    }
    Ok((header.e_entry(endian), out))
}

/// The image of `segments` starting at `entry`.
pub fn image(entry: u32, segments: &[RamSegment]) -> Result<Vec<u8>> {
    if segments.is_empty() || segments.len() > 16 {
        bail!(
            "a bootloader image needs 1..=16 segments, not {}",
            segments.len()
        );
    }
    let mut out = Vec::with_capacity(BOOTLOADER_MAX_LEN);
    out.extend_from_slice(&[MAGIC, segments.len() as u8, SPI_MODE_DIO, SIZE_SPEED_4MB_40MHZ]);
    out.extend_from_slice(&entry.to_le_bytes());
    out.extend_from_slice(&[0xEE, 0, 0, 0]);
    out.extend_from_slice(&CHIP_ID_C6.to_le_bytes());
    out.push(0); // min revision
    out.extend_from_slice(&0u16.to_le_bytes()); // min full revision
    out.extend_from_slice(&MAX_FULL_REVISION.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.push(1); // SHA-256 appended
    let mut checksum = CHECKSUM_SEED;
    for s in segments {
        let end = s.addr as u64 + s.bytes.len() as u64;
        if s.addr < LOADABLE_RAM.start || end > LOADABLE_RAM.end as u64 {
            bail!(
                "segment {:#010x}+{:#x} is not RAM the ROM loads into ({:#010x}..{:#010x})",
                s.addr,
                s.bytes.len(),
                LOADABLE_RAM.start,
                LOADABLE_RAM.end
            );
        }
        let mut bytes = s.bytes.clone();
        bytes.resize(bytes.len().div_ceil(4) * 4, 0);
        out.extend_from_slice(&s.addr.to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        for b in &bytes {
            checksum ^= b;
        }
        out.extend_from_slice(&bytes);
    }
    while out.len() % 16 != 15 {
        out.push(0);
    }
    out.push(checksum);
    let digest: [u8; 32] = Sha256::digest(&out).into();
    out.extend_from_slice(&digest);
    if out.len() > BOOTLOADER_MAX_LEN {
        bail!(
            "the bootloader image is {} B; it must end before the partition table at {BOOTLOADER_MAX_LEN:#x}",
            out.len()
        );
    }
    Ok(out)
}

/// Check an image this module wrote (or any ESP image) the way the ROM
/// reads it: magic, segments within the bytes, checksum, appended hash.
/// Its segments, for a caller that wants them.
pub fn check(image: &[u8]) -> Result<Vec<(u32, u32)>> {
    if image.len() < 24 || image[0] != MAGIC {
        bail!("not an ESP image");
    }
    let mut off = 24;
    let mut checksum = CHECKSUM_SEED;
    let mut segments = Vec::new();
    for k in 0..image[1] {
        let header = image
            .get(off..off + 8)
            .with_context(|| format!("segment {k}'s header runs past the end"))?;
        let addr = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let len = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let data = image
            .get(off + 8..off + 8 + len as usize)
            .with_context(|| format!("segment {k} runs past the end"))?;
        for b in data {
            checksum ^= b;
        }
        segments.push((addr, len));
        off += 8 + len as usize;
    }
    while off % 16 != 15 {
        off += 1;
    }
    if image.get(off) != Some(&checksum) {
        bail!("checksum mismatch at {off:#x}");
    }
    off += 1;
    if image[23] == 1 {
        let digest: [u8; 32] = Sha256::digest(&image[..off]).into();
        if image.get(off..off + 32) != Some(&digest[..]) {
            bail!("appended SHA-256 mismatch");
        }
    }
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_checks_and_carries_the_c6_header() {
        let img = image(
            0x4085_0008,
            &[RamSegment {
                addr: 0x4085_0000,
                bytes: b"LPLB\x01\0\0\0abcdefg".to_vec(),
            }],
        )
        .unwrap();
        assert_eq!(&img[..4], &[0xE9, 1, 2, 0x20]);
        assert_eq!(u32::from_le_bytes(img[4..8].try_into().unwrap()), 0x4085_0008);
        assert_eq!(&img[12..14], &[0x0D, 0]);
        assert_eq!(img[23], 1);
        // The data starts at 0x20, padded to a word.
        assert_eq!(&img[0x20..0x24], b"LPLB");
        assert_eq!(check(&img).unwrap(), vec![(0x4085_0000, 16)]);
        assert_eq!(
            lp_bootctl::find_bootloader_version(&img),
            1,
            "the identity word is where a reader scans for it"
        );
        assert_eq!((img.len() - 32) % 16, 0);
    }

    #[test]
    fn a_corrupted_byte_fails_the_check() {
        let mut img = image(
            0x4085_0000,
            &[RamSegment {
                addr: 0x4085_0000,
                bytes: vec![1, 2, 3, 4],
            }],
        )
        .unwrap();
        img[33] ^= 1;
        assert!(check(&img).is_err());
    }

    #[test]
    fn flash_window_and_rom_ram_segments_are_refused() {
        for addr in [0x4200_0000, 0x4087_C610] {
            let err = image(
                addr,
                &[RamSegment {
                    addr,
                    bytes: vec![0; 4],
                }],
            )
            .unwrap_err();
            assert!(err.to_string().contains("not RAM"), "{err}");
        }
    }
}
