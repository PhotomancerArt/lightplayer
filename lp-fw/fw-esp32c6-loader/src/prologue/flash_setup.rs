//! The flash controllers' read setup (contract C3): exactly what the IDF
//! leaves, so the core reads flash at the speed and in the mode it always
//! has.
//!
//! The ROM loads this image in DIO from the header and leaves SPI0 reading
//! in DIO. The IDF then (measured, emulated IDF boot, 2026-10-17):
//!
//! - sets the MSPI fast clock's high-speed divider to 5
//!   (`PCR.mspi_clk_conf = 0x500`: 480 MHz / 6 = 80 MHz once the core puts
//!   the root clock on the PLL — esp-hal writes the same divider before it
//!   switches; the low-speed divider stays 0, the crystal's 40 MHz, while
//!   this stage runs);
//! - sets SPI0's and SPI1's clocks equal to that MSPI clock
//!   (`clock = 0x8000_0000`);
//! - zeroes SPI0's CS setup and hold times (`ctrl2[9:0]`: the ROM leaves
//!   `0x2C41`, the IDF `0x2C00`);
//! - sets the flash pads' drive strength to 1 (`IO_MUX` `FUN_DRV`; the ROM
//!   leaves 2).
//!
//! It also issues a few SPI1 commands (status reads, the ID) and leaves
//! SPI1's user/length registers as its last command did. **We issue no SPI1
//! command at all** — the loader's rule since a ROM SPI1 routine left the
//! core's `RDID` reading garbage — but leave those registers holding the
//! same values, so the core's first SPI1 command starts from the state it
//! always has. None of these writes starts a transaction (only `cmd` does).

use super::mmio::{modify, write};

const PCR_MSPI_CLK_CONF: u32 = 0x6009_601C;
const MSPI_FAST_HS_DIV: u32 = 0xff << 8;

const SPI0: u32 = 0x6000_2000;
const SPI1: u32 = 0x6000_3000;
const ADDR: u32 = 0x04;
const CTRL2: u32 = 0x10;
const CLOCK: u32 = 0x14;
const USER: u32 = 0x18;
const USER1: u32 = 0x1c;
const USER2: u32 = 0x20;
const MOSI_DLEN: u32 = 0x24;
const MISO_DLEN: u32 = 0x28;
const W0: u32 = 0x58;
const CLK_EQU_SYSCLK: u32 = 1 << 31;
/// `ctrl2` cs_setup_time [4:0] and cs_hold_time [9:5].
const CS_SETUP_AND_HOLD: u32 = 0x3ff;

const IO_MUX: u32 = 0x6009_0000;
/// The flash pads the IDF sets (GPIO 24–30 but 27, which it leaves alone).
const FLASH_PADS: [u32; 6] = [24, 25, 26, 28, 29, 30];
const FUN_DRV: u32 = 0b11 << 10;

pub unsafe fn mspi_and_spi_clocks() {
    // SAFETY: PCR and SPI0/SPI1 registers; nothing reads flash meanwhile
    // (this stage runs from RAM).
    unsafe {
        modify(PCR_MSPI_CLK_CONF, MSPI_FAST_HS_DIV, 5 << 8);
        write(SPI0 + CLOCK, CLK_EQU_SYSCLK);
        write(SPI1 + CLOCK, CLK_EQU_SYSCLK);
    }
}

pub unsafe fn spi_leftovers() {
    // SAFETY: as above; no `cmd` write, so no transaction.
    unsafe {
        modify(SPI0 + CTRL2, CS_SETUP_AND_HOLD, 0);
        write(SPI1 + ADDR, 0);
        write(SPI1 + USER, 0x8000_00C0);
        write(SPI1 + USER1, 0x5C00_0007);
        write(SPI1 + USER2, 0x7000_0004);
        write(SPI1 + MOSI_DLEN, 0);
        write(SPI1 + MISO_DLEN, 0);
        write(SPI1 + W0, 0);
        for pad in FLASH_PADS {
            modify(IO_MUX + 0x04 + 4 * pad, FUN_DRV, 1 << 10);
        }
    }
}
