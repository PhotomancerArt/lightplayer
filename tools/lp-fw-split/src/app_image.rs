//! `app.bin`: everything the app partition holds, written at its start.
//!
//! The loader at the partition's start, the first boot record (sequence 1,
//! not on trial: a first flash is a proven boot) in record sector 0, record
//! sector 1 **erased**, the core at the region's start and the engine at the
//! first page after the core. Laid out with `lp_bootctl`'s own constants and
//! encoder; nothing here re-types the format.

use anyhow::{Result, bail};
use lp_bootctl::{BOOT_RECORD_SECTORS, BootRecord, LOADER_MAX_LEN, LOADER_OFFSET, REGION_START};

/// The page every extent is aligned to: what espflash 3.3.0's bundled IDF
/// bootloader selects on a 4 MB C6. The loader and the core read the real
/// one from the MMU and refuse a mismatch; the ROM-up emulator gate boots
/// the real bootloader, so a different choice fails CI.
pub const PAGE: u32 = 0x8000;

/// The laid-out partition contents.
pub struct AppImage {
    pub bytes: Vec<u8>,
    pub loader_len: u32,
    pub core_off: u32,
    pub core_len: u32,
    pub engine_off: u32,
    pub engine_len: u32,
    /// Bytes left between the engine's end and the region's end.
    pub room_left: u32,
}

/// Lay out `loader`, `core` and `engine` for a region ending at `region_end`.
/// `build` is the record's `build` (`lp_bootctl::build_hash` of the build id).
pub fn assemble(
    loader: &[u8],
    core: &[u8],
    engine: &[u8],
    region_end: u32,
    build: u32,
) -> Result<AppImage> {
    if loader.len() as u32 > LOADER_MAX_LEN {
        bail!(
            "loader is {} B; the boot records start {LOADER_MAX_LEN} B in",
            loader.len()
        );
    }
    let core_len = core.len() as u32;
    let engine_len = engine.len() as u32;
    let engine_off = (REGION_START + core_len).div_ceil(PAGE) * PAGE;
    if engine_off + engine_len > region_end {
        bail!(
            "core {core_len} B + engine {engine_len} B do not fit the region ({} B at page {PAGE:#x})",
            region_end - REGION_START
        );
    }
    let mut bytes = vec![0xffu8; (engine_off + engine_len - LOADER_OFFSET) as usize];
    let mut put = |at: u32, data: &[u8]| {
        let at = (at - LOADER_OFFSET) as usize;
        bytes[at..at + data.len()].copy_from_slice(data);
    };
    put(LOADER_OFFSET, loader);
    let record = BootRecord {
        seq: 1,
        core_off: REGION_START,
        core_len,
        build,
        trial: false,
    };
    put(BOOT_RECORD_SECTORS[0], &record.encode());
    // Record sector 1 stays erased (0xFF): a flasher writing this image
    // erases whatever newer record a board held there.
    put(REGION_START, core);
    put(engine_off, engine);
    Ok(AppImage {
        bytes,
        loader_len: loader.len() as u32,
        core_off: REGION_START,
        core_len,
        engine_off,
        engine_len,
        room_left: region_end - engine_off - engine_len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_bootctl::{BOOT_RECORD_READ_LEN, BootSlot};

    #[test]
    fn the_layout_comes_from_lp_bootctl() {
        let img = assemble(&[1; 2000], &[2; 40_000], &[3; 1000], 0x35_0000, 0xabcd).unwrap();
        let at = |off: u32| (off - LOADER_OFFSET) as usize;
        assert_eq!(img.bytes[0], 1);
        assert_eq!(img.bytes[at(REGION_START)], 2);
        assert_eq!(img.engine_off, 0x28000, "first page after 0x18000 + 40000");
        assert_eq!(img.bytes[at(img.engine_off)], 3);
        let slot =
            BootSlot::decode(&img.bytes[at(BOOT_RECORD_SECTORS[0])..][..BOOT_RECORD_READ_LEN])
                .unwrap();
        assert_eq!(
            (
                slot.record.seq,
                slot.record.core_off,
                slot.record.core_len,
                slot.record.build
            ),
            (1, REGION_START, 40_000, 0xabcd)
        );
        assert!(!slot.record.trial && !slot.marks.attempted);
        let sector1 = &img.bytes[at(BOOT_RECORD_SECTORS[1])..][..0x1000];
        assert!(
            sector1.iter().all(|b| *b == 0xff),
            "record sector 1 is erased"
        );
        assert_eq!(
            img.bytes.len() as u32,
            img.engine_off + 1000 - LOADER_OFFSET
        );
        assert_eq!(img.room_left, 0x35_0000 - img.engine_off - 1000);
    }

    #[test]
    fn too_much_is_refused() {
        assert!(assemble(&[0; 0x7000], &[0; 1], &[0; 1], 0x35_0000, 0).is_err());
        assert!(assemble(&[0; 1], &[0; 0x20_0000], &[0; 0x20_0000], 0x35_0000, 0).is_err());
    }
}
