//! `SPI1` at `0x6000_2000` — the flash controller, the block every flash
//! byte in this machine moves through.
//!
//! ⚠️ **`SPI1` is at `0x6000_2000` on this chip and `SPI0` at `0x6000_3000`
//! — the other way round from the C6** (`m6/notes.md` §3.0 row 13). This
//! file is ported from the C6's by register *name* against
//! [`crate::regs::SPI1`], never by address.
//!
//! # Who drives it, and how
//!
//! Two callers, and between them they are the whole decode this model needs.
//!
//! **`esp_storage::get_flash_size()`** drives the block directly
//! (`third_party/esp-storage/src/hardware.rs:52-55`): `cmd.flash_rdid`,
//! spin until it clears, read `w0 & 0x00ff_ffff`. That write is where the
//! P05 hello stopped — `cyc=47999999 pc=0x4207ea69 R4 SPI1+0x000 cmd =
//! 0x10000000`, a `RegFile` remembering bit 28 for ever.
//!
//! **The mask ROM's `esp_rom_spiflash_*` family** does everything else, and
//! esp-storage reaches it through `esp_rom_sys` (`ll.rs`). The sequences
//! below are read off the vendored ROM ELF
//! (`lp-emu/esp/roms/esp32s3_rev0_rom.elf`, `xtensa-esp32s3-elf-objdump -d`);
//! every address is a real symbol in it and every literal is one the
//! disassembly loads:
//!
//! | ROM function | what it writes to SPI1 |
//! |---|---|
//! | `SPI_read_data` `0x4004_98d0` | per chunk: `miso_dlen`(`+0x28`) = bits−1, **`addr`(`+0x04`) = the plain byte address** (`400498c0: l32r a13, 60002004` / `4004990d: s32i.n a3, a13, 0`), `cmd` = `0x40000` (`usr`), spin `while cmd != 0` (`40049914..19`), copy words out of `w0..` (`+0x58`) |
//! | `_esp_rom_spiflash_read` `0x4004_abf0` | `user` &= ~`usr_mosi` (`f7ffffff`), \|= `0x7000_0000` (addr/dummy/miso); `user1` dummy length; `user2` = `0x7000_00eb`/`6b`/`3b`/`0b`/`03` by `ctrl`'s read mode; then `SPI_read_data` |
//! | `SPI_page_program` `0x4004_9f84` | `addr` = plain byte address (`40049fb6: s32i.n a3, a8(=60002004), 0`), `mosi_dlen` = `(bits−1) & 0x3ff` (`40049fcd: extui a8, a8, 0, 10`), `w0..`, `user` \|= `0xc800_0000` (command/addr/mosi) &= `0xcfff_ffff` (no dummy/miso), `user2` = `0x7000_0002`, `cmd` = `0x60000` (`usr` \| `flash_pe`), spin, `Wait_SPI_Idle` |
//! | `SPI_sector_erase` `0x4004_9b98` | `addr` = plain byte address (`40049bb2`), `cmd` = `0x0102_0000` (`flash_se` \| `flash_pe`, `40049bb7: l32r a3, 1020000`), spin |
//! | `_SPI_write_enable` `0x4004_9f14` | `cmd` = `0x4000_0000` (`flash_wren`), spin, then `esp_rom_spiflash_read_status` until **WEL** (bit 1) is set (`40049f56: bnone a3, a8`) |
//! | `esp_rom_spiflash_read_status` `0x4004_9a78` | `rd_status`(`+0x2c`) &= `0xffff_0000`, `cmd` = `0x0800_0000` (`flash_rdsr`), spin, `status = rd_status & chip->status_mask`, loop while **WIP** (bit 0) (`40049ada: bbsi a9, 0`) |
//! | `esp_rom_spiflash_write_status` `0x4004_9c40` | `rd_status[15:0]` = value, `cmd` = `0x0400_0000` (`flash_wrsr`), spin |
//! | `esp_rom_spiflash_read_user_cmd` `0x4004_99b4` | saves `ctrl`/`user`/`user1`/`user2`; `miso_dlen` = 7, `user` = `0x9000_0000` (or `0xb000_0000` with a dummy phase), `user2` = `0x7000_0000 \| cmd`, `w0` = 0, `cmd` = `0x40000`, spin, reads `w0 & 0xff`; restores the four |
//! | `Wait_SPI_Idle` `0x4004_9b30` | spins on **`fsm`(`+0x54`) bits 2:0** here (`40049b28: 60002054`) **and** on `SPI0+0x54`, then `read_status` until WIP is clear |
//!
//! Three consequences shape the model, and they are the C6's three because
//! they are the part's:
//!
//! 1. **Every trigger bit is self-clearing and the operation completes
//!    inside the write.** Every spin above is `while cmd != 0`, and
//!    `Wait_SPI_Idle` additionally wants `fsm.st == 0` (PAC: "0: idle
//!    state(IDLE)"). A model that took a cycle to finish would need an event
//!    and would give the same answer, because nothing observes the interval
//!    — so [`Spi1`] completes synchronously and says so. (Flash wait states
//!    are a cycle-model rung, not a peripheral one; `t1` is the only grade.)
//! 2. **The status register is real state.** WIP (bit 0) is always 0 because
//!    operations complete; WEL (bit 1) is set by `flash_wren`, cleared by
//!    `flash_wrdi` and by any completed program or erase. `_SPI_write_enable`
//!    **loops until it sees WEL set**, so a model that left it 0 hangs the
//!    first write.
//! 3. **`usr` is a generic engine.** The ROM reaches read, page-program and
//!    the status commands through it with the command byte in `user2`, so
//!    this models the engine (command / address / dummy / data phases from
//!    `user`, `user1`, `user2`, `addr`, `mosi_dlen`, `miso_dlen`, `w0..w15`)
//!    rather than the ROM's particular choices.
//!
//! # ⚠️ `addr` carries the plain byte address — the C6's convention, not the classic's
//!
//! The classic's `usr` engine left-justifies the address into bits 31:8 and
//! its dedicated triggers pack a byte count into bits 31:24
//! (`lp-emu-esp32v3/src/periph/spi1.rs`). The S3 ROM does neither: every
//! store to `+0x04` quoted above is the byte address itself, and
//! `esp_rom_spi_set_address_bit_len` (`0x4004_955c`) puts the phase width
//! in `user1` bits 31:26. Which convention applies is a ROM fact per chip;
//! this one is the C6's, and the test
//! `the_usr_address_phase_carries_the_plain_byte_address` holds it.
//!
//! # Where the S3's registers differ from the C6's
//!
//! `cmd..rd_status` (`0x00..0x2c`) and `w0..w15` (`0x58..0x98`) are at the
//! C6's offsets and this block reads them off [`crate::regs::SPI1`]. What
//! moved does not carry work: `int_ena/clr/raw/st` are `0xf0/0xf4/0xf8/0xfc`
//! (C6 `0xc0..0xcc`), `timing_cali` `0xa8` (C6 `0x180`), `ddr` `0xe0`,
//! `clock_gate` `0xe8` (C6 `0x200`), and the S3 adds `ext_addr` `0x30` and
//! `fsm` `0x54`. All of them are accept-and-remember at the PAC's resets;
//! `fsm` is read-only and reads its reset 0, which is exactly the idle
//! `Wait_SPI_Idle` polls for.
//!
//! # What is *not* modelled, and what happens instead
//!
//! Bus width (`ctrl`'s `fread_*` bits), the clock divider, `ctrl2`'s CS
//! timing, `user`'s `usr_*_highpart` buffer-half selects, the DMA block, the
//! suspend/resume path and the encryption path are accept-and-remember: the
//! bytes that move do not depend on them, and a model that pretended to
//! honour them would be inventing behaviour. A `cmd` trigger this table does
//! not know is **refused loudly** — the bit clears (so the guest does not
//! hang) and a `SPI1 unmodelled command` line goes into the trace naming the
//! bits and the pc.

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

// Register offsets (`regs::SPI1`, generated from the PAC).
pub const CMD: u32 = 0x000;
pub const ADDR: u32 = 0x004;
pub const CTRL: u32 = 0x008;
pub const CTRL1: u32 = 0x00c;
pub const CTRL2: u32 = 0x010;
pub const CLOCK: u32 = 0x014;
pub const USER: u32 = 0x018;
pub const USER1: u32 = 0x01c;
pub const USER2: u32 = 0x020;
pub const MOSI_DLEN: u32 = 0x024;
pub const MISO_DLEN: u32 = 0x028;
pub const RD_STATUS: u32 = 0x02c;
/// `fsm` — bits 2:0 are `st`, the controller's state machine, which
/// `Wait_SPI_Idle` polls on **both** controllers. Read-only in the PAC and
/// reset 0, which is idle.
pub const FSM: u32 = 0x054;
pub const W0: u32 = 0x058;
/// `w0..w15`: the 64-byte data buffer every transfer moves through.
pub const W_COUNT: u32 = 16;

// `cmd` trigger bits (PAC `spi1::cmd`, `esp32s3-0.35.2/src/spi1/cmd.rs`,
// bit numbers from its own doc lines: "Bit 17 - In user mode, it is set to
// indicate that program/erase operation will be triggered" … "Bit 31 - Read
// flash enable").
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

/// Every bit `cmd` can be triggered with: bits 17..=31. The PAC names
/// nothing below 17 on this part, so a write to those bits is remembered
/// and does nothing.
pub const CMD_TRIGGERS: u32 = 0xfffe_0000;

// `user` phase-enable bits (PAC `spi1::user`: "Bit 27 - … DOUT phase",
// "Bit 28 - … DIN phase", "Bit 29 - … DUMMY phase", "Bit 30 - … ADDR
// phase", "Bit 31 - … CMD phase").
const USER_MOSI: u32 = 1 << 27;
const USER_MISO: u32 = 1 << 28;
const USER_ADDR: u32 = 1 << 30;
const USER_COMMAND: u32 = 1 << 31;

/// `mosi_dlen` / `miso_dlen` carry **10 bits** on this part ("Bits 0:9 -
/// The length in bits of DOUT phase. The register value shall be
/// (bit_num-1)"), and `SPI_page_program` masks with exactly that
/// (`40049fcd: extui a8, a8, 0, 10`). The byte count is then capped at the
/// 64-byte buffer.
const DLEN_MASK: u32 = 0x3ff;

// The flash part's status-register bits, as the ROM reads them: WIP (0) and
// WEL (1) belong to the SPI NOR command set, so they live with the chip in
// [`lp_emu_esp_common::engine::spi_flash`] and are re-exported here for the
// paths that already say `spi1::SR_WEL`.
pub use lp_emu_esp_common::engine::spi_flash::{SR_WEL, SR_WIP};

/// The flash controller.
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
    /// The register file is [`accept::spi1`]'s — the same PAC-seeded
    /// `RegFile` P04 registered, so `accept.rs`'s reset-value sweep still
    /// covers every offset this block answers at.
    ///
    /// **`user`'s reset is the load-bearing one.** `0x8000_0000` has
    /// `usr_command` (bit 31) already set, and the ROM's read path never
    /// sets it: `_esp_rom_spiflash_read` only clears `usr_mosi` and ORs in
    /// `0x7000_0000`. A block that reset to zero would issue every read with
    /// no command phase and move no bytes — the C6's second-boot gate caught
    /// exactly that, and the symptom was `lpfs` reformatting on the second
    /// boot rather than anything that looked like a read failure.
    pub fn new(flash: FlashHandle) -> Self {
        let regs = accept::spi1()
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
    /// the ROM's copy loops assume (whole words out of `0x6000_2058 + 4n`,
    /// `SPI_read_data` `40049921..2d`).
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

    /// The flash byte address the address phase carries: the plain byte
    /// address in `addr` (module docs). Bits above 24 are mode bits, and the
    /// engine masks and reports them.
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
        // `flash_pe` is a modifier ("this transfer is a program or an
        // erase") and carries no work of its own.
        let dedicated = triggered & !(CMD_USR | CMD_FLASH_PE);
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
                    // The ROM clears `rd_status`'s low half first and then
                    // reads the whole word back, masking with
                    // `chip->status_mask`.
                    let kept = self.regs.stored(RD_STATUS) & 0xffff_0000;
                    self.regs.poke(RD_STATUS, kept | u32::from(status));
                }
            }
            CMD_FLASH_WRSR => {
                // `esp_rom_spiflash_write_status` put the value in
                // `rd_status`'s low half.
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
                    // `esp_rom_spiflash_read_user_cmd` reads the answer out
                    // of `w0`'s low byte, not out of `rd_status`.
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
            // `while cmd != 0` spins waits for, and what esp-storage's
            // `while cmd.flash_rdid()` waits for.
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

    /// The block's grades, from the PAC, with two hand-written rows —
    /// [`RegFile::with_pac_grades`] cannot see a register this block
    /// intercepts in its own [`Peripheral::read`] and [`Peripheral::write`].
    ///
    /// | register | grade | source |
    /// |---|---|---|
    /// | `cmd` +0x000 | `documented` | PAC `spi1::cmd`: every trigger bit is documented as cleared once the operation is done. This block completes inside the write, so `cmd` reads exactly 0 — the state `SPI_read_data` (`0x4004_98d0`) and `_SPI_write_enable` (`0x4004_9f14`) both spin for. The *duration* is a timing claim this block does not make. |
    /// | `rd_status` +0x02c | `documented` | The flash part's status register, not the SoC's: WIP (bit 0) and WEL (bit 1) are the SPI NOR command set's, and `_SPI_write_enable` loops until it reads WEL set — a model that left it clear would hang the first write. Bits above WEL are whatever `flash_wrsr` last wrote, because `_esp_rom_spiflash_unlock` (`0x4004_a76c`) writes block-protect bits and reads them back. |
    ///
    /// Nothing here is `measured`: that needs a committed silicon transcript
    /// naming the register, and no S3 silicon has been read yet (P09).
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

    /// `esp_storage::get_flash_size` — the write the P05 hello spun on.
    #[test]
    fn the_rdid_esp_storage_spins_on_completes_and_leaves_the_id_in_w0() {
        // `third_party/esp-storage/src/hardware.rs:52-55`, verbatim:
        //   spi1.cmd().write(|w| w.flash_rdid().set_bit());
        //   while spi1.cmd().read().flash_rdid().bit_is_set() {}
        //   spi1.w(0).read().buf().bits() & 0x00FF_FFFF
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_RDID);
        assert_eq!(sb.read(&mut spi, CMD), 0, "the trigger bit self-clears");
        assert_eq!(sb.read(&mut spi, W0) & 0x00ff_ffff, 0x0017_40ef);
        // esp-storage's own decode of it: capacity byte 0x17 → 8 MiB.
        let [_, _, capacity, _] = (sb.read(&mut spi, W0) & 0x00ff_ffff).to_le_bytes();
        assert_eq!(1u32 << capacity, DEFAULT_FLASH_LEN);
    }

    #[test]
    fn the_pac_resets_this_block_comes_up_with_are_the_s3s() {
        let (mut sb, mut spi, _flash) = rig();
        // `regs::SPI1`'s own `resets` table.
        assert_eq!(sb.read(&mut spi, USER), 0x8000_0000);
        assert_eq!(sb.read(&mut spi, USER1), 0x5c00_0007);
        assert_eq!(sb.read(&mut spi, USER2), 0x7000_0000);
        assert_eq!(sb.read(&mut spi, CTRL), 0x002c_a000);
        assert_eq!(sb.read(&mut spi, CTRL1), 0x0000_0ffc);
        assert_eq!(sb.read(&mut spi, CLOCK), 0x0003_0103);
        // `fsm.st == 0` is idle, and `Wait_SPI_Idle` polls it first.
        assert_eq!(sb.read(&mut spi, FSM) & 0b111, 0);
    }

    #[test]
    fn wait_spi_idle_sees_an_idle_fsm_and_a_clear_wip() {
        // `Wait_SPI_Idle` (`0x4004_9b30`): `while (fsm & 7) {}` on SPI1 and
        // SPI0, then `read_status` until WIP is clear. Both loops must
        // terminate on the first pass or nothing else in the ROM ever runs.
        let (mut sb, mut spi, _flash) = rig();
        assert_eq!(sb.read(&mut spi, FSM) & 7, 0);
        sb.write(&mut spi, RD_STATUS, 0);
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        assert_eq!(sb.read(&mut spi, CMD), 0);
        assert_eq!(sb.read(&mut spi, RD_STATUS) & u32::from(SR_WIP), 0);
    }

    #[test]
    fn write_enable_sets_the_latch_the_rom_loops_until_it_sees() {
        // `_SPI_write_enable` (`0x4004_9f14`): cmd = 1<<30, spin, then
        // `esp_rom_spiflash_read_status` until bit 1 is set.
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
        // `SPI_read_data` (`0x4004_98d0`): addr = byte address, miso_dlen =
        // bits-1, cmd = 1<<18, then whole words out of `w0..`. The read
        // path leaves `user`'s reset `usr_command`, ORs in
        // addr/dummy/miso and clears mosi (`_esp_rom_spiflash_read`).
        let (mut sb, mut spi, flash) = rig();
        flash
            .lock()
            .unwrap()
            .stage(0x61_0000, b"littlefs-ish header bytes");
        let user = sb.read(&mut spi, USER);
        sb.write(&mut spi, USER, (user & 0xf7ff_ffff) | 0x7000_0000);
        sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(op::READ));
        sb.write(&mut spi, ADDR, 0x61_0000);
        sb.write(&mut spi, MISO_DLEN, 8 * 8 - 1);
        sb.write(&mut spi, CMD, CMD_USR);
        assert_eq!(sb.read(&mut spi, CMD), 0);
        assert_eq!(sb.read(&mut spi, W0).to_le_bytes(), *b"litt");
        assert_eq!(sb.read(&mut spi, W0 + 4).to_le_bytes(), *b"lefs");
    }

    /// The convention the classic does NOT share: `addr` is the byte
    /// address itself, so the partition table at `0x8000` is read from
    /// `addr = 0x8000` — not from `0x0080_0000`.
    #[test]
    fn the_usr_address_phase_carries_the_plain_byte_address() {
        let (mut sb, mut spi, flash) = rig();
        flash.lock().unwrap().stage(0x8000, b"\xaaP");
        sb.write(&mut spi, USER, USER_COMMAND | USER_ADDR | USER_MISO);
        sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(op::READ));
        sb.write(&mut spi, ADDR, 0x8000);
        sb.write(&mut spi, MISO_DLEN, 512 - 1);
        sb.write(&mut spi, CMD, CMD_USR);
        assert_eq!(
            sb.read(&mut spi, W0) & 0xffff,
            0x50aa,
            "the partition table's magic, read at the address the ROM stored"
        );
    }

    #[test]
    fn every_read_mode_the_rom_can_pick_moves_the_same_bytes() {
        // `_esp_rom_spiflash_read` chooses 0xeb/0x6b/0x3b/0x0b/0x03 (and
        // 0xbb) off `ctrl`'s read-mode bits and only changes the phase
        // widths.
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
        // `SPI_page_program` (`0x4004_9f84`): user |= 0xc8000000, user2 =
        // 0x70000002, cmd = 0x60000 (usr | flash_pe), after a write-enable.
        let (mut sb, mut spi, flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        sb.write(&mut spi, ADDR, 0x61_0000);
        sb.write(&mut spi, W0, u32::from_le_bytes(*b"lpfs"));
        sb.write(&mut spi, MOSI_DLEN, 4 * 8 - 1);
        sb.write(&mut spi, USER, 0xc800_0000);
        sb.write(&mut spi, USER2, 0x7000_0002);
        sb.write(&mut spi, CMD, CMD_USR | CMD_FLASH_PE);
        assert_eq!(flash.lock().unwrap().peek(0x61_0000, 4).unwrap(), b"lpfs");
        assert_eq!(spi.status() & SR_WEL, 0, "the program consumed WEL");

        // Without a write-enable the part ignores the program, and so does
        // this: the second word must not land.
        sb.write(&mut spi, ADDR, 0x61_0010);
        sb.write(&mut spi, W0, u32::from_le_bytes(*b"nope"));
        sb.write(&mut spi, CMD, CMD_USR | CMD_FLASH_PE);
        assert_eq!(
            flash.lock().unwrap().peek(0x61_0010, 4).unwrap(),
            &[0xff; 4]
        );
    }

    #[test]
    fn the_dedicated_sector_erase_bit_erases_the_granule_the_address_is_in() {
        // `SPI_sector_erase` (`0x4004_9b98`): addr, cmd = 0x1020000
        // (flash_se | flash_pe).
        let (mut sb, mut spi, flash) = rig();
        flash.lock().unwrap().stage(0x61_0800, b"stale");
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        // A byte inside the sector, not its base: the part aligns.
        sb.write(&mut spi, ADDR, 0x61_0800);
        sb.write(&mut spi, CMD, 0x0102_0000);
        assert_eq!(
            flash.lock().unwrap().peek(0x61_0800, 5).unwrap(),
            &[0xff; 5]
        );
        assert_eq!(flash.lock().unwrap().sector_erases, 1);
    }

    #[test]
    fn write_status_carries_the_value_out_of_rd_status_and_keeps_wip_clear() {
        // `esp_rom_spiflash_write_status` (`0x4004_9c40`) puts the value in
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
        // `esp_rom_spiflash_read_user_cmd` (`0x4004_99b4`): miso_dlen = 7,
        // user = 0x9000_0000, user2 = 0x7000_0000|cmd, w0 = 0, cmd = 1<<18,
        // then `w0 & 0xff`; `esp_rom_spiflash_read_statushigh` sends 0x35.
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
    fn the_dlen_field_is_ten_bits_wide_on_this_part() {
        // `SPI_page_program` masks the count with `extui …, 0, 10`, and the
        // PAC says "Bits 0:9". The buffer cap is what makes both safe.
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, MISO_DLEN, 0x3ff);
        assert_eq!(spi.dlen_bytes(MISO_DLEN), spi_flash::BUFFER_LEN as u32);
        sb.write(&mut spi, MISO_DLEN, 4 * 8 - 1);
        assert_eq!(spi.dlen_bytes(MISO_DLEN), 4);
    }

    #[test]
    fn the_names_come_from_the_generated_table_at_the_s3s_offsets() {
        let (_, spi, _flash) = rig();
        assert_eq!(spi.reg_name(CMD), Some("cmd"));
        assert_eq!(spi.reg_name(RD_STATUS), Some("rd_status"));
        assert_eq!(spi.reg_name(W0), Some("w0"));
        assert_eq!(spi.reg_name(FSM), Some("fsm"));
        assert_eq!(spi.reg_name(0x0f0), Some("int_ena"));
    }
}
