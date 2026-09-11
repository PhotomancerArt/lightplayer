//! `SPI0` at `0x3FF4_3000` — the **cache's** flash port.
//!
//! ⚠️ **On the classic, SPI0 and SPI1 are the same IP.** The PAC gives them
//! one `RegisterBlock` (`regs::SPI0`), so this block has a `cmd`, a `user`
//! engine and a `w0..w15` buffer exactly like [`super::spi1`] — and the
//! classic's block carries `cache_fctrl` / `cache_sctrl` / `sram_*` in that
//! same register file, where the C6 keeps its cache registers in SPI0 alone
//! and its MMU behind `mmu_item_index`/`mmu_item_content`. **The classic's
//! flash MMU is not in SPI0 at all**: it is two raw arrays at `0x3FF1_0000`
//! and `0x3FF1_2000` ([`crate::periph::flash_mmu`]).
//!
//! So there is much less here than the C6's SPI0 has, and what there is, is
//! accept-and-remember. Two registers are worth naming:
//!
//! | offset | PAC name | who touches it |
//! |---|---|---|
//! | `0x050` | `cache_fctrl` | `Cache_Read_Enable` (`0x4000_9A84`) ORs in **bit 0**; `Cache_Read_Disable` (`0x4000_9AB8`) clears it after the other core's cache is off |
//! | `0x0f8` | `ext2` | `Wait_SPI_Idle` (`0x4006_22C0`) polls `st` (bits 2:0) here **as well as** on SPI1 — the ROM waits for both controllers before it touches the part |
//!
//! `cache_fctrl` bit 0 is *not* D4's bit. D4 is defined against
//! `DPORT.pro_cache_ctrl.pro_cache_enable` (bit 3), which is the per-core
//! enable the same two ROM routines write in the same breath
//! ([`crate::cache`] carries both disassemblies). This one is the cache
//! controller's own flash-read enable, and this machine remembers it rather
//! than acting on it: the window is served by a **fill** here, so there is
//! no read path for it to gate. Said out loud rather than modelled wrongly.
//!
//! `spi_flash_attach` (`0x4006_2A6C`) writes `+0x08`, `+0x18`, `+0x24`,
//! `+0x28`, `+0x2c`, `+0x34`, `+0x50` and `+0xfc` here on the ROM-up path;
//! all of them are remembered, and none of them changes which bytes move.
//!
//! **`cmd` refuses rather than executes.** The register is there and its
//! trigger bits self-clear — a model that remembered a trigger would wedge
//! anything polling it — but nothing is performed: on this chip the bytes
//! the *guest* asks for come through SPI1, and answering an SPI0 flash
//! command with a real JEDEC id would be a better-looking answer than the
//! part gives while its cache owns the bus. The refusal is written into the
//! trace once per distinct trigger.

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::periph::accept;
use crate::periph::spi1::{CMD, CMD_TRIGGERS};
use crate::regs;

const NAME: &str = "SPI0";

/// `cache_fctrl` — the cache controller's own flash-read enable.
pub const CACHE_FCTRL: u32 = 0x050;

/// `cache_fctrl.cache_flash_usr_cmd`, bit 0: what `Cache_Read_Enable` ORs in
/// and `Cache_Read_Disable` masks out.
pub const CACHE_FLASH_USR_CMD: u32 = 1 << 0;

/// The cache's flash port.
pub struct Spi0 {
    regs: RegFile,
    /// Every `cmd` trigger written here, once each.
    refused: Vec<u32>,
}

impl Default for Spi0 {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for Spi0 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Spi0").finish_non_exhaustive()
    }
}

impl Spi0 {
    /// The register file is [`accept::spi`]'s, so `accept.rs`'s reset sweep
    /// covers this block's every offset as it did in P3.
    pub fn new() -> Self {
        Self {
            regs: accept::spi(NAME).with_grade(CMD, RegGrade::Documented),
            refused: Vec::new(),
        }
    }

    /// Is the cache controller's flash-read enable set? For the run report
    /// and for a test; nothing in this machine reads it to decide anything.
    pub fn cache_flash_enabled(&self) -> bool {
        self.regs.stored(CACHE_FCTRL) & CACHE_FLASH_USR_CMD != 0
    }

    fn refuse(&mut self, triggered: u32, cx: &mut BusCx<'_>) {
        if self.refused.contains(&triggered) {
            return;
        }
        self.refused.push(triggered);
        cx.trace.note(&format!(
            "cyc={} pc={:#010x} {NAME} cmd trigger {triggered:#010x} is not performed here — \
             the bit self-clears and the w0..w15 buffer is unchanged. SPI1 at 0x3ff42000 is \
             the block that moves flash bytes on this machine, and the cache window is served \
             by a fill (see crate::cache).",
            cx.now, cx.pc
        ));
    }
}

impl Peripheral for Spi0 {
    fn name(&self) -> &'static str {
        NAME
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        if off & !3 == CMD {
            // Idle, always — see the note on the write side. `Wait_SPI_Idle`
            // polls `ext2.st` rather than `cmd` on this block, and that is
            // the `RegFile`'s read-only reset of 0.
            return lane_of(0, off, width);
        }
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        if off & !3 != CMD {
            self.regs.write(off, width, value, cx);
            return;
        }
        let word = merge_lane(0, off, width, value);
        if word & CMD_TRIGGERS != 0 {
            self.refuse(word & CMD_TRIGGERS, cx);
        }
        self.regs.poke(CMD, 0);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::SPI0.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.regs.reg_grade(off)
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
    use crate::periph::spi1::{CMD_FLASH_RDID, EXT2};
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn wait_spi_idle_sees_an_idle_state_machine_on_this_block_too() {
        // `Wait_SPI_Idle` (`0x4006_22C0`) polls `0x3ff420f8` and then
        // `0x3ff430f8`, both `extui a3, a3, 0, 3`.
        let mut sb = Sandbox::new();
        let mut spi0 = Spi0::new();
        assert_eq!(sb.read(&mut spi0, EXT2) & 0b111, 0);
    }

    #[test]
    fn cache_read_enable_and_disable_move_bit_zero_of_cache_fctrl() {
        // `Cache_Read_Enable` (`0x4000_9A84`): SPI0+0x50 |= 1.
        // `Cache_Read_Disable` (`0x4000_9AB8`): SPI0+0x50 &= ~1, after the
        // spin on `ext2`.
        let mut sb = Sandbox::new();
        let mut spi0 = Spi0::new();
        assert!(!spi0.cache_flash_enabled(), "the PAC reset for +0x50 is 0");
        let word = sb.read(&mut spi0, CACHE_FCTRL);
        sb.write(&mut spi0, CACHE_FCTRL, word | CACHE_FLASH_USR_CMD);
        assert!(spi0.cache_flash_enabled());
        let word = sb.read(&mut spi0, CACHE_FCTRL);
        sb.write(&mut spi0, CACHE_FCTRL, word & !CACHE_FLASH_USR_CMD);
        assert!(!spi0.cache_flash_enabled());
    }

    #[test]
    fn a_cmd_trigger_here_self_clears_and_is_refused_once() {
        let mut sb = Sandbox::new();
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut spi0 = Spi0::new();
        sb.write(&mut spi0, CMD, CMD_FLASH_RDID);
        sb.write(&mut spi0, CMD, CMD_FLASH_RDID);
        assert_eq!(sb.read(&mut spi0, CMD), 0, "the guest must not spin");
        assert_eq!(
            sb.read(&mut spi0, crate::periph::spi1::W0),
            0,
            "the buffer is unchanged: no id was invented"
        );
        let refusals = buf
            .lines()
            .into_iter()
            .filter(|l| l.contains("is not performed here"))
            .count();
        assert_eq!(refusals, 1);
    }

    #[test]
    fn spi_flash_attachs_writes_are_remembered() {
        // `spi_flash_attach` (`0x4006_2A6C`) writes these eight and reads
        // none of them back for a decision this machine makes.
        let mut sb = Sandbox::new();
        let mut spi0 = Spi0::new();
        for off in [0x008, 0x018, 0x024, 0x028, 0x02c, 0x034, 0x050, 0x0fc] {
            sb.write(&mut spi0, off, 0xdead_0000 | off);
            assert_eq!(sb.read(&mut spi0, off), 0xdead_0000 | off, "+{off:#05x}");
        }
    }
}
