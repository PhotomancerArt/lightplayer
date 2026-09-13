//! The flash MMU page table at `0x600C_5000`, as a peripheral.
//!
//! **This is not a register block.** It is a raw 512-entry `u32` array the
//! PAC does not name — `regs::EXTMEM` ends at `date +0x3fc` and nothing in
//! the S3 PAC mentions `0x600C_5000` — written with plain `s32i` stores by
//! `Cache_MMU_Init` and `Cache_Ibus_MMU_Set`/`Cache_Dbus_MMU_Set`;
//! [`crate::cache`] carries the disassembly that says where it is and what
//! an entry means.
//!
//! It is a *peripheral* rather than a RAM region for one reason: an entry
//! write changes what an address in the flash windows **means**, and the
//! fill that has to follow is the machine's. A region would let the store
//! land silently in the arena with nothing watching. So this view holds no
//! bytes of its own — it is the guest's door onto [`crate::cache::FlashMmu`],
//! which the machine also holds — and every store asks the machine for a
//! slice boundary, exactly as the C6's `mmu_item_content` and the classic's
//! `FlashMmuView` do.

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, Width};

use crate::cache::{CacheHandle, MMU_TABLE_LEN};

/// The window: `0x600C_5000..0x600C_5800`.
pub const LEN: u32 = MMU_TABLE_LEN;

/// The flash MMU page table. Stateless — the table lives in
/// [`crate::cache::S3Cache`].
pub struct FlashMmuView {
    cache: CacheHandle,
}

impl FlashMmuView {
    pub fn new(cache: CacheHandle) -> Self {
        Self { cache }
    }
}

impl Peripheral for FlashMmuView {
    fn name(&self) -> &'static str {
        "FLASH_MMU"
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        let index = ((off & !3) / 4) as usize;
        let word = self.cache.lock().expect("cache poisoned").mmu.entry(index);
        lane_of(word, off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let index = ((off & !3) / 4) as usize;
        let mut c = self.cache.lock().expect("cache poisoned");
        let old = c.mmu.entry(index);
        let next = merge_lane(old, off, width, value);
        c.mmu.set_entry(index, next);
        drop(c);
        // An entry write changes what an address means. The fill runs at
        // the slice boundary; asking for one here is what makes it possible.
        cx.yield_to_machine();
    }

    /// The table has no register names — it is an array, and inventing
    /// `entry123` would put a name in the trace that no source uses. The
    /// trace prints `FLASH_MMU+0x14`, which is the entry number times four
    /// and is what the ROM's own `slli a3, a3, 2` computes.
    fn reg_name(&self, _off: u32) -> Option<&'static str> {
        None
    }

    /// Graded `Modeled`: the format comes from the ROM's own disassembly and
    /// nothing on silicon has confirmed the model.
    fn reg_grade(&self, _off: u32) -> Option<RegGrade> {
        Some(RegGrade::Modeled)
    }

    /// The table is the cache's, and the `EXTMEM` view is what serialises
    /// it — one state, one blob, and a restore that loaded it twice would be
    /// loading it from two places.
    fn save_state(&self) -> Vec<u8> {
        Vec::new()
    }

    fn load_state(&mut self, _bytes: &[u8]) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{MMU_INVALID, S3Cache};
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn the_window_covers_the_table_and_nothing_else() {
        assert_eq!(LEN, 0x800);
        assert_eq!(
            crate::memmap::periph::FLASH_MMU,
            crate::cache::MMU_TABLE_BASE
        );
    }

    #[test]
    fn an_entry_write_reaches_the_shared_table_and_asks_for_a_slice_boundary() {
        let cache = S3Cache::handle();
        let mut v = FlashMmuView::new(cache.clone());
        let mut sb = Sandbox::new();

        // A fresh table reads what `Cache_MMU_Init` writes.
        assert_eq!(sb.read(&mut v, 5 * 4), MMU_INVALID);

        // What `Cache_Ibus_MMU_Set(0, 0x4205_0000, 0x0006_0000, 64, 1, 0)`
        // stores: the physical page number, at entry 5.
        sb.write(&mut v, 5 * 4, 6);
        assert_eq!(cache.lock().unwrap().mmu.entry(5), 6);
        assert_eq!(sb.read(&mut v, 5 * 4), 6);
        assert!(sb.yield_now, "the fill runs at the slice boundary");
        assert_eq!(
            cache.lock().unwrap().translate(0x4205_0020),
            Some(0x0006_0020)
        );

        // `Cache_MMU_Init`'s loop writes 0x4000 over the whole table.
        sb.write(&mut v, 5 * 4, MMU_INVALID);
        assert_eq!(cache.lock().unwrap().translate(0x4205_0020), None);
        // The last entry is where the ROM's `0x600c57fc` literal says.
        sb.write(&mut v, LEN - 4, 0x7f);
        assert_eq!(cache.lock().unwrap().mmu.entry(511), 0x7f);
    }
}
