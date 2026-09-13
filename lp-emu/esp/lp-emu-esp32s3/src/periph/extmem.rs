//! `EXTMEM` at `0x600C_4000` — the cache controller, as a **view** over
//! P04's accept block: every operation-done bit P04 answered is still
//! answered by the same [`RegFile`] ([`super::accept::extmem`]), and the two
//! enable bits now have behaviour behind them.
//!
//! # What this view adds
//!
//! **The two enable bits reach the cache model.** A write to `dcache_ctrl`
//! (`+0x00`) or `icache_ctrl` (`+0x60`) is stored as before and then bit 0
//! is reported to [`crate::cache::S3Cache::note_ctrl`] with the cycle and
//! the pc of the store — the provenance D4's stop message quotes. When the
//! bit *changes* the view asks the machine for a slice boundary, so the
//! window between "the cache went off" and "the watch is armed" is zero
//! instructions — the classic's `DportView` does the same for its
//! `pro_cache_enable`.
//!
//! ⚠️ **`1` is ON.** `icache_ctrl.icache_enable`, "0: disable, 1: enable"
//! (`esp32s3-0.35.2/src/extmem/icache_ctrl.rs`), and the ROM's
//! `Cache_Enable_ICache` `4004f315: or a8, a8, 1`. The C6's `l1_icache_ctrl`
//! bit 0 is a *shut* bit; a view copied from it arms backwards. The test
//! `icache_enable_is_a_set_bit_not_a_shut_bit` in `accept.rs` and
//! `the_enable_bits_reach_the_cache_with_the_s3s_polarity` here hold both
//! directions.
//!
//! # What this view does not change
//!
//! Everything else in the block is P04's accept-and-remember: the sync,
//! preload, lock, autoload and freeze registers with their pulse and mirror
//! shapes (`READ_OVERRIDES` / `READ_MIRRORS` in `accept.rs` list each with
//! its ROM citation), `cache_state` reading idle, and every other register
//! at the PAC's reset. The ROM's freeze/occupy/invalidate sequences complete
//! against this view exactly as they completed against the accept block,
//! which `accept.rs`'s own tests still prove — this file only intercepts the
//! two control words on the way through.
//!
//! **Not here**: the MMU table, which is not in this block at all — it is
//! `0x600C_5000`, [`super::flash_mmu`] — and the fill, which is the
//! machine's ([`crate::cache::fill`]).

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::cache::{CacheHandle, Which};
use crate::periph::accept;
use crate::regs;

/// `dcache_ctrl` — bit 0 is `dcache_enable`.
pub const DCACHE_CTRL: u32 = 0x000;
/// `icache_ctrl` — bit 0 is `icache_enable`.
pub const ICACHE_CTRL: u32 = 0x060;

/// The cache controller.
pub struct Extmem {
    regs: RegFile,
    cache: CacheHandle,
}

impl Extmem {
    /// P04's register file, with the two control words re-graded `Modeled`:
    /// the enable bit has behaviour behind it now, and a grade table that
    /// still called it `Documented` would be describing the accept block.
    pub fn new(cache: CacheHandle) -> Self {
        let regs = accept::extmem()
            .with_grade(DCACHE_CTRL, RegGrade::Modeled)
            .with_grade(ICACHE_CTRL, RegGrade::Modeled);
        // The model's view of the bits is what the file holds: both clear at
        // reset, which is what the PAC says (neither register has a reset).
        {
            let mut c = cache.lock().expect("cache poisoned");
            c.note_ctrl(Which::DCache, regs.stored(DCACHE_CTRL), 0, 0);
            c.note_ctrl(Which::ICache, regs.stored(ICACHE_CTRL), 0, 0);
        }
        Self { regs, cache }
    }

    /// The handle, for the machine.
    pub fn cache(&self) -> &CacheHandle {
        &self.cache
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
        self.regs.write(off, width, value, cx);
        let which = match off & !3 {
            DCACHE_CTRL => Which::DCache,
            ICACHE_CTRL => Which::ICache,
            _ => return,
        };
        let word = self.regs.stored(which.ctrl_offset());
        let changed = self
            .cache
            .lock()
            .expect("cache poisoned")
            .note_ctrl(which, word, cx.now, cx.pc);
        if changed {
            // Whether D4's watch should be armed is the machine's question,
            // asked between slices; answer it before the guest runs on.
            cx.yield_to_machine();
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::EXTMEM.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.regs.reg_grade(off)
    }

    /// The register file, then the cache model's blob — the enable bits'
    /// provenance and the MMU table, which the `FLASH_MMU` view does not
    /// serialise (one state, one blob).
    fn save_state(&self) -> Vec<u8> {
        let regs = self.regs.save_state();
        let cache = self.cache.lock().expect("cache poisoned").save_state();
        let mut out = Vec::with_capacity(4 + regs.len() + cache.len());
        out.extend_from_slice(&(regs.len() as u32).to_le_bytes());
        out.extend_from_slice(&regs);
        out.extend_from_slice(&cache);
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let Some(len) = bytes
            .get(..4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize)
        else {
            log::warn!("EXTMEM::load_state: blob too short, ignored");
            return;
        };
        let Some(regs) = bytes.get(4..4 + len) else {
            log::warn!("EXTMEM::load_state: blob too short, ignored");
            return;
        };
        self.regs.load_state(regs);
        self.cache
            .lock()
            .expect("cache poisoned")
            .load_state(&bytes[4 + len..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CACHE_ENABLE, S3Cache};
    use lp_emu_esp_common::Sandbox;

    /// `Cache_Enable_ICache` / `Cache_Disable_ICache`, register for register,
    /// and what the model sees. Both directions, both caches.
    #[test]
    fn the_enable_bits_reach_the_cache_with_the_s3s_polarity() {
        let cache = S3Cache::handle();
        let mut e = Extmem::new(cache.clone());
        let mut sb = Sandbox::new();
        assert_eq!(e.reg_name(ICACHE_CTRL), Some("icache_ctrl"));
        assert_eq!(e.reg_name(DCACHE_CTRL), Some("dcache_ctrl"));
        assert!(
            !cache.lock().unwrap().enabled(Which::ICache),
            "off at reset"
        );
        assert!(!cache.lock().unwrap().enabled(Which::DCache));

        // `Cache_Enable_ICache` (`4004f308`): +0x60 |= 1.
        let v = sb.read(&mut e, ICACHE_CTRL);
        sb.yield_now = false;
        sb.write(&mut e, ICACHE_CTRL, v | CACHE_ENABLE);
        assert!(cache.lock().unwrap().enabled(Which::ICache), "1 means ON");
        assert!(sb.yield_now, "a change asks for a boundary");
        assert!(!cache.lock().unwrap().enabled(Which::DCache), "its own bit");

        // `Cache_Set_ICache_Mode` read-modify-writes bits 1..3 around it.
        sb.yield_now = false;
        let v = sb.read(&mut e, ICACHE_CTRL);
        sb.write(&mut e, ICACHE_CTRL, v | 0b1110);
        assert!(cache.lock().unwrap().enabled(Which::ICache));
        assert!(!sb.yield_now, "no change, no boundary");

        // `Cache_Disable_ICache` (`4004f2b8`): +0x60 &= ~1, with the store's
        // cycle and pc kept.
        sb.now = 4_242;
        sb.pc = 0x4004_f2cb;
        let v = sb.read(&mut e, ICACHE_CTRL);
        sb.write(&mut e, ICACHE_CTRL, v & !CACHE_ENABLE);
        assert!(!cache.lock().unwrap().enabled(Which::ICache));
        assert_eq!(
            cache.lock().unwrap().disabled(Which::ICache),
            (4_242, Some(0x4004_f2cb))
        );

        // And the DCache pair at +0x00 (`Cache_Enable_DCache` `4004f37c`).
        let v = sb.read(&mut e, DCACHE_CTRL);
        sb.write(&mut e, DCACHE_CTRL, v | CACHE_ENABLE);
        assert!(cache.lock().unwrap().enabled(Which::DCache));
    }

    /// P04's operation-done answers survive the view: the ROM's freeze
    /// enable/disable and the icache invalidate still complete.
    #[test]
    fn p04s_done_bits_still_answer_through_the_view() {
        let mut e = Extmem::new(S3Cache::handle());
        let mut sb = Sandbox::new();
        // `Cache_Freeze_DCache_Enable`/`_Disable`: done mirrors ena.
        let v = sb.read(&mut e, 0x150);
        sb.write(&mut e, 0x150, v | 1);
        assert_ne!(sb.read(&mut e, 0x150) & 4, 0);
        let v = sb.read(&mut e, 0x150);
        sb.write(&mut e, 0x150, v & !1);
        assert_eq!(sb.read(&mut e, 0x150) & 4, 0);
        // `Cache_Invalidate_ICache_Items`: the operation bit is consumed and
        // done reads 1.
        let ctrl = sb.read(&mut e, 0x088);
        sb.write(&mut e, 0x088, ctrl | 1);
        assert_eq!(sb.read(&mut e, 0x088) & 3, 2);
        // `Cache_Disable_ICache`'s last poll: the fsm is idle.
        assert_eq!(sb.read(&mut e, 0x130) & 0xfff, 1);
    }

    #[test]
    fn the_state_round_trips_with_the_table_inside_it() {
        let cache = S3Cache::handle();
        let mut e = Extmem::new(cache.clone());
        let mut sb = Sandbox::new();
        sb.write(&mut e, ICACHE_CTRL, CACHE_ENABLE | 0b1110);
        cache.lock().unwrap().mmu.set_entry(5, 6);
        let blob = e.save_state();

        let other = S3Cache::handle();
        let mut back = Extmem::new(other.clone());
        back.load_state(&blob);
        assert_eq!(sb.read(&mut back, ICACHE_CTRL), CACHE_ENABLE | 0b1110);
        assert!(other.lock().unwrap().enabled(Which::ICache));
        assert_eq!(other.lock().unwrap().mmu.entry(5), 6);
    }
}
