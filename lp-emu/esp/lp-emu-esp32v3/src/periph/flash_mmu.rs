//! The flash MMU page tables at `0x3FF1_0000` (PRO) and `0x3FF1_2000` (APP),
//! as a peripheral.
//!
//! **These are not registers.** They are two raw 2048-entry `u32` arrays
//! inside the DPORT window but past the end of what svd2rust generates, so
//! `pac-regnames.py` cannot name them and there is nothing to seed from. The
//! ROM writes them with plain `s32i` stores through `cache_flash_mmu_set`;
//! [`crate::cache`] carries the disassembly that says what an entry means.
//!
//! They are a *peripheral* rather than a RAM region for one reason: an entry
//! write changes what an address in the flash window **means**, and the fill
//! that has to follow is the machine's (P7). A region would let the store
//! land silently in the arena with nothing watching. So this view holds no
//! bytes of its own — it is the guest's door onto [`crate::cache::FlashMmu`],
//! which the machine also holds — and every store asks the machine for a
//! slice boundary, exactly as the C6's `mmu_item_content` does.
//!
//! The **fill** is P7's. In P4 a table write is remembered, translated by
//! [`crate::cache::FlashMmu::translate`], and nothing else: there is no flash
//! chip to copy from yet.

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, Width};

use crate::cache::{CORES, CacheHandle, MMU_ENTRIES, MMU_TABLE_LEN};

/// Both cores' tables, back to back: `0x3FF1_0000..0x3FF1_4000`.
pub const LEN: u32 = MMU_TABLE_LEN * CORES as u32;

/// The two flash MMU page tables. Stateless — the tables live in
/// [`crate::cache::ClassicCache`].
pub struct FlashMmuView {
    cache: CacheHandle,
}

impl FlashMmuView {
    pub fn new(cache: CacheHandle) -> Self {
        Self { cache }
    }

    /// An offset in this window → `(core, entry index)`.
    const fn slot(off: u32) -> (usize, usize) {
        let word = (off / 4) as usize;
        (word / MMU_ENTRIES, word % MMU_ENTRIES)
    }
}

impl Peripheral for FlashMmuView {
    fn name(&self) -> &'static str {
        "FLASH_MMU"
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        let (core, index) = Self::slot(off & !3);
        let word = self.cache.lock().expect("cache poisoned").mmu.entry(core, index);
        lane_of(word, off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let (core, index) = Self::slot(off & !3);
        let mut c = self.cache.lock().expect("cache poisoned");
        let old = c.mmu.entry(core, index);
        let next = merge_lane(old, off, width, value);
        if next == old {
            return;
        }
        c.mmu.set_entry(core, index, next);
        drop(c);
        // An entry write changes what an address means. P7's fill runs at the
        // slice boundary; asking for one here is what makes it possible.
        cx.yield_to_machine();
    }

    /// The tables have no register names — they are an array, and inventing
    /// `entry123` would put a name in the trace that no source uses. The
    /// trace prints `FLASH_MMU+0x134`, which is the entry number times four
    /// and is what the ROM's own `addx4` computes.
    fn reg_name(&self, _off: u32) -> Option<&'static str> {
        None
    }

    /// Graded, and graded `Modeled`: the format comes from the ROM's own
    /// disassembly and nothing on silicon has confirmed the model. Answering
    /// `Some` for every offset is the trait's contract for a block that
    /// publishes a table at all.
    fn reg_grade(&self, _off: u32) -> Option<RegGrade> {
        Some(RegGrade::Modeled)
    }

    /// The tables are the cache's, and `DportView` is what serialises them —
    /// one state, one blob, and a restore that loaded it twice would be
    /// loading it from two places.
    fn save_state(&self) -> Vec<u8> {
        Vec::new()
    }

    fn load_state(&mut self, _bytes: &[u8]) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::ClassicCache;
    use crate::memmap;
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn the_window_covers_both_tables_and_nothing_else() {
        assert_eq!(LEN, 0x4000);
        assert_eq!(memmap::FLASH_MMU_PRO + MMU_TABLE_LEN, memmap::FLASH_MMU_APP);
        assert_eq!(FlashMmuView::slot(0), (0, 0));
        assert_eq!(FlashMmuView::slot(77 * 4), (0, 77));
        assert_eq!(FlashMmuView::slot(MMU_TABLE_LEN), (1, 0));
        assert_eq!(FlashMmuView::slot(LEN - 4), (1, MMU_ENTRIES - 1));
    }

    #[test]
    fn an_entry_write_reaches_the_shared_table_and_asks_for_a_slice_boundary() {
        let cache = ClassicCache::handle();
        let mut v = FlashMmuView::new(cache.clone());
        let mut sb = Sandbox::new();

        // What `cache_flash_mmu_set(0, 0, 0x400D_0000, 0x0031_0000, 64, 1)`
        // stores: the physical page number, at entry 77.
        sb.write(&mut v, 77 * 4, 0x31);
        assert_eq!(cache.lock().unwrap().mmu.entry(0, 77), 0x31);
        assert_eq!(sb.read(&mut v, 77 * 4), 0x31);
        assert!(sb.yield_now, "the fill runs at the slice boundary (P7)");
        assert_eq!(
            cache.lock().unwrap().translate(0, memmap::IROM_BASE),
            Some(0x0031_0000)
        );

        // The APP core's table is the second half of the window.
        sb.write(&mut v, MMU_TABLE_LEN + 77 * 4, 0x40);
        assert_eq!(cache.lock().unwrap().mmu.entry(1, 77), 0x40);
        assert_eq!(cache.lock().unwrap().mmu.entry(0, 77), 0x31);

        // `mmu_init`'s memset writes zeros over the whole table.
        sb.write(&mut v, 77 * 4, 0);
        assert_eq!(cache.lock().unwrap().mmu.entry(0, 77), 0);
    }
}
