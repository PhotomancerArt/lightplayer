//! `SPI1` at `0x6000_3000` — the legacy flash controller, the block every
//! flash access in this firmware goes through.
//!
//! # Who drives it, and how
//!
//! Two callers, and they are the whole of the decode this model needs.
//!
//! **`esp_storage::get_flash_size()`** drives the block directly
//! (`third_party/esp-storage/src/hardware.rs`):
//!
//! ```text
//! spi1.cmd().write(|w| w.flash_rdid().set_bit());
//! while spi1.cmd().read().flash_rdid().bit_is_set() {}
//! spi1.w(0).read().buf().bits() & 0x00FF_FFFF
//! ```
//!
//! That single write is where every flash-backed image stopped at the end of
//! M3: `W4 SPI1+0x000 cmd = 0x10000000` at 11 ms, then a `RegFile` that
//! remembered the bit for ever.
//!
//! **The mask ROM's `esp_rom_spiflash_*` family** does everything else, and
//! esp-storage reaches it through `esp_rom_sys` (`hardware.rs` again). The
//! sequences below are read off the vendored ROM ELF
//! (`lp-emu/esp/roms/esp32c6_rev0_rom.elf`, `riscv64-unknown-elf-objdump
//! -d`); every address is a real symbol in it:
//!
//! | ROM function | what it writes to SPI1 |
//! |---|---|
//! | `SPI_read_data` `0x4002_4100` | `addr` = byte address, `miso_dlen` = bits−1, `cmd` = `1<<18` (`usr`), spin on `cmd != 0`, read `w0..w15` |
//! | `SPI_page_program` `0x4002_4692` | `addr`, `w0..`, `mosi_dlen`, `user` \|= `usr_command\|usr_addr\|usr_mosi`, `user2` = `0x7000_0002` (8-bit command `0x02`), `cmd` = `1<<18 \| 1<<17` |
//! | `SPI_sector_erase` `0x4002_4340` | `addr`, `cmd` = `1<<24 \| 1<<17` (`flash_se` + `flash_pe`) |
//! | `SPI_chip_erase` `0x4002_4308` | `cmd` = `1<<22` (`flash_ce`) |
//! | `_SPI_write_enable` `0x4002_4630` | `cmd` = `1<<30` (`flash_wren`), then reads status until **WEL** (bit 1) is set |
//! | `esp_rom_spiflash_read_status` `0x4002_4228` | `rd_status &= 0xffff_0000`, `cmd` = `1<<27` (`flash_rdsr`), spin, read `rd_status` |
//! | `esp_rom_spiflash_write_status` `0x4002_43f0` | `rd_status[15:0]` = value, `cmd` = `1<<26` (`flash_wrsr`) |
//! | `esp_rom_spiflash_read_user_cmd` `0x4002_41b2` | `miso_dlen` = 7, `user` = `0x9000_0000`, `user2` = `0x7000_0000\|cmd`, `w0` = 0, `cmd` = `1<<18`, read `w0 & 0xff` |
//! | `Wait_SPI_Idle` `0x4002_42e8` | spins on `cmd & 0xf` (`mst_st`), then on status **WIP** (bit 0) |
//!
//! Three consequences shape this model.
//!
//! 1. **Every trigger bit is self-clearing and the operation completes
//!    inside the write.** `Wait_SPI_Idle` spins on `mst_st`, so `cmd`'s low
//!    nibble must read 0; every spin above is `while cmd != 0`. A model that
//!    took a cycle to finish would need an event and would give the same
//!    answer, because nothing observes the interval — [`Spi1`] therefore
//!    completes synchronously and says so, rather than inventing a duration
//!    it never measured. (`t2`'s flash wait states are a cycle-model rung,
//!    not a peripheral one — director note 6.)
//! 2. **The status register is real state.** WIP (bit 0) is always 0 because
//!    operations complete; WEL (bit 1) is set by `flash_wren`, cleared by
//!    `flash_wrdi` and by any completed program or erase, exactly as the
//!    part does — and `_SPI_write_enable` **loops until it sees WEL set**, so
//!    a model that left it 0 would hang the first write.
//! 3. **`usr` is a generic engine.** The ROM reaches read and page-program
//!    through it with the command byte in `user2`, so this models the
//!    engine (command / address / dummy / data phases from `user`, `user1`,
//!    `user2`, `addr`, `mosi_dlen`, `miso_dlen`, `w0..w15`) rather than the
//!    ROM's particular choices. Any read mode the ROM picks — `0x03`,
//!    `0x0b`, `0x3b`, `0x6b`, `0xbb`, `0xeb` — is then the same transaction
//!    with different phase widths.
//!
//! # What is *not* modelled, and what happens instead
//!
//! Bus width (`ctrl`'s `fread_*` bits), clock dividers, timing calibration,
//! CRC, encrypted writes, suspend/resume and the interrupt registers are
//! accept-and-remember: the bytes that move do not depend on them, and a
//! model that pretended to honour them would be inventing behaviour. A
//! `cmd` trigger this table does not know is **refused loudly** — it clears
//! the bit (so the guest does not hang) and writes a `SPI1 unmodelled
//! command` line into the trace naming the bit and the PC, which is the
//! honest answer for a bring-up: the run continues and the log says what it
//! did not do.

use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::flash::{BLOCK_LEN, FlashHandle, PAGE_LEN, SECTOR_LEN};
use crate::regs;

// Register offsets (`regs::SPI1`, generated from the PAC).
pub const CMD: u32 = 0x000;
pub const ADDR: u32 = 0x004;
pub const CTRL: u32 = 0x008;
pub const USER: u32 = 0x018;
pub const USER1: u32 = 0x01c;
pub const USER2: u32 = 0x020;
pub const MOSI_DLEN: u32 = 0x024;
pub const MISO_DLEN: u32 = 0x028;
pub const RD_STATUS: u32 = 0x02c;
pub const W0: u32 = 0x058;
/// `w0..w15`: the 64-byte data buffer every transfer moves through.
pub const W_COUNT: u32 = 16;

// `cmd` trigger bits (PAC `spi1::cmd`, bit numbers from its own doc lines).
pub const CMD_FLASH_PE: u32 = 1 << 17;
pub const CMD_USR: u32 = 1 << 18;
pub const CMD_FLASH_HPM: u32 = 1 << 19;
pub const CMD_FLASH_RES: u32 = 1 << 20;
pub const CMD_FLASH_DP: u32 = 1 << 21;
pub const CMD_FLASH_CE: u32 = 1 << 22;
pub const CMD_FLASH_BE: u32 = 1 << 23;
pub const CMD_FLASH_SE: u32 = 1 << 24;
pub const CMD_FLASH_PP: u32 = 1 << 25;
pub const CMD_FLASH_WRSR: u32 = 1 << 26;
pub const CMD_FLASH_RDSR: u32 = 1 << 27;
pub const CMD_FLASH_RDID: u32 = 1 << 28;
pub const CMD_FLASH_WRDI: u32 = 1 << 29;
pub const CMD_FLASH_WREN: u32 = 1 << 30;
pub const CMD_FLASH_READ: u32 = 1 << 31;
/// Every bit `cmd` can be triggered with. `mst_st`/`slv_st` (bits 0:7) are
/// status, and read 0 — the whole of `Wait_SPI_Idle`'s first loop.
const CMD_TRIGGERS: u32 = CMD_FLASH_PE
    | CMD_USR
    | CMD_FLASH_HPM
    | CMD_FLASH_RES
    | CMD_FLASH_DP
    | CMD_FLASH_CE
    | CMD_FLASH_BE
    | CMD_FLASH_SE
    | CMD_FLASH_PP
    | CMD_FLASH_WRSR
    | CMD_FLASH_RDSR
    | CMD_FLASH_RDID
    | CMD_FLASH_WRDI
    | CMD_FLASH_WREN
    | CMD_FLASH_READ;

// `user` phase-enable bits.
const USER_MOSI: u32 = 1 << 27;
const USER_MISO: u32 = 1 << 28;
const USER_ADDR: u32 = 1 << 30;
const USER_COMMAND: u32 = 1 << 31;

// Status-register bits, as the ROM reads them.
/// Write-in-progress. Always 0 here: operations complete inside the write.
pub const SR_WIP: u16 = 1 << 0;
/// Write-enable latch. `_SPI_write_enable` loops until this is set.
pub const SR_WEL: u16 = 1 << 1;

/// SPI NOR command bytes the `usr` engine may carry. Only the ones the ROM
/// actually sends are named; anything else is refused with a trace line.
mod op {
    pub const READ: u8 = 0x03;
    pub const FAST_READ: u8 = 0x0b;
    pub const DUAL_OUT: u8 = 0x3b;
    pub const QUAD_OUT: u8 = 0x6b;
    pub const DUAL_IO: u8 = 0xbb;
    pub const QUAD_IO: u8 = 0xeb;
    pub const PAGE_PROGRAM: u8 = 0x02;
    pub const READ_STATUS: u8 = 0x05;
    pub const READ_STATUS_HIGH: u8 = 0x35;
    pub const WRITE_ENABLE: u8 = 0x06;
    pub const WRITE_DISABLE: u8 = 0x04;
    pub const RDID: u8 = 0x9f;
    pub const SECTOR_ERASE: u8 = 0x20;
    pub const BLOCK_ERASE_64K: u8 = 0xd8;

    /// The read commands, all of which mean "give me `miso_dlen` bits from
    /// `addr`". The mode/dummy differences are wire-level and move no
    /// different bytes.
    pub const READS: &[u8] = &[READ, FAST_READ, DUAL_OUT, QUAD_OUT, DUAL_IO, QUAD_IO];
}

/// The legacy flash controller.
pub struct Spi1 {
    regs: RegFile,
    flash: FlashHandle,
    /// The flash chip's status register. Bits above WEL are whatever
    /// `flash_wrsr` last wrote — the part remembers block-protect bits and
    /// `_esp_rom_spiflash_unlock` writes them, so a model that dropped them
    /// would make the unlock unobservable.
    status: u16,
    /// Every `cmd` trigger this model does not know, once each, so a boot
    /// that reaches one says so without flooding the log.
    refused: Vec<u32>,
}

impl core::fmt::Debug for Spi1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Spi1")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Spi1 {
    pub fn new(flash: FlashHandle) -> Self {
        Self {
            regs: RegFile::new("SPI1", 0x400).with_names(regs::SPI1),
            flash,
            status: 0,
            refused: Vec::new(),
        }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    /// The `w0..w15` buffer as bytes, little-endian per word — the layout
    /// the ROM's copy loops assume (`SPI_read_data` reads whole words out of
    /// `0x6000_3058 + 4n`).
    fn buffer_bytes(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        for i in 0..W_COUNT {
            let word = self.regs.stored(W0 + 4 * i);
            out[(i * 4) as usize..(i * 4 + 4) as usize].copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    fn set_buffer_bytes(&mut self, data: &[u8]) {
        for i in 0..W_COUNT {
            let mut word = [0u8; 4];
            for (b, slot) in word.iter_mut().enumerate() {
                let at = (i * 4) as usize + b;
                *slot = data.get(at).copied().unwrap_or(0xff);
            }
            self.regs.poke(W0 + 4 * i, u32::from_le_bytes(word));
        }
    }

    /// The flash byte address the address phase carries.
    ///
    /// `SPI_read_data` and `SPI_page_program` both write the plain byte
    /// address into `addr` (`sw a1,4(a5)`), and `user1`'s
    /// `usr_addr_bitlen` says how many bits go on the wire — 24 for a plain
    /// read, 28 for QIO's four mode bits. The bits above 24 are therefore
    /// mode, not address, and this chip is at most 16 MiB, so the address is
    /// the low 24 bits. Anything above them is reported once rather than
    /// silently folded.
    fn flash_addr(&mut self, cx: &mut BusCx<'_>) -> u32 {
        let raw = self.regs.stored(ADDR);
        if raw & 0xff00_0000 != 0 {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} SPI1 addr {raw:#010x} has bits above 24; \
                 this chip is 16 MiB at most, using {:#010x}",
                cx.now,
                cx.pc,
                raw & 0x00ff_ffff
            ));
        }
        raw & 0x00ff_ffff
    }

    /// Bytes = `dlen + 1` bits, rounded up, capped at the 64-byte buffer.
    fn dlen_bytes(&self, off: u32) -> u32 {
        let bits = (self.regs.stored(off) & 0x3ff) + 1;
        bits.div_ceil(8).min(64)
    }

    /// Run whatever `cmd` was just triggered with, and clear the bits.
    fn execute(&mut self, triggered: u32, cx: &mut BusCx<'_>) {
        // Order matters only in that `usr` is the generic engine and the
        // dedicated bits are shortcuts; nothing sets two at once except
        // `flash_pe`, which is a modifier ("this `usr`/`flash_se` is a
        // program or erase") and carries no work of its own.
        let dedicated = triggered & !(CMD_USR | CMD_FLASH_PE);
        if triggered & CMD_USR != 0 {
            self.execute_usr(cx);
        }
        match dedicated {
            0 => {}
            CMD_FLASH_RDID => {
                let id = self.flash.lock().unwrap().jedec_id();
                self.regs.poke(W0, id);
            }
            CMD_FLASH_RDSR => {
                self.flash.lock().unwrap().status_reads += 1;
                let kept = self.regs.stored(RD_STATUS) & 0xffff_0000;
                self.regs.poke(RD_STATUS, kept | u32::from(self.status));
            }
            CMD_FLASH_WRSR => {
                // The value is in `rd_status`'s low half — the ROM put it
                // there (`esp_rom_spiflash_write_status`). WIP stays 0 and
                // the write consumes WEL, as it does on the part.
                self.status = (self.regs.stored(RD_STATUS) & 0xffff) as u16 & !SR_WIP;
                self.status &= !SR_WEL;
            }
            CMD_FLASH_WREN => {
                self.flash.lock().unwrap().write_enables += 1;
                self.status |= SR_WEL;
            }
            CMD_FLASH_WRDI => self.status &= !SR_WEL,
            CMD_FLASH_SE => {
                let addr = self.flash_addr(cx);
                self.erase(addr, SECTOR_LEN, cx);
            }
            CMD_FLASH_BE => {
                let addr = self.flash_addr(cx);
                self.erase(addr, BLOCK_LEN, cx);
            }
            CMD_FLASH_CE => {
                self.flash.lock().unwrap().erase_chip();
                self.status &= !SR_WEL;
            }
            CMD_FLASH_PP => {
                let addr = self.flash_addr(cx);
                let len = self.dlen_bytes(MOSI_DLEN);
                self.program(addr, len, cx);
            }
            CMD_FLASH_READ => {
                let addr = self.flash_addr(cx);
                let len = self.dlen_bytes(MISO_DLEN);
                self.read_into_buffer(addr, len, cx);
            }
            other => self.refuse(other, cx),
        }
    }

    /// The `usr` engine: one transaction described by `user`/`user1`/`user2`.
    fn execute_usr(&mut self, cx: &mut BusCx<'_>) {
        let user = self.regs.stored(USER);
        let user2 = self.regs.stored(USER2);
        let cmd_bits = ((user2 >> 28) & 0xf) + 1;
        let command = (user2 & 0xffff) as u8;
        if user & USER_COMMAND == 0 {
            // No command phase: the ROM never issues one, and a transfer
            // with no opcode has no meaning for a NOR flash.
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} SPI1 usr with no command phase (user={user:#010x}); \
                 nothing was sent",
                cx.now, cx.pc
            ));
            return;
        }
        if cmd_bits != 8 {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} SPI1 usr command phase is {cmd_bits} bits, not 8; \
                 using the low 8 of {command:#04x}",
                cx.now, cx.pc
            ));
        }

        if op::READS.contains(&command) {
            if !self.phase(user, USER_MISO, "read-data", command, cx) {
                return;
            }
            self.phase(user, USER_ADDR, "address", command, cx);
            let addr = self.flash_addr(cx);
            let len = self.dlen_bytes(MISO_DLEN);
            self.read_into_buffer(addr, len, cx);
            return;
        }
        match command {
            op::PAGE_PROGRAM => {
                if !self.phase(user, USER_MOSI, "write-data", command, cx) {
                    return;
                }
                self.phase(user, USER_ADDR, "address", command, cx);
                let addr = self.flash_addr(cx);
                let len = self.dlen_bytes(MOSI_DLEN);
                self.program(addr, len, cx);
            }
            op::SECTOR_ERASE => {
                let addr = self.flash_addr(cx);
                self.erase(addr, SECTOR_LEN, cx);
            }
            op::BLOCK_ERASE_64K => {
                let addr = self.flash_addr(cx);
                self.erase(addr, BLOCK_LEN, cx);
            }
            op::WRITE_ENABLE => {
                self.flash.lock().unwrap().write_enables += 1;
                self.status |= SR_WEL;
            }
            op::WRITE_DISABLE => self.status &= !SR_WEL,
            op::READ_STATUS => {
                self.flash.lock().unwrap().status_reads += 1;
                // `esp_rom_spiflash_read_user_cmd` reads the answer out of
                // `w0`'s low byte, not out of `rd_status`.
                self.regs.poke(W0, u32::from(self.status & 0xff));
            }
            op::READ_STATUS_HIGH => {
                self.flash.lock().unwrap().status_reads += 1;
                self.regs.poke(W0, u32::from(self.status >> 8));
            }
            op::RDID => {
                let id = self.flash.lock().unwrap().jedec_id();
                self.regs.poke(W0, id);
            }
            other => {
                cx.trace.note(&format!(
                    "cyc={} pc={:#010x} SPI1 usr command {other:#04x} is not modelled; \
                     no bytes moved and the buffer is unchanged",
                    cx.now, cx.pc
                ));
            }
        }
    }

    /// Is the phase `bit` enables present? A command that needs a phase
    /// `user` did not enable moves nothing on the wire, and saying so is
    /// how a mis-programmed transfer is caught here rather than three
    /// layers up.
    fn phase(&self, user: u32, bit: u32, what: &str, command: u8, cx: &mut BusCx<'_>) -> bool {
        if user & bit != 0 {
            return true;
        }
        cx.trace.note(&format!(
            "cyc={} pc={:#010x} SPI1 usr command {command:#04x} with no {what} phase \
             (user={user:#010x})",
            cx.now, cx.pc
        ));
        false
    }

    fn read_into_buffer(&mut self, addr: u32, len: u32, cx: &mut BusCx<'_>) {
        let bytes = {
            let mut flash = self.flash.lock().unwrap();
            match flash.read(addr, len) {
                Some(slice) => slice.to_vec(),
                None => {
                    let chip = flash.len();
                    drop(flash);
                    cx.trace.note(&format!(
                        "cyc={} pc={:#010x} SPI1 read {len} bytes at {addr:#010x} leaves the \
                         {chip:#x}-byte chip; the buffer reads 0xff",
                        cx.now, cx.pc
                    ));
                    vec![0xff; len as usize]
                }
            }
        };
        self.set_buffer_bytes(&bytes);
    }

    fn program(&mut self, addr: u32, len: u32, cx: &mut BusCx<'_>) {
        if self.status & SR_WEL == 0 {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} SPI1 page program at {addr:#010x} with WEL clear; \
                 the part would ignore it, and so does this",
                cx.now, cx.pc
            ));
            return;
        }
        let data = self.buffer_bytes();
        let len = len.min(64) as usize;
        // A page program may not cross a 256-byte page: the part wraps
        // within the page instead, which is a bug the caller wants to see.
        if (addr % PAGE_LEN) as usize + len > PAGE_LEN as usize {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} SPI1 page program of {len} bytes at {addr:#010x} crosses a \
                 256-byte page boundary; the part would wrap, this model programs straight \
                 through",
                cx.now, cx.pc
            ));
        }
        if !self.flash.lock().unwrap().program(addr, &data[..len]) {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} SPI1 page program of {len} bytes at {addr:#010x} leaves \
                 the chip; nothing was written",
                cx.now, cx.pc
            ));
        }
        self.status &= !SR_WEL;
    }

    fn erase(&mut self, addr: u32, len: u32, cx: &mut BusCx<'_>) {
        if self.status & SR_WEL == 0 {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} SPI1 erase at {addr:#010x} with WEL clear; ignored",
                cx.now, cx.pc
            ));
            return;
        }
        // The part erases the granule the address falls in.
        let aligned = addr & !(len - 1);
        if !self.flash.lock().unwrap().erase(aligned, len) {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} SPI1 erase of {len} bytes at {aligned:#010x} leaves the \
                 chip; nothing was erased",
                cx.now, cx.pc
            ));
        }
        self.status &= !SR_WEL;
    }

    fn refuse(&mut self, bits: u32, cx: &mut BusCx<'_>) {
        if self.refused.contains(&bits) {
            return;
        }
        self.refused.push(bits);
        cx.trace.note(&format!(
            "cyc={} pc={:#010x} SPI1 unmodelled command cmd={bits:#010x}; the trigger bit is \
             cleared so the guest does not spin, and nothing was done",
            cx.now, cx.pc
        ));
    }
}

impl Peripheral for Spi1 {
    fn name(&self) -> &'static str {
        "SPI1"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        if off & !3 == CMD {
            // Every trigger has already completed and cleared; `mst_st` and
            // `slv_st` read 0 (idle), which is what `Wait_SPI_Idle` waits
            // for. So `cmd` always reads exactly zero.
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
        let triggered = word & CMD_TRIGGERS;
        if triggered != 0 {
            self.execute(triggered, cx);
        }
        // Whatever was triggered is done; the register is back to idle.
        self.regs.poke(CMD, 0);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::SPI1.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x400 + 2);
        out.extend_from_slice(&self.status.to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        if bytes.len() < 2 {
            log::warn!("SPI1::load_state: {} bytes is too short", bytes.len());
            return;
        }
        self.status = u16::from_le_bytes([bytes[0], bytes[1]]);
        self.regs.load_state(&bytes[2..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flash::{DEFAULT_FLASH_LEN, FlashImage};
    use lp_emu_esp_common::Sandbox;
    use std::sync::{Arc, Mutex};

    fn rig() -> (Sandbox, Spi1, FlashHandle) {
        let flash = Arc::new(Mutex::new(FlashImage::blank(DEFAULT_FLASH_LEN)));
        (Sandbox::new(), Spi1::new(flash.clone()), flash)
    }

    #[test]
    fn the_rdid_esp_storage_spins_on_completes_and_leaves_the_id_in_w0() {
        // `third_party/esp-storage/src/hardware.rs`, verbatim:
        //   spi1.cmd().write(|w| w.flash_rdid().set_bit());
        //   while spi1.cmd().read().flash_rdid().bit_is_set() {}
        //   spi1.w(0).read().buf().bits() & 0x00FF_FFFF
        // M3's `RegFile` remembered the bit and the guest spun for ever.
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_RDID);
        assert_eq!(sb.read(&mut spi, CMD), 0, "the trigger bit self-clears");
        assert_eq!(sb.read(&mut spi, W0) & 0x00ff_ffff, 0x0016_40ef);
        // And esp-storage's own decode of it: capacity byte 0x16 → 4 MiB.
        let [_, _, capacity, _] = (sb.read(&mut spi, W0) & 0x00ff_ffff).to_le_bytes();
        assert_eq!(1u32 << capacity, DEFAULT_FLASH_LEN);
    }

    #[test]
    fn wait_spi_idle_sees_an_idle_mst_st_and_a_clear_wip() {
        // `Wait_SPI_Idle` (`0x4002_42e8`): `while (cmd & 0xf) {}` then
        // `read_status` until WIP is clear. Both loops must terminate on the
        // first pass or nothing else in the ROM ever runs.
        let (mut sb, mut spi, _flash) = rig();
        assert_eq!(sb.read(&mut spi, CMD) & 0xf, 0);
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        assert_eq!(sb.read(&mut spi, RD_STATUS) & u32::from(SR_WIP), 0);
    }

    #[test]
    fn write_enable_sets_the_latch_the_rom_loops_until_it_sees() {
        // `_SPI_write_enable` (`0x4002_4630`) loops:
        //   sw s2,0(s0)      ; cmd = 1<<30
        //   ... read_status ; andi a5,2 ; beqz → round again
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        assert_eq!(
            sb.read(&mut spi, RD_STATUS) & u32::from(SR_WEL),
            u32::from(SR_WEL),
            "WEL must be set or the ROM spins here for ever"
        );
        sb.write(&mut spi, CMD, CMD_FLASH_WRDI);
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        assert_eq!(sb.read(&mut spi, RD_STATUS) & u32::from(SR_WEL), 0);
    }

    #[test]
    fn spi_read_data_moves_the_bytes_the_rom_copy_loop_expects() {
        // `SPI_read_data` (`0x4002_4100`): addr, miso_dlen = bits-1,
        // cmd = 1<<18, then whole words out of `w0..`.
        let (mut sb, mut spi, flash) = rig();
        flash
            .lock()
            .unwrap()
            .stage(0x31_0000, b"littlefs-ish header bytes");
        sb.write(&mut spi, USER, USER_COMMAND | USER_ADDR | USER_MISO);
        sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(op::READ));
        sb.write(&mut spi, ADDR, 0x31_0000);
        sb.write(&mut spi, MISO_DLEN, 8 * 8 - 1);
        sb.write(&mut spi, CMD, CMD_USR);
        assert_eq!(sb.read(&mut spi, CMD), 0);
        let w0 = sb.read(&mut spi, W0).to_le_bytes();
        let w1 = sb.read(&mut spi, W0 + 4).to_le_bytes();
        assert_eq!(&w0, b"litt");
        assert_eq!(&w1, b"lefs");
    }

    #[test]
    fn every_read_mode_the_rom_can_pick_moves_the_same_bytes() {
        // `_esp_rom_spiflash_read` chooses 0xeb/0xbb/0x6b/0x3b/0x0b/0x03 off
        // `ctrl`'s fread bits and only changes the phase widths.
        for command in op::READS {
            let (mut sb, mut spi, flash) = rig();
            flash.lock().unwrap().stage(0x1000, b"ABCD");
            sb.write(&mut spi, USER, USER_COMMAND | USER_ADDR | USER_MISO);
            sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(*command));
            sb.write(&mut spi, ADDR, 0x1000);
            sb.write(&mut spi, MISO_DLEN, 4 * 8 - 1);
            sb.write(&mut spi, CMD, CMD_USR);
            assert_eq!(
                sb.read(&mut spi, W0).to_le_bytes(),
                *b"ABCD",
                "read command {command:#04x}"
            );
        }
    }

    #[test]
    fn the_page_program_sequence_writes_and_consumes_the_latch() {
        // `SPI_page_program` (`0x4002_4692`): user2 = 0x70000002,
        // cmd = 1<<18 | 1<<17, after a `SPI_write_enable`.
        let (mut sb, mut spi, flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        sb.write(&mut spi, ADDR, 0x31_0000);
        sb.write(&mut spi, W0, u32::from_le_bytes(*b"lpfs"));
        sb.write(&mut spi, MOSI_DLEN, 4 * 8 - 1);
        sb.write(&mut spi, USER, USER_COMMAND | USER_ADDR | USER_MOSI);
        sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(op::PAGE_PROGRAM));
        sb.write(&mut spi, CMD, CMD_USR | CMD_FLASH_PE);
        assert_eq!(flash.lock().unwrap().peek(0x31_0000, 4).unwrap(), b"lpfs");
        assert_eq!(spi.status() & SR_WEL, 0, "the program consumed WEL");

        // Without a write-enable the part ignores the program, and so does
        // this: the second word must not land.
        sb.write(&mut spi, ADDR, 0x31_0010);
        sb.write(&mut spi, W0, u32::from_le_bytes(*b"nope"));
        sb.write(&mut spi, CMD, CMD_USR | CMD_FLASH_PE);
        assert_eq!(
            flash.lock().unwrap().peek(0x31_0010, 4).unwrap(),
            &[0xff; 4]
        );
    }

    #[test]
    fn the_dedicated_sector_erase_bit_erases_the_granule_the_address_is_in() {
        // `SPI_sector_erase` (`0x4002_4340`): addr, cmd = 1<<24 | 1<<17.
        let (mut sb, mut spi, flash) = rig();
        flash.lock().unwrap().stage(0x31_0800, b"stale");
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        // A byte inside the sector, not its base: the part aligns.
        sb.write(&mut spi, ADDR, 0x31_0800);
        sb.write(&mut spi, CMD, CMD_FLASH_SE | CMD_FLASH_PE);
        assert_eq!(
            flash.lock().unwrap().peek(0x31_0800, 5).unwrap(),
            &[0xff; 5]
        );
        assert_eq!(flash.lock().unwrap().sector_erases, 1);
    }

    #[test]
    fn write_status_carries_the_value_out_of_rd_status_and_keeps_wip_clear() {
        // `esp_rom_spiflash_write_status` (`0x4002_43f0`) puts the value in
        // `rd_status[15:0]` and triggers bit 26; `_esp_rom_spiflash_unlock`
        // is the caller, clearing the block-protect bits.
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        sb.write(&mut spi, RD_STATUS, 0x0000_00fd);
        sb.write(&mut spi, CMD, CMD_FLASH_WRSR);
        assert_eq!(spi.status() & SR_WIP, 0, "WIP is never set: writes finish");
        assert_eq!(spi.status() & SR_WEL, 0, "the write consumed WEL");
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        assert_eq!(sb.read(&mut spi, RD_STATUS) & 0xff, 0xfc);
    }

    #[test]
    fn read_user_cmd_answers_status_in_w0_the_way_the_rom_reads_it() {
        // `esp_rom_spiflash_read_user_cmd` (`0x4002_41b2`) reads `w0 & 0xff`,
        // and `esp_rom_spiflash_read_statushigh` sends 0x35 through it.
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        sb.write(&mut spi, USER, 0x9000_0000);
        sb.write(&mut spi, MISO_DLEN, 7);
        sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(op::READ_STATUS));
        sb.write(&mut spi, W0, 0);
        sb.write(&mut spi, CMD, CMD_USR);
        assert_eq!(sb.read(&mut spi, W0) & 0xff, u32::from(SR_WEL));

        sb.write(
            &mut spi,
            USER2,
            0x7000_0000 | u32::from(op::READ_STATUS_HIGH),
        );
        sb.write(&mut spi, CMD, CMD_USR);
        assert_eq!(sb.read(&mut spi, W0) & 0xff, 0, "no protection bits set");
    }

    #[test]
    fn an_unmodelled_trigger_clears_itself_and_says_so_once() {
        let mut sb = Sandbox::new();
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let flash = Arc::new(Mutex::new(FlashImage::blank(DEFAULT_FLASH_LEN)));
        let mut spi = Spi1::new(flash);
        sb.write(&mut spi, CMD, CMD_FLASH_DP);
        sb.write(&mut spi, CMD, CMD_FLASH_DP);
        assert_eq!(sb.read(&mut spi, CMD), 0, "the guest must not spin");
        let refusals = buf
            .lines()
            .into_iter()
            .filter(|l| l.contains("unmodelled command"))
            .count();
        assert_eq!(refusals, 1, "once per distinct trigger, not per write");
    }

    #[test]
    fn a_read_past_the_chip_reads_erased_bytes_and_says_so() {
        let mut sb = Sandbox::new();
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let flash = Arc::new(Mutex::new(FlashImage::blank(0x1000)));
        let mut spi = Spi1::new(flash);
        sb.write(&mut spi, USER, USER_COMMAND | USER_ADDR | USER_MISO);
        sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(op::READ));
        sb.write(&mut spi, ADDR, 0x0ffc);
        sb.write(&mut spi, MISO_DLEN, 16 * 8 - 1);
        sb.write(&mut spi, CMD, CMD_USR);
        assert_eq!(sb.read(&mut spi, W0), 0xffff_ffff);
        assert!(buf.contents().contains("leaves the"), "{}", buf.contents());
    }

    #[test]
    fn the_names_come_from_the_generated_table() {
        let (_, spi, _flash) = rig();
        assert_eq!(spi.reg_name(CMD), Some("cmd"));
        assert_eq!(spi.reg_name(RD_STATUS), Some("rd_status"));
        assert_eq!(spi.reg_name(W0), Some("w0"));
        assert_eq!(spi.reg_name(0x03c), Some("cache_fctrl"));
    }
}
