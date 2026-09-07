//! `SPI0` at `0x6000_2000` — the cache controller's register block.
//!
//! Almost all of it is accept-and-remember; three registers are live, and
//! they are the ones the mask ROM's `Cache_MMU_Init`, `Cache_MSPI_MMU_Set`
//! and `MMU_Get_Page_Mode` touch (the disassembly is quoted in
//! [`crate::cache`]):
//!
//! | offset | PAC name | what it does here |
//! |---|---|---|
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

/// The cache controller's register block, with the MMU behind it.
pub struct Spi0 {
    regs: RegFile,
    mmu: CacheHandle,
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
const RESETS: &[(u32, u32)] = &[
    (0x008, 0x802c_200c), // ctrl
    (0x00c, 0x28e0_0000), // ctrl1
    (0x010, 0x0000_2c21), // ctrl2
    (0x014, 0x0003_0103), // clock
    (0x01c, 0x5c00_0047), // user1
    (0x020, 0x7000_0000), // user2
    (0x03c, 0xc000_0000), // cache_fctrl
    (0x040, 0x0055_c070), // cache_sctrl
    (0x044, 0xc040_0000), // sram_cmd
    (0x050, 0x0003_0103), // sram_clk
    (0x054, 0x0000_0200), // fsm
    (0x0d4, 0x0000_3020), // ddr
    (0x0d8, 0x0000_3020), // spi_smem_ddr
    (0x168, 0x0100_5000), // ecc_ctrl
    (0x170, 0xfc00_0000), // axi_err_addr
    (0x174, 0x0008_0000), // spi_smem_ecc_ctrl
    (0x180, 0x0000_0001), // timing_cali
    (0x190, 0x0000_0001), // spi_smem_timing_cali
    (0x1a0, 0x8000_b084), // spi_smem_ac
    (0x200, 0x0000_0001), // clock_gate
    (0x384, 0x1320_0004), // mmu_power_ctrl — page mode 0 (bits 4:3 clear)
    (0x388, 0x0000_000f), // dpa_ctrl
];

/// The four-entry PMS arrays, whose reset value the PAC gives once for the
/// array: `spi_{f,s}mem_pms<n>_attr` = 3, `…_size` = `0x1000`.
const PMS_RESETS: &[(u32, u32)] = &[
    (0x100, 0x0000_0003), // spi_fmem_pms0..3_attr
    (0x120, 0x0000_1000), // spi_fmem_pms0..3_size
    (0x130, 0x0000_0003), // spi_smem_pms0..3_attr
    (0x150, 0x0000_1000), // spi_smem_pms0..3_size
];

impl Spi0 {
    pub fn new(mmu: CacheHandle) -> Self {
        let mut regs = RegFile::new("SPI0", 0x400).with_names(regs::SPI0);
        for (off, value) in RESETS {
            regs = regs.with_reset(*off, *value);
        }
        for (base, value) in PMS_RESETS {
            for n in 0..4 {
                regs = regs.with_reset(base + 4 * n, *value);
            }
        }
        Self { regs, mmu }
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
            _ => self.regs.read(off, width, cx),
        }
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        match off & !3 {
            ITEM_CONTENT => {
                let mut mmu = self.mmu.lock().unwrap();
                let index = mmu.index();
                let word = merge_lane(mmu.entry(index), off, width, value);
                mmu.set_entry(index, word);
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
