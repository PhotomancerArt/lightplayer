//! `SPI0` at `0x6000_2000` — the cache controller's register block.
//!
//! Almost all of it is accept-and-remember; four registers are live, and
//! three of them are the ones the mask ROM's `Cache_MMU_Init`,
//! `Cache_MSPI_MMU_Set` and `MMU_Get_Page_Mode` touch (the disassembly is
//! quoted in [`crate::cache`]):
//!
//! | offset | PAC name | what it does here |
//! |---|---|---|
//! | `0x000` | `cmd` | reads idle, and a trigger written to it self-clears — see [`Spi0::refuse`], and note that **esptool addresses this block, not SPI1** |
//! | `0x37c` | `mmu_item_content` | reads and writes the entry `mmu_item_index` selects |
//! | `0x380` | `mmu_item_index` | selects it; no auto-increment |
//! | `0x384` | `mmu_power_ctrl` | bits 4:3 are the page mode, and drive [`CacheMmu::set_page_mode`] |
//!
//! Everything else — the timing calibration, the PMS protection ranges, the
//! XTS flash-encryption block, `cache_fctrl`/`cache_sctrl` and the SRAM
//! command registers — is a [`RegFile`]. The shipped C6 image touches none
//! of them on the direct-load path (the spike inventory's `spimem` rows are
//! esp-emu's one synthetic controller standing in for SPI0 *and* SPI1); the
//! ROM-up boot is M7's, and this is the block it will program.

use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::cache::{CacheHandle, ITEM_CONTENT, ITEM_INDEX, POWER_CTRL};
use crate::regs;

/// `cmd` +0x000. Same layout as SPI1's: every bit above `mst_st`/`slv_st` is
/// a self-clearing trigger, and the register reads back idle once whatever
/// was asked for is done.
const CMD: u32 = 0x000;

/// The trigger bits of `cmd`, SPI1's set exactly ([`super::spi1`]): `usr`
/// (1<<18) through `flash_read` (1<<31), plus `flash_pe` (1<<17).
const CMD_TRIGGERS: u32 = 0xfffe_0000;

/// The cache controller's register block, with the MMU behind it.
pub struct Spi0 {
    regs: RegFile,
    mmu: CacheHandle,
    /// Every `cmd` trigger written here, once each, so a run that leans on
    /// SPI0 moving bytes says so instead of quietly reading zeros.
    refused: Vec<u32>,
}

impl core::fmt::Debug for Spi0 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Spi0").finish_non_exhaustive()
    }
}

/// SPI0's non-zero PAC reset values (`esp32c6-0.23.2/src/spi0/*.rs`), the
/// same derived data as [`crate::periph::spi1::Spi1`]'s — where a wrong
/// reset value was a live bug.
///
/// Two are deliberately **not** here. `mmu_item_content` and
/// `registerrnd_eco_high`/`_low` all claim a reset value of `0x37c`, which is
/// `mmu_item_content`'s own offset: an SVD artefact, not a hardware fact
/// (and `0x37c & 0x200` would set [`crate::cache::VALID`] on every entry).
/// The ROM says what a cleared table is instead — `Cache_MMU_Init` writes
/// **0** to all 256 entries — so [`crate::cache::CacheMmu::new`] starts at 0
/// and this table leaves those three alone.
impl Spi0 {
    pub fn new(mmu: CacheHandle) -> Self {
        let regs = RegFile::new("SPI0", 0x400).with_names(regs::SPI0);
        Self {
            regs,
            mmu,
            refused: Vec::new(),
        }
    }

    /// A `cmd` trigger this block does not perform: say so once, and leave
    /// the buffer alone.
    ///
    /// **Why the bit clears anyway, and what that models.** `cmd`'s trigger
    /// bits are self-clearing on the part — the operation completes and the
    /// register reads idle, which is what `Wait_SPI_Idle` spins for — so a
    /// model that REMEMBERED the write would wedge any host polling it. That
    /// is not hypothetical: esptool-js 0.6.0 puts the ESP32-C6's
    /// `SPI_REG_BASE` at `0x6000_2000`, which is **this** block and not SPI1
    /// at `0x6000_3000` (the PAC's own map, `esp32c6-0.23.2/src/lib.rs`), so
    /// `readFlashId`'s `RDID` lands here. Measured 2026-09-09 through
    /// Studio's own flash flow: the poll read `1<<18` ten times and
    /// `ESPLoader.main()` threw `Unable to verify flash chip connection Error:
    /// SPI command did not complete in time` — before the chip guard, before
    /// any write.
    ///
    /// **And why nothing is executed.** What the part does *next* is measured
    /// too, on the desk C6 (rev 2, over USB-Serial-JTAG, 2026-07-31, quoted in
    /// `browser_esp32_flash.js`): the ID probe reads **0** and esptool prints
    /// "Failed to communicate with the flash chip", while the stub's own
    /// reads and writes work fine. So the faithful model of this register on
    /// this part is "the command completes and the buffer is unchanged" — a
    /// `w0` of 0 — and `main()` logs its warning and carries on, exactly as
    /// it does on the board. Implementing an SPI0 `usr` engine over the flash
    /// would answer with a real JEDEC id, which is a better-looking answer
    /// than the silicon gives and therefore the wrong one.
    fn refuse(&mut self, triggered: u32, cx: &mut BusCx<'_>) {
        if self.refused.contains(&triggered) {
            return;
        }
        self.refused.push(triggered);
        cx.trace.note(&format!(
            "cyc={} pc={:#010x} SPI0 cmd trigger {triggered:#010x} is not performed here — the \
             bit self-clears and the w0..w15 buffer is unchanged, which is what the part does \
             for a host-driven flash-id probe (see Spi0::refuse). SPI1 at 0x6000_3000 is the \
             block that moves flash bytes.",
            cx.now, cx.pc
        ));
    }
}

impl Peripheral for Spi0 {
    fn name(&self) -> &'static str {
        "SPI0"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        match off & !3 {
            ITEM_CONTENT => {
                let mmu = self.mmu.lock().unwrap();
                lane_of(mmu.entry(mmu.index()), off, width)
            }
            ITEM_INDEX => lane_of(self.mmu.lock().unwrap().index(), off, width),
            // Idle, always — see the note on the write side.
            CMD => lane_of(0, off, width),
            _ => self.regs.read(off, width, cx),
        }
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        match off & !3 {
            CMD => {
                let word = merge_lane(0, off, width, value);
                if word & CMD_TRIGGERS != 0 {
                    self.refuse(word & CMD_TRIGGERS, cx);
                }
                self.regs.poke(CMD, 0);
            }
            ITEM_CONTENT => {
                let mut mmu = self.mmu.lock().unwrap();
                let index = mmu.index();
                let word = merge_lane(mmu.entry(index), off, width, value);
                mmu.set_entry(index, word);
                drop(mmu);
                // The entry is dirty and only the machine can fill the
                // window from flash. Ending the slice here is the whole
                // difference between "the guest reads what it just mapped"
                // and "the guest reads what was there before": the
                // second-stage bootloader maps its own image header and
                // reads it a dozen instructions later, and read zeros until
                // this line existed.
                cx.yield_to_machine();
            }
            ITEM_INDEX => {
                let mut mmu = self.mmu.lock().unwrap();
                let word = merge_lane(mmu.index(), off, width, value);
                mmu.set_index(word);
            }
            POWER_CTRL => {
                self.regs.write(off, width, value, cx);
                let word = self.regs.stored(POWER_CTRL);
                self.mmu
                    .lock()
                    .unwrap()
                    .set_page_mode(((word >> 3) & 3) as u8);
            }
            _ => self.regs.write(off, width, value, cx),
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::SPI0.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        self.regs.save_state()
    }

    fn load_state(&mut self, bytes: &[u8]) {
        self.regs.load_state(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CacheMmu, ENTRIES, VALID};
    use lp_emu_esp_common::Sandbox;
    use std::sync::{Arc, Mutex};

    fn rig() -> (Sandbox, Spi0, CacheHandle) {
        let mmu = Arc::new(Mutex::new(CacheMmu::new()));
        (Sandbox::new(), Spi0::new(mmu.clone()), mmu)
    }

    #[test]
    fn cache_mmu_inits_loop_clears_every_entry() {
        // `Cache_MMU_Init` (`0x4002_7c76`), instruction for instruction:
        //   for (i = 0; i != 256; i++) { *(0x60002380) = i; *(0x6000237c) = 0; }
        let (mut sb, mut spi0, mmu) = rig();
        for i in 0..ENTRIES as u32 {
            sb.write(&mut spi0, ITEM_INDEX, i);
            sb.write(&mut spi0, ITEM_CONTENT, 0);
        }
        assert!(mmu.lock().unwrap().mapped_pages().is_empty());
        assert_eq!(sb.read(&mut spi0, ITEM_INDEX), 255);
    }

    #[test]
    fn cache_mspi_mmu_set_writes_index_then_content_and_the_page_maps() {
        // `Cache_MSPI_MMU_Set` (`0x4002_7c90`): index at +0x380, then
        // `page | encrypt | 512` at +0x37c.
        let (mut sb, mut spi0, mmu) = rig();
        sb.write(&mut spi0, ITEM_INDEX, 5);
        sb.write(&mut spi0, ITEM_CONTENT, 6 | VALID);
        assert_eq!(
            mmu.lock().unwrap().translate(0x4205_0020),
            Some(0x0006_0020)
        );
        // And the entry reads back through the same window.
        assert_eq!(sb.read(&mut spi0, ITEM_CONTENT), 6 | VALID);
        // The index does not auto-increment: the ROM rewrites it each time.
        assert_eq!(sb.read(&mut spi0, ITEM_INDEX), 5);
    }

    #[test]
    fn the_page_mode_lives_where_mmu_get_page_mode_reads_it() {
        // `MMU_Get_Page_Mode` (`0x4002_75ea`): `lw a0, 900(a5); srli 3; andi 3`.
        let (mut sb, mut spi0, mmu) = rig();
        assert_eq!(mmu.lock().unwrap().page_mode(), 0);
        sb.write(&mut spi0, POWER_CTRL, 1 << 3);
        assert_eq!(mmu.lock().unwrap().page_mode(), 1);
        assert_eq!(mmu.lock().unwrap().page_len(), 32 * 1024);
        sb.write(&mut spi0, POWER_CTRL, 3 << 3);
        assert_eq!(mmu.lock().unwrap().page_mode(), 3);
        // And the register itself still reads back what was written.
        assert_eq!((sb.read(&mut spi0, POWER_CTRL) >> 3) & 3, 3);
    }

    #[test]
    fn everything_else_is_accept_and_remember_with_the_pacs_names() {
        let (mut sb, mut spi0, _mmu) = rig();
        sb.write(&mut spi0, 0x03c, 0xdead_beef);
        assert_eq!(sb.read(&mut spi0, 0x03c), 0xdead_beef);
        assert_eq!(spi0.reg_name(0x03c), Some("cache_fctrl"));
        assert_eq!(spi0.reg_name(ITEM_CONTENT), Some("mmu_item_content"));
        assert_eq!(spi0.reg_name(ITEM_INDEX), Some("mmu_item_index"));
        assert_eq!(spi0.reg_name(POWER_CTRL), Some("mmu_power_ctrl"));
    }
}
