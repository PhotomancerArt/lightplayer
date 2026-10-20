//! `EXTMEM` — the cache controller: [`accept::extmem`]'s register file
//! (every `done` bit already set, every self-clearing enable already
//! clear), plus the one operation whose effect a guest can see — a **range
//! invalidate**.
//!
//! The mask ROM's `Cache_Invalidate_Addr` (`0x4002_786c`) ends in
//! `Cache_Invalidate_ICache_Items` (`0x4002_76bc`), which is the whole
//! hardware protocol:
//!
//! ```text
//! sw   a0, 160(a5)        ; +0x0a0 cache_sync_addr = vaddr
//! ...  a1 & 0x00ff_ffff   ; +0x0a4 cache_sync_size (low 24 bits)
//! ori  a4, a4, 1          ; +0x098 cache_sync_ctrl |= invalidate_ena
//! loop: lw +0x098 ; andi 16 ; beqz loop   ; until cache_sync_done
//! ```
//!
//! A real cache then serves those lines from flash on their next read.
//! This model's window is a page fill ([`crate::cache`]), refilled at a
//! slice boundary after a flash write — so a guest that programs flash
//! under a mapped page and reads it back in the same slice would read the
//! page as it was. With this, its invalidate marks the pages it names for a
//! refill and ends the slice, and the read after it sees the flash: what
//! `fw-esp32c6`'s tree store relies on when it reads `lpfs` through the
//! cache (`tree_flash.rs`; it invalidates after every program and erase).
//! A guest that forgets the invalidate still reads the old bytes until the
//! slice ends, as silicon would until the line is evicted.

use lp_emu_esp_common::regfile::merge_lane;
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use std::sync::Arc;

use super::accept;
use crate::cache::CacheHandle;
use crate::flash_timing::FlashClocks;
use crate::regs;

const SYNC_CTRL: u32 = 0x098;
const SYNC_ADDR: u32 = 0x0a0;
const SYNC_SIZE: u32 = 0x0a4;
/// `cache_sync_ctrl`'s `cache_invalidate_ena`.
const INVALIDATE_ENA: u32 = 1;

/// The cache controller, with the MMU whose pages an invalidate names.
pub struct Extmem {
    regs: RegFile,
    mmu: CacheHandle,
    /// `t4`: where a range invalidate is posted for the cache's cost model,
    /// so the lines it names are filled again ([`crate::flash_timing`]).
    /// `None` at every other grade, whose cost model never forgets a line.
    clocks: Option<Arc<FlashClocks>>,
}

impl core::fmt::Debug for Extmem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Extmem").finish_non_exhaustive()
    }
}

impl Extmem {
    pub fn new(mmu: CacheHandle) -> Self {
        Self {
            regs: accept::extmem(),
            mmu,
            clocks: None,
        }
    }

    /// Post every range invalidate to `clocks` from now on (`t4`).
    pub fn set_flash_clocks(&mut self, clocks: Arc<FlashClocks>) {
        self.clocks = Some(clocks);
    }
}

impl Peripheral for Extmem {
    fn name(&self) -> &'static str {
        "EXTMEM"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = merge_lane(self.regs.stored(off & !3), off, width, value);
        self.regs.write(off, width, value, cx);
        if off & !3 == SYNC_CTRL && word & INVALIDATE_ENA != 0 {
            let addr = self.regs.stored(SYNC_ADDR);
            let size = self.regs.stored(SYNC_SIZE) & 0x00ff_ffff;
            self.mmu.lock().unwrap().invalidate_vaddr_range(addr, size);
            if let Some(clocks) = &self.clocks {
                clocks.post_invalidate(addr, size);
            }
            // Only the machine can refill the window: end the slice so the
            // read after the invalidate sees the flash (as `Spi0` does for
            // an MMU entry write).
            cx.yield_to_machine();
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::EXTMEM.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        self.regs.save_state()
    }

    fn load_state(&mut self, bytes: &[u8]) {
        self.regs.load_state(bytes);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use lp_emu_esp_common::Sandbox;

    use super::*;
    use crate::cache::{CacheMmu, VALID};

    #[test]
    fn a_range_invalidate_marks_its_pages_and_ends_the_slice() {
        let mmu = Arc::new(Mutex::new(CacheMmu::new()));
        let mut ext = Extmem::new(mmu.clone());
        let mut sb = Sandbox::new();
        {
            let mut m = mmu.lock().unwrap();
            m.set_page_mode(1); // 32 KiB pages
            for (i, page) in [(234u32, 106u32), (235, 107), (236, 108)] {
                m.set_entry(i, page | VALID);
            }
            m.take_dirty();
        }
        // `Cache_Invalidate_ICache_Items`: addr, size, then the enable.
        sb.write(&mut ext, SYNC_ADDR, 0x4275_7fe0);
        sb.write(&mut ext, SYNC_SIZE, 0x40);
        assert!(mmu.lock().unwrap().take_dirty().is_empty());
        sb.yield_now = false;
        sb.write(&mut ext, SYNC_CTRL, INVALIDATE_ENA);
        assert!(sb.yield_now, "the slice ends so the next read sees flash");
        let mut dirty = mmu.lock().unwrap().take_dirty();
        dirty.sort();
        assert_eq!(dirty, [234, 235]);
        // Done reads as done; the enable as clear.
        assert_eq!(sb.read(&mut ext, SYNC_CTRL) & 0b1_0001, 0b1_0000);
    }
}
