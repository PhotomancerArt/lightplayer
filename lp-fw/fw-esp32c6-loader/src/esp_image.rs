//! Loading one ESP application image the way the IDF bootloader does, minus
//! its verification: the core's integrity is the trial mechanism's job (a
//! core that does not come up never confirms, and the next boot rolls back).
//!
//! Format: a 24-byte header (`0xE9`, segment count at byte 1, entry point at
//! bytes 4..8), then per segment an 8-byte header (load address, length) and
//! its bytes.

use core::ffi::CStr;

use crate::{mmu, rom};

const MAGIC: u8 = 0xE9;
const HEADER_LEN: u32 = 24;
const MAX_SEGMENTS: u8 = 16;

/// Map and load the image at flash `at`; its entry point.
pub fn load(at: u32, page_shift: u32, loader_ram: &core::ops::Range<u32>) -> Result<u32, &'static CStr> {
    let mut header = [0u32; (HEADER_LEN / 4) as usize];
    if !rom::flash_read(at, &mut header) {
        return Err(c"flash read failed");
    }
    let bytes = header[0].to_le_bytes();
    let segments = bytes[1];
    if bytes[0] != MAGIC || segments == 0 || segments > MAX_SEGMENTS {
        return Err(c"no image there");
    }
    let entry = header[1];
    let page = 1u32 << page_shift;

    let mut off = at + HEADER_LEN;
    for _ in 0..segments {
        let mut seg = [0u32; 2];
        if !rom::flash_read(off, &mut seg) {
            return Err(c"flash read failed");
        }
        let [addr, len] = seg;
        let data = off + 8;
        if mmu::WINDOW.contains(&addr) {
            // A flash segment: map every page it touches. The image was laid
            // out so that a segment's bytes sit at the same offset within a
            // page as its link address.
            if data % page != addr % page {
                return Err(c"segment not page-congruent");
            }
            let mut v = addr - addr % page;
            let mut p = data - data % page;
            while v < addr + len {
                mmu::map(v, p, page_shift);
                v += page;
                p += page;
            }
        } else {
            // A RAM segment: straight from flash into place.
            if addr < loader_ram.end && addr + len > loader_ram.start {
                return Err(c"segment overlaps the loader");
            }
            if !rom::flash_read_raw(data, addr as *mut u32, len) {
                return Err(c"flash read failed");
            }
        }
        off = data + len;
    }
    Ok(entry)
}
