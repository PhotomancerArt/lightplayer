//! `SPI1` at `0x3FF4_2000` — the legacy flash controller, the block every
//! flash byte in this machine moves through.
//!
//! # Who drives it, and how
//!
//! Two callers, and between them they are the whole decode this model needs.
//!
//! **The mask ROM's `SPI_*` family**, read off the vendored ROM ELF
//! (`lp-emu/esp/roms/esp32_rev300_rom.elf`,
//! `xtensa-esp32-elf-objdump -d`); every address below is a real symbol in
//! it, and every literal is one the disassembly loads:
//!
//! | ROM function | what it writes to SPI1 |
//! |---|---|
//! | `SPI_read_status$isra$1` `0x4006_226C` | `rd_status`(`+0x10`) = 0, `cmd` = `1<<27` (`flash_rdsr`), spin `while cmd != 0`, read `rd_status`, loop while **WIP** (bit 0) |
//! | `Wait_SPI_Idle` `0x4006_22C0` | spins on `SPI1+0xf8` **and** `SPI0+0xf8` (`ext2.st`, bits 2:0), then `SPI_read_status` until WIP is clear |
//! | `SPI_write_status` `0x4006_22F0` | `rd_status` = value, `cmd` = `1<<26` (`flash_wrsr`), spin |
//! | `SPI_write_enable` `0x4006_2320` | `cmd` = `1<<30` (`flash_wren`), spin, then reads status until **WEL** (bit 1) is set |
//! | `SPI_page_program` `0x4006_2368` | `addr`, `w0..`, `mosi_dlen`, `user`, `user2` = `0x7000_0002`, `cmd` = `1<<18 \| 1<<17` |
//! | `SPI_user_command_read` `0x4006_21B0` | saves `ctrl`/`user`/`user1`/`user2`, `miso_dlen`(`+0x2c`) = 7, `user` = `0x9000_0000` (or `0xb000_0000` with a dummy phase), `user2` = `0x7000_0000\|cmd`, `w0` = 0, `cmd` = `1<<18`, reads `w0 & 0xff`, restores the four |
//!
//! **esp-storage's IRAM copies of the same routines.** The shipped image
//! links `esp_rom_spiflash_read` into its own `.rwtext` so a flash read can
//! run with the cache off (`docs/debt/classic-iram-handlers-reach-flash.md`),
//! and P3's tenth strict stop is inside that copy at `0x4008_3C98` — the
//! same register sequence, from a different address. That is why this model
//! is written against the *registers*, never against a pc.
//!
//! Three consequences shape the model, and they are the C6's three because
//! they are the part's:
//!
//! 1. **Every trigger bit is self-clearing and the operation completes
//!    inside the write.** Every spin above is `while cmd != 0`, and
//!    `Wait_SPI_Idle` additionally wants `ext2.st == 0`. A model that took a
//!    cycle to finish would need an event and would give the same answer,
//!    because nothing observes the interval — so [`Spi1`] completes
//!    synchronously and says so, rather than inventing a duration nobody
//!    measured. (Flash wait states are a cycle-model rung, not a peripheral
//!    one.)
//! 2. **The status register is real state.** WIP (bit 0) is always 0 because
//!    operations complete; WEL (bit 1) is set by `flash_wren`, cleared by
//!    `flash_wrdi` and by any completed program or erase. `SPI_write_enable`
//!    **loops until it sees WEL set**, so a model that left it 0 hangs the
//!    first write.
//! 3. **`usr` is a generic engine.** The ROM reaches read, page-program and
//!    every status command through it with the command byte in `user2`, so
//!    this models the engine (command / address / dummy / data phases from
//!    `user`, `user1`, `user2`, `addr`, `mosi_dlen`, `miso_dlen`,
//!    `w0..w15`) rather than the ROM's particular choices.
//!
//! # Where the classic's registers are, and where they are not
//!
//! The offsets diverge from the C6's **from `+0x10` on**, and the PAC gives
//! SPI0/SPI1/SPI2/SPI3 one `RegisterBlock` (`regs::SPI0`), so one table
//! serves both controllers:
//!
//! ```text
//! 0x00 cmd | 0x04 addr | 0x08 ctrl | 0x0c ctrl1 | 0x10 rd_status | 0x14 ctrl2
//! 0x18 clock | 0x1c user | 0x20 user1 | 0x24 user2 | 0x28 mosi_dlen
//! 0x2c miso_dlen | … | 0x80..0xc0 w0..w15 | 0xf8 ext2
//! ```
//!
//! The C6 has `rd_status` at `+0x2c` and `w0` at `+0x58`. Both are load
//! bearing: the ROM's status read writes `+0x10` and its buffer starts at
//! `+0x80`.
//!
//! `cmd`'s trigger bits are bit-for-bit the C6's from `usr` (18) up, with
//! **one difference**: the classic splits bit 16/17 into `flash_per` /
//! `flash_pes` where the C6 has a single `flash_pe` at 17. Both are taken
//! from the PAC's own doc lines (`esp32-0.40.2/src/spi0/cmd.rs`, "Bit 16 -
//! program erase resume", "Bit 17 - program erase suspend"), and neither
//! carries work of its own: `SPI_page_program` sets `1<<17` alongside `usr`
//! as a modifier.
//!
//! # What is *not* modelled, and what happens instead
//!
//! Bus width (`ctrl`'s `fread_*` bits), the clock divider, `ctrl2`'s CS
//! timing, `user`'s `usr_*_highpart` buffer-half selects, the DMA block and
//! the encryption path are accept-and-remember: the bytes that move do not
//! depend on them, and a model that pretended to honour them would be
//! inventing behaviour. A `cmd` trigger this table does not know is
//! **refused loudly** — the bit clears (so the guest does not hang) and a
//! `SPI1 unmodelled command` line goes into the trace naming the bits and
//! the pc.

use lp_emu_esp_common::engine::spi_flash::{self, FlashEngine, FlashOp, FlashOutcome, op};
use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::flash::{BLOCK_LEN, FlashHandle, SECTOR_LEN};
use crate::periph::accept;
use crate::regs;

/// This block's name — in its own trace lines and in the ones the engine
/// writes on its behalf, since the engine models a NOR flash and does not
/// know which controller is driving it.
const NAME: &str = "SPI1";

// Register offsets (`regs::SPI0`, generated from the PAC; the block type
// SPI0/SPI1/SPI2/SPI3 all share).
pub const CMD: u32 = 0x000;
pub const ADDR: u32 = 0x004;
pub const CTRL: u32 = 0x008;
pub const CTRL1: u32 = 0x00c;
/// ⚠️ `+0x10` on the classic, `+0x2c` on the C6.
pub const RD_STATUS: u32 = 0x010;
pub const CTRL2: u32 = 0x014;
pub const CLOCK: u32 = 0x018;
pub const USER: u32 = 0x01c;
pub const USER1: u32 = 0x020;
pub const USER2: u32 = 0x024;
pub const MOSI_DLEN: u32 = 0x028;
pub const MISO_DLEN: u32 = 0x02c;
/// ⚠️ `+0x80` on the classic, `+0x58` on the C6.
pub const W0: u32 = 0x080;
/// `w0..w15`: the 64-byte data buffer every transfer moves through.
pub const W_COUNT: u32 = 16;
/// `ext2` — bits 2:0 are `st`, the controller's state machine, which
/// `Wait_SPI_Idle` polls on **both** controllers. Read-only in the PAC and
/// reset 0, which is idle.
pub const EXT2: u32 = 0x0f8;

// `cmd` trigger bits (PAC `spi0::cmd`, bit numbers from its own doc lines).
/// ⚠️ The classic's own: "Bit 16 - program erase resume". The C6 has no
/// bit 16 here.
pub const CMD_FLASH_PER: u32 = 1 << 16;
/// "Bit 17 - program erase suspend" — the C6 calls its bit 17 `flash_pe`.
pub const CMD_FLASH_PES: u32 = 1 << 17;
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

/// Every bit `cmd` can be triggered with. Bits 15:0 are not fields at all on
/// this part (the PAC names nothing below 16), so a write to them is
/// remembered and does nothing.
pub const CMD_TRIGGERS: u32 = 0xffff_0000;

// `user` phase-enable bits (PAC `spi0::user`: "Bit 27 … write-data phase",
// "Bit 28 … read-data phase", "Bit 30 … address phase", "Bit 31 … command
// phase"). The same four numbers as the C6's.
const USER_MOSI: u32 = 1 << 27;
const USER_MISO: u32 = 1 << 28;
const USER_ADDR: u32 = 1 << 30;
const USER_COMMAND: u32 = 1 << 31;

/// `mosi_dlen` / `miso_dlen` carry **24 bits** on this part ("Bits 0:23 …
/// the register value shall be (bit_num-1)"), where the C6's field is ten.
/// The mask is the field's, and the byte count is then capped at the 64-byte
/// buffer.
const DLEN_MASK: u32 = 0x00ff_ffff;

// The flash part's status-register bits, as the ROM reads them: WIP (0) and
// WEL (1) belong to the SPI NOR command set, so they live with the chip in
// [`lp_emu_esp_common::engine::spi_flash`] and are re-exported here for the
// paths that already say `spi1::SR_WEL`.
pub use lp_emu_esp_common::engine::spi_flash::{SR_WEL, SR_WIP};

/// The legacy flash controller.
pub struct Spi1 {
    regs: RegFile,
    /// The chip on the other side, and the WIP/WEL latch the ROM spins on.
    /// Everything this block's `cmd` triggers ends up here.
    engine: FlashEngine,
    /// Every `cmd` trigger this model does not know, once each, so a boot
    /// that reaches one says so without flooding the log.
    refused: Vec<u32>,
}

impl core::fmt::Debug for Spi1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Spi1")
            .field("status", &self.engine.status())
            .finish_non_exhaustive()
    }
}

impl Spi1 {
    /// The register file is [`accept::spi`]'s — the same PAC-seeded
    /// `RegFile` P3 registered, so `accept.rs`'s reset-value sweep still
    /// covers every offset this block answers at.
    ///
    /// **`user`'s reset is the load-bearing one.** `0x8000_0040` has
    /// `usr_command` (bit 31) already set, and the flash read path never sets
    /// it: it only clears `usr_mosi` and sets `usr_miso`. A block that reset
    /// to zero would issue every read with no command phase and move no
    /// bytes — the C6's second-boot gate caught exactly that, and the
    /// symptom was `lpfs` reformatting on the second boot rather than
    /// anything that looked like a read failure.
    pub fn new(flash: FlashHandle) -> Self {
        let regs = accept::spi(NAME)
            .with_grade(CMD, RegGrade::Documented)
            .with_grade(RD_STATUS, RegGrade::Documented);
        Self {
            regs,
            engine: FlashEngine::new(flash),
            refused: Vec::new(),
        }
    }

    pub fn status(&self) -> u16 {
        self.engine.status()
    }

    /// The `w0..w15` buffer as bytes, little-endian per word — the layout
    /// the ROM's copy loops assume (whole words out of `0x3FF4_2080 + 4n`).
    fn buffer_bytes(&self) -> [u8; spi_flash::BUFFER_LEN] {
        let mut out = [0u8; spi_flash::BUFFER_LEN];
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

    /// The flash byte address the address phase carries. The masking and its
    /// diagnostic are the engine's; the *offset* of `addr` is this block's.
    fn flash_addr(&mut self, cx: &mut BusCx<'_>) -> u32 {
        FlashEngine::flash_addr(self.regs.stored(ADDR), NAME, cx)
    }

    /// Bytes = `dlen + 1` bits, rounded up, capped at the 64-byte buffer.
    fn dlen_bytes(&self, off: u32) -> u32 {
        let bits = (self.regs.stored(off) & DLEN_MASK) + 1;
        bits.div_ceil(8).min(spi_flash::BUFFER_LEN as u32)
    }

    /// Hand one decoded transaction to the engine and put back what belongs
    /// in this block's registers. The engine never touches `w0..w15`: it
    /// does not know where they are.
    fn run(&mut self, op: FlashOp, cx: &mut BusCx<'_>) -> FlashOutcome {
        let mut buffer = match op {
            FlashOp::Program { .. } => self.buffer_bytes(),
            _ => [0u8; spi_flash::BUFFER_LEN],
        };
        let outcome = self.engine.execute(op, &mut buffer, NAME, cx);
        if outcome == FlashOutcome::Buffer {
            self.set_buffer_bytes(&buffer);
        }
        outcome
    }

    /// Run whatever `cmd` was just triggered with, and clear the bits.
    fn execute(&mut self, triggered: u32, cx: &mut BusCx<'_>) {
        // `usr` is the generic engine and the dedicated bits are shortcuts;
        // `flash_per`/`flash_pes` are modifiers ("this transfer is a program
        // or an erase") and carry no work of their own.
        let dedicated = triggered & !(CMD_USR | CMD_FLASH_PER | CMD_FLASH_PES);
        if triggered & CMD_USR != 0 {
            self.execute_usr(cx);
        }
        match dedicated {
            0 => {}
            CMD_FLASH_RDID => {
                if let FlashOutcome::Word(id) = self.run(FlashOp::ReadId, cx) {
                    self.regs.poke(W0, id);
                }
            }
            CMD_FLASH_RDSR => {
                if let FlashOutcome::Status(status) = self.run(FlashOp::ReadStatus, cx) {
                    // The ROM zeroes `rd_status` first and then reads the
                    // whole word back, masking with `chip->status_mask`.
                    let kept = self.regs.stored(RD_STATUS) & 0xffff_0000;
                    self.regs.poke(RD_STATUS, kept | u32::from(status));
                }
            }
            CMD_FLASH_WRSR => {
                // `SPI_write_status` put the value in `rd_status`'s low half.
                let value = (self.regs.stored(RD_STATUS) & 0xffff) as u16;
                self.run(FlashOp::WriteStatus(value), cx);
            }
            CMD_FLASH_WREN => {
                self.run(FlashOp::WriteEnable, cx);
            }
            CMD_FLASH_WRDI => {
                self.run(FlashOp::WriteDisable, cx);
            }
            CMD_FLASH_SE => {
                let addr = self.flash_addr(cx);
                self.run(
                    FlashOp::Erase {
                        addr,
                        len: SECTOR_LEN,
                    },
                    cx,
                );
            }
            CMD_FLASH_BE => {
                let addr = self.flash_addr(cx);
                self.run(
                    FlashOp::Erase {
                        addr,
                        len: BLOCK_LEN,
                    },
                    cx,
                );
            }
            CMD_FLASH_CE => {
                self.run(FlashOp::EraseChip, cx);
            }
            CMD_FLASH_PP => {
                let addr = self.flash_addr(cx);
                let len = self.dlen_bytes(MOSI_DLEN);
                self.run(FlashOp::Program { addr, len }, cx);
            }
            CMD_FLASH_READ => {
                let addr = self.flash_addr(cx);
                let len = self.dlen_bytes(MISO_DLEN);
                self.run(FlashOp::Read { addr, len }, cx);
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
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} {NAME} usr with no command phase (user={user:#010x}); \
                 nothing was sent",
                cx.now, cx.pc
            ));
            return;
        }
        if cmd_bits != 8 {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} {NAME} usr command phase is {cmd_bits} bits, not 8; \
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
            self.run(FlashOp::Read { addr, len }, cx);
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
                self.run(FlashOp::Program { addr, len }, cx);
            }
            op::SECTOR_ERASE => {
                let addr = self.flash_addr(cx);
                self.run(
                    FlashOp::Erase {
                        addr,
                        len: SECTOR_LEN,
                    },
                    cx,
                );
            }
            op::BLOCK_ERASE_64K => {
                let addr = self.flash_addr(cx);
                self.run(
                    FlashOp::Erase {
                        addr,
                        len: BLOCK_LEN,
                    },
                    cx,
                );
            }
            op::WRITE_ENABLE => {
                self.run(FlashOp::WriteEnable, cx);
            }
            op::WRITE_DISABLE => {
                self.run(FlashOp::WriteDisable, cx);
            }
            op::READ_STATUS => {
                if let FlashOutcome::Status(status) = self.run(FlashOp::ReadStatus, cx) {
                    // `SPI_user_command_read` reads the answer out of `w0`'s
                    // low byte, not out of `rd_status`.
                    self.regs.poke(W0, u32::from(status & 0xff));
                }
            }
            op::READ_STATUS_HIGH => {
                if let FlashOutcome::Status(status) = self.run(FlashOp::ReadStatus, cx) {
                    self.regs.poke(W0, u32::from(status >> 8));
                }
            }
            op::RDID => {
                if let FlashOutcome::Word(id) = self.run(FlashOp::ReadId, cx) {
                    self.regs.poke(W0, id);
                }
            }
            other => {
                if self.run(FlashOp::Unknown(other), cx) == FlashOutcome::Unknown {
                    cx.trace.note(&format!(
                        "cyc={} pc={:#010x} {NAME} usr command {other:#04x} is not modelled; \
                         no bytes moved and the buffer is unchanged",
                        cx.now, cx.pc
                    ));
                }
            }
        }
    }

    /// Is the phase `bit` enables present? A command that needs a phase
    /// `user` did not enable moves nothing on the wire, and saying so is how
    /// a mis-programmed transfer is caught here rather than three layers up.
    fn phase(&self, user: u32, bit: u32, what: &str, command: u8, cx: &mut BusCx<'_>) -> bool {
        if user & bit != 0 {
            return true;
        }
        cx.trace.note(&format!(
            "cyc={} pc={:#010x} {NAME} usr command {command:#04x} with no {what} phase \
             (user={user:#010x})",
            cx.now, cx.pc
        ));
        false
    }

    fn refuse(&mut self, bits: u32, cx: &mut BusCx<'_>) {
        if self.refused.contains(&bits) {
            return;
        }
        self.refused.push(bits);
        cx.trace.note(&format!(
            "cyc={} pc={:#010x} {NAME} unmodelled command cmd={bits:#010x}; the trigger bit is \
             cleared so the guest does not spin, and nothing was done",
            cx.now, cx.pc
        ));
    }
}

impl Peripheral for Spi1 {
    fn name(&self) -> &'static str {
        NAME
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        if off & !3 == CMD {
            // Every trigger has already completed and cleared, so `cmd`
            // reads exactly zero — which is what every one of the ROM's
            // `while cmd != 0` spins waits for.
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
        regs::SPI0.name(off)
    }

    /// The block's grades, from the PAC, with two hand-written rows —
    /// [`RegFile::with_pac_grades`] cannot see a register this block
    /// intercepts in its own [`Peripheral::read`] and [`Peripheral::write`].
    ///
    /// | register | grade | source |
    /// |---|---|---|
    /// | `cmd` +0x000 | `documented` | PAC `spi0::cmd`: every trigger bit is documented as cleared once the operation is done. This block completes inside the write, so `cmd` reads exactly 0 — which is the state `SPI_read_status` (`0x4006_226C`) and `SPI_write_enable` (`0x4006_2320`) both spin for. The *duration* is a timing claim this block does not make. |
    /// | `rd_status` +0x010 | `documented` | The flash part's status register, not the SoC's: WIP (bit 0) and WEL (bit 1) are the SPI NOR command set's, and `SPI_write_enable` loops until it reads WEL set — a model that left it clear would hang the first write. Bits above WEL are whatever `flash_wrsr` last wrote, because the unlock path writes block-protect bits and reads them back. |
    ///
    /// Nothing here is `measured`: that needs a committed silicon transcript
    /// naming the register, and this repository's classic captures are UART
    /// ones.
    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.regs.reg_grade(off)
    }

    /// The engine's status latch, then this block's register file.
    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(accept::SPI_LEN as usize + 2);
        self.engine.save_status(&mut out);
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let Some((status, n)) = FlashEngine::load_status(bytes) else {
            log::warn!("SPI1::load_state: {} bytes is too short", bytes.len());
            return;
        };
        self.engine.restore(status);
        self.regs.load_state(&bytes[n..]);
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
    fn the_pac_resets_this_block_comes_up_with_are_the_classics() {
        let (mut sb, mut spi, _flash) = rig();
        // `regs::SPI0`'s own `resets` table, at the classic's offsets.
        assert_eq!(sb.read(&mut spi, USER), 0x8000_0040);
        assert_eq!(sb.read(&mut spi, USER1), 0x5c00_0007);
        assert_eq!(sb.read(&mut spi, USER2), 0x7000_0000);
        assert_eq!(sb.read(&mut spi, CTRL), 0x0020_a400);
        assert_eq!(sb.read(&mut spi, CTRL1), 0x5fff_0000);
        assert_eq!(sb.read(&mut spi, CTRL2), 0x0000_0011);
        assert_eq!(sb.read(&mut spi, CLOCK), 0x8000_3043);
        // `ext2.st == 0` is idle, and `Wait_SPI_Idle` polls it first.
        assert_eq!(sb.read(&mut spi, EXT2) & 0b111, 0);
    }

    #[test]
    fn the_status_read_the_rom_spins_on_completes_and_lands_in_rd_status() {
        // `SPI_read_status$isra$1` (`0x4006_226C`):
        //   s32i a12(0),  [0x3ff42010]   ; rd_status = 0
        //   s32i a13(1<<27), [0x3ff42000] ; cmd = flash_rdsr
        //   l32i a8, [0x3ff42000]; bnez a8 → round again
        //   l32i a14, [0x3ff42010]; and with status_mask; bbsi a8,0 → loop
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, RD_STATUS, 0);
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        assert_eq!(sb.read(&mut spi, CMD), 0, "the trigger bit self-clears");
        assert_eq!(
            sb.read(&mut spi, RD_STATUS) & u32::from(SR_WIP),
            0,
            "WIP must be clear or the ROM spins here for ever"
        );
    }

    #[test]
    fn write_enable_sets_the_latch_the_rom_loops_until_it_sees() {
        // `SPI_write_enable` (`0x4006_2320`): cmd = 1<<30, spin, then read
        // status until bit 1 is set.
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        assert_eq!(
            sb.read(&mut spi, RD_STATUS) & u32::from(SR_WEL),
            u32::from(SR_WEL)
        );
        sb.write(&mut spi, CMD, CMD_FLASH_WRDI);
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        assert_eq!(sb.read(&mut spi, RD_STATUS) & u32::from(SR_WEL), 0);
    }

    #[test]
    fn write_status_carries_the_value_out_of_rd_status_and_keeps_wip_clear() {
        // `SPI_write_status` (`0x4006_22F0`): rd_status = value, cmd = 1<<26.
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
    fn a_usr_read_moves_the_bytes_the_roms_copy_loop_expects() {
        // The read path: `user` keeps its reset `usr_command`, clears
        // `usr_mosi` and sets `usr_miso`; `user1`/`user2` carry the address
        // and dummy lengths and the command byte; `miso_dlen` the bit count.
        let (mut sb, mut spi, flash) = rig();
        flash.lock().unwrap().stage(0x0031_0000, b"littlefs");
        let user = sb.read(&mut spi, USER);
        sb.write(&mut spi, USER, (user & !USER_MOSI) | USER_MISO | USER_ADDR);
        sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(op::READ));
        sb.write(&mut spi, ADDR, 0x0031_0000);
        sb.write(&mut spi, MISO_DLEN, 8 * 8 - 1);
        sb.write(&mut spi, CMD, CMD_USR);
        assert_eq!(sb.read(&mut spi, CMD), 0);
        assert_eq!(sb.read(&mut spi, W0).to_le_bytes(), *b"litt");
        assert_eq!(sb.read(&mut spi, W0 + 4).to_le_bytes(), *b"lefs");
    }

    #[test]
    fn every_read_mode_the_rom_can_pick_moves_the_same_bytes() {
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
        // `SPI_page_program` (`0x4006_2368`): user2 = 0x70000002,
        // cmd = 1<<18 | 1<<17, after a write-enable.
        let (mut sb, mut spi, flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        sb.write(&mut spi, ADDR, 0x0031_0000);
        sb.write(&mut spi, W0, u32::from_le_bytes(*b"lpfs"));
        sb.write(&mut spi, MOSI_DLEN, 4 * 8 - 1);
        sb.write(&mut spi, USER, USER_COMMAND | USER_ADDR | USER_MOSI);
        sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(op::PAGE_PROGRAM));
        sb.write(&mut spi, CMD, CMD_USR | CMD_FLASH_PES);
        assert_eq!(
            flash.lock().unwrap().peek(0x0031_0000, 4).unwrap(),
            b"lpfs"
        );
        assert_eq!(spi.status() & SR_WEL, 0, "the program consumed WEL");

        // Without a write-enable the part ignores the program, and so does
        // this.
        sb.write(&mut spi, ADDR, 0x0031_0010);
        sb.write(&mut spi, W0, u32::from_le_bytes(*b"nope"));
        sb.write(&mut spi, CMD, CMD_USR | CMD_FLASH_PES);
        assert_eq!(
            flash.lock().unwrap().peek(0x0031_0010, 4).unwrap(),
            &[0xff; 4]
        );
    }

    #[test]
    fn the_dedicated_erase_bits_erase_the_granule_the_address_is_in() {
        let (mut sb, mut spi, flash) = rig();
        flash.lock().unwrap().stage(0x0031_0800, b"stale");
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        // A byte inside the sector, not its base: the part aligns.
        sb.write(&mut spi, ADDR, 0x0031_0800);
        sb.write(&mut spi, CMD, CMD_FLASH_SE | CMD_FLASH_PES);
        assert_eq!(
            flash.lock().unwrap().peek(0x0031_0800, 5).unwrap(),
            &[0xff; 5]
        );
        assert_eq!(flash.lock().unwrap().sector_erases, 1);
    }

    #[test]
    fn rdid_leaves_the_jedec_id_in_w0_where_esp_storage_reads_it() {
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_RDID);
        assert_eq!(sb.read(&mut spi, CMD), 0);
        let id = sb.read(&mut spi, W0) & 0x00ff_ffff;
        assert_eq!(id, 0x0016_40ef);
        let [_, _, capacity, _] = id.to_le_bytes();
        assert_eq!(1u32 << capacity, DEFAULT_FLASH_LEN);
    }

    #[test]
    fn read_user_cmd_answers_status_in_w0_the_way_the_rom_reads_it() {
        // `SPI_user_command_read` (`0x4006_21B0`): miso_dlen = 7,
        // user = 0x9000_0000, user2 = 0x7000_0000|cmd, w0 = 0, cmd = 1<<18,
        // then `w0 & 0xff`.
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        sb.write(&mut spi, MISO_DLEN, 7);
        sb.write(&mut spi, USER, 0x9000_0000);
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
    fn the_dlen_field_is_twenty_four_bits_wide_on_this_part() {
        // The C6's field is ten bits; the classic's is "Bits 0:23". A model
        // that masked with the C6's 0x3ff would ask for the wrong length on
        // any transfer longer than 128 bytes — which the buffer caps anyway,
        // so the *cap*, not the mask, is what makes both safe. Both are
        // asserted so a change to either is visible.
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, MISO_DLEN, 0x00ff_ffff);
        assert_eq!(spi.dlen_bytes(MISO_DLEN), spi_flash::BUFFER_LEN as u32);
        sb.write(&mut spi, MISO_DLEN, 0x0000_07ff);
        assert_eq!(spi.dlen_bytes(MISO_DLEN), spi_flash::BUFFER_LEN as u32);
        sb.write(&mut spi, MISO_DLEN, 4 * 8 - 1);
        assert_eq!(spi.dlen_bytes(MISO_DLEN), 4);
    }

    #[test]
    fn the_names_come_from_the_generated_table_at_the_classics_offsets() {
        let (_, spi, _flash) = rig();
        assert_eq!(spi.reg_name(CMD), Some("cmd"));
        assert_eq!(spi.reg_name(RD_STATUS), Some("rd_status"));
        assert_eq!(spi.reg_name(W0), Some("w0"));
        assert_eq!(spi.reg_name(EXT2), Some("ext2"));
        assert_eq!(spi.reg_name(0x050), Some("cache_fctrl"));
    }
}
