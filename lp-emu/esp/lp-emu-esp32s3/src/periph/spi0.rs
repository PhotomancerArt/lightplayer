//! `SPI0` at `0x6000_3000` — the **cache's** own flash port.
//!
//! ⚠️ **On this chip SPI0 is at `0x6000_3000`, one block above SPI1 — the
//! other way round from the C6**, whose `spi0.rs` hard-codes `0x6000_2000`
//! (`m6/notes.md` §3.0 row 13). Ported by name against
//! [`crate::regs::SPI0`].
//!
//! **SPI0 does not port from the C6.** The C6's live SPI0 behaviour is its
//! flash-MMU path — `mmu_item_content 0x37c`, `mmu_item_index 0x380`,
//! `mmu_power_ctrl 0x384` — and **the S3's SPI0 has none of those**: its
//! table ends at `cache_fctrl 0x3c`, `cache_sctrl 0x40`, `sram_cmd 0x44`,
//! `sram_drd_cmd 0x48`, `sram_dwr_cmd 0x4c`, `ecc_err_addr 0xd0`. The S3's
//! flash MMU is a directly-addressed table at `0x600C_5000`
//! ([`crate::periph::flash_mmu`], [`crate::cache`]), read out of the ROM
//! rather than out of this block. So there is much less here than the C6's
//! SPI0 has, and what there is, is accept-and-remember. Three registers are
//! worth naming, all from the vendored ROM's disassembly:
//!
//! | offset | PAC name | who touches it |
//! |---|---|---|
//! | `0x054` | `fsm` | `Wait_SPI_Idle` (`0x4004_9b30`) polls `st` (bits 2:0) here **as well as** on SPI1 (`40049b2c: 60003054`) — the ROM waits for both controllers before it touches the part |
//! | `0x03c` | `cache_fctrl` | `SPI_init` (`0x4004_a4a8`) ORs in bit 0 and bit 2 (`4004a5d0..`, `4004a610..`); `esp_rom_spi_set_address_bit_len` read-modify-writes bit 1 |
//! | `0x008`, `0x010`, `0x014`, `0x018`..`0x028`, `0x0dc`, `0x0e0` | `ctrl`, `ctrl2`, `clock`, `user`..`miso_dlen`, `timing_cali`, `ddr` | `SPI_init`, `spi_common_set_flash_cs_timing` (`0x4004_a3a4`), `spi_cache_mode_switch` (`0x4004_a190`) and `SPIMasterReadModeCnfig` (`0x4004_a93c`) write the cache port's own read mode and timing on the ROM-up path; all remembered, none changes which bytes move |
//!
//! `cache_fctrl` bit 0 is *not* D4's bit. D4 is defined against `EXTMEM`'s
//! `icache_ctrl.icache_enable` / `dcache_ctrl.dcache_enable` — the bits the
//! ROM's `Cache_Enable_*`/`Cache_Disable_*` write ([`crate::cache`] carries
//! the disassemblies). This one is the cache controller's own flash-read
//! enable, and this machine remembers it rather than acting on it: the
//! window is served by a **fill** here, so there is no read path for it to
//! gate. Said out loud rather than modelled wrongly.
//!
//! **`cmd` refuses rather than executes.** The register is there and its
//! trigger bits self-clear — a model that remembered a trigger would wedge
//! anything polling it — but nothing is performed: on this chip the bytes
//! the *guest* asks for come through SPI1, and answering an SPI0 flash
//! command with a real JEDEC id would be a better-looking answer than the
//! part gives while its cache owns the bus. The refusal is written into the
//! trace once per distinct trigger, exactly as the classic's does.

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::periph::accept;
use crate::periph::spi1::{CMD, CMD_TRIGGERS};
use crate::regs;

const NAME: &str = "SPI0";

/// `cache_fctrl` — the cache controller's own flash-read enable.
pub const CACHE_FCTRL: u32 = 0x03c;

/// `fsm` — `Wait_SPI_Idle` polls bits 2:0 here too. Read-only, reset 0.
pub const FSM: u32 = 0x054;

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
    /// The register file is [`accept::spi0`]'s, so `accept.rs`'s reset sweep
    /// covers this block's every offset as it did in P04.
    pub fn new() -> Self {
        Self {
            regs: accept::spi0().with_grade(CMD, RegGrade::Documented),
            refused: Vec::new(),
        }
    }

    fn refuse(&mut self, triggered: u32, cx: &mut BusCx<'_>) {
        if self.refused.contains(&triggered) {
            return;
        }
        self.refused.push(triggered);
        cx.trace.note(&format!(
            "cyc={} pc={:#010x} {NAME} cmd trigger {triggered:#010x} is not performed here — \
             the bit self-clears and the w0..w15 buffer is unchanged. SPI1 at 0x60002000 is \
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
            // polls `fsm.st` rather than `cmd` on this block, and that is the
            // `RegFile`'s read-only reset of 0.
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
    use crate::periph::spi1::CMD_FLASH_RDID;
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn wait_spi_idle_sees_an_idle_state_machine_on_this_block_too() {
        // `Wait_SPI_Idle` (`0x4004_9b30`) polls `0x60002054` and then
        // `0x60003054`, both `extui a3, a3, 0, 3`.
        let mut sb = Sandbox::new();
        let mut spi0 = Spi0::new();
        assert_eq!(spi0.reg_name(FSM), Some("fsm"));
        assert_eq!(sb.read(&mut spi0, FSM) & 0b111, 0);
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
    fn spi_inits_writes_are_remembered() {
        // `SPI_init` (`0x4004_a4a8`) and `spi_common_set_flash_cs_timing`
        // write these on the ROM-up path and read none of them back for a
        // decision this machine makes.
        let mut sb = Sandbox::new();
        let mut spi0 = Spi0::new();
        for off in [
            0x008,
            0x010,
            0x014,
            0x018,
            0x020,
            0x024,
            0x028,
            CACHE_FCTRL,
            0x0dc,
            0x0e0,
        ] {
            sb.write(&mut spi0, off, 0xdead_0000 | off);
            assert_eq!(sb.read(&mut spi0, off), 0xdead_0000 | off, "+{off:#05x}");
        }
    }
}
