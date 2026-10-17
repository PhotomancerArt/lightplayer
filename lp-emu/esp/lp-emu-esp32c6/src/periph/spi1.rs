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
//!    answer, because nothing observes the interval — so below `t4` [`Spi1`]
//!    completes synchronously. At `t4` ([`Spi1::set_timing`],
//!    [`crate::flash_timing`]) the interval is the point: the bytes still
//!    move at the trigger, but `cmd` reads the trigger bits and a busy
//!    `mst_st` back until the transfer's wire time has passed, and the guest
//!    spins for it as on silicon.
//! 2. **The status register is real state.** WIP (bit 0) is 0 below `t4`,
//!    because operations complete; at `t4` it reads set for the part's busy
//!    time after a program or an erase. WEL (bit 1) is set by `flash_wren`, cleared by
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
//! model that pretended to honour them would be inventing behaviour. (At
//! `t4` the bus width and the clock divider are read — not for the bytes,
//! for how long the transfer takes.) A
//! `cmd` trigger this table does not know is **refused loudly** — it clears
//! the bit (so the guest does not hang) and writes a `SPI1 unmodelled
//! command` line into the trace naming the bit and the PC, which is the
//! honest answer for a bring-up: the run continues and the log says what it
//! did not do.

use std::sync::Arc;

use lp_emu_esp_common::engine::nor_timing::NorTiming;
use lp_emu_esp_common::engine::spi_flash::{self, FlashEngine, FlashOp, FlashOutcome, op};
use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::flash::{BLOCK_LEN, FlashHandle, SECTOR_LEN};
use crate::flash_timing::{self, FlashClocks, Phases};
use crate::{memmap, regs};

/// This block's name, in its own trace lines and in the ones the engine
/// writes on its behalf — the engine models a NOR flash and does not know
/// which controller is driving it.
const NAME: &str = "SPI1";

// Register offsets (`regs::SPI1`, generated from the PAC).
pub const CMD: u32 = 0x000;
pub const ADDR: u32 = 0x004;
pub const CTRL: u32 = 0x008;
/// Chip-select setup and hold timing. The PAC calls this register
/// **write-only** and the mask ROM reads it back twice — see
/// [`Spi1::reg_grade`] and
/// `docs/defects/2026-09-09-the-pac-calls-spi-ctrl2-write-only-and-the-mask-rom-reads-it.md`.
pub const CTRL2: u32 = 0x010;
/// The SPI clock divider (`clkcnt_n`, `clk_equ_sysclk`); read at `t4`.
pub const CLOCK: u32 = 0x014;
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
const USER_DUMMY: u32 = 1 << 29;
const USER_ADDR: u32 = 1 << 30;
const USER_COMMAND: u32 = 1 << 31;

/// `ctrl.wrsr_2b`: a status write carries two bytes.
const CTRL_WRSR_2B: u32 = 1 << 22;

// The flash part's status-register bits, as the ROM reads them: WIP (0) and
// WEL (1) are the SPI NOR command set's own, so they live with the chip in
// [`lp_emu_esp_common::engine::spi_flash`] and are named here for the paths
// that already say `spi1::SR_WEL`.
pub use lp_emu_esp_common::engine::spi_flash::{SR_WEL, SR_WIP};

/// The legacy flash controller.
pub struct Spi1 {
    regs: RegFile,
    /// The chip on the other side, and the WIP/WEL latch a mask ROM spins
    /// on. Everything this block's `cmd` triggers ends up here.
    engine: FlashEngine,
    /// Every `cmd` trigger this model does not know, once each, so a boot
    /// that reaches one says so without flooding the log.
    refused: Vec<u32>,
    /// `t4`'s transfer time ([`set_timing`](Self::set_timing)); `None` at
    /// every other grade, where every trigger completes inside the write.
    timing: Option<Spi1Timing>,
    /// The cycles the transfer being executed takes on the wire — what the
    /// engine adds before a program's or an erase's busy time starts.
    wire: u64,
}

/// A transfer in flight, and the clock it is timed by.
struct Spi1Timing {
    clocks: Arc<FlashClocks>,
    /// The guest cycle the in-flight transfer ends at.
    busy_until: u64,
    /// The trigger bits it was started with: `cmd` reads them back, with a
    /// non-idle `mst_st`, until it ends.
    busy_bits: u32,
}

/// `mst_st` while a transfer runs: any non-idle state is what the ROM's
/// `Wait_SPI_Idle` (`cmd & 0xf`) and every `while cmd != 0` wait out.
const MST_ST_BUSY: u32 = 1;

/// The register file's snapshot length: `0x400` bytes of window.
const REGS_STATE_LEN: usize = 0x400;

impl core::fmt::Debug for Spi1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Spi1")
            .field("status", &self.engine.status())
            .finish_non_exhaustive()
    }
}

/// Every non-zero reset value the PAC gives SPI1's registers
/// (`esp32c6-0.23.2/src/spi1/*.rs`, each register module's
/// `impl Resettable`). Derived data with the same provenance as the name
/// tables.
///
/// **`user` is the load-bearing one.** It resets to `0x8000_0000` —
/// `usr_command` already set — and the mask ROM's read path *never sets it*:
/// `_esp_rom_spiflash_read` only clears `usr_mosi` and sets `usr_miso`, and
/// `esp_rom_spi_set_rd_cmd_bit_len` only touches `usr_addr` and `usr_dummy`.
/// A block that reset to zero therefore issued every flash read with no
/// command phase, and every read returned nothing. That is not a theory: it
/// is what the second-boot gate caught here — the first boot formatted
/// `lpfs`, the bytes were in the image, and the second boot reformatted,
/// because the mount's reads moved no data. The `[FS]` pair and the §5.1
/// heap figures had already matched, since a read that returns nothing and a
/// read of a blank chip both fail the superblock check the same way.
impl Spi1 {
    pub fn new(flash: FlashHandle) -> Self {
        let regs = RegFile::new("SPI1", 0x400)
            .with_names(regs::SPI1)
            .with_pac_grades()
            .with_grade(CMD, RegGrade::Documented)
            .with_grade(CTRL2, RegGrade::Documented)
            .with_grade(RD_STATUS, RegGrade::Documented);
        Self {
            regs,
            engine: FlashEngine::new(flash),
            refused: Vec::new(),
            timing: None,
            wire: 0,
        }
    }

    /// Time this block (`t4`): every trigger keeps `cmd` busy for its
    /// transfer at the SCK `clocks` and this block's own `clock` register
    /// give, and the part holds WIP for `part`'s busy times.
    pub fn set_timing(&mut self, clocks: Arc<FlashClocks>, part: NorTiming) {
        self.engine.set_timing(Some(part), memmap::CPU_HZ);
        self.timing = Some(Spi1Timing {
            clocks,
            busy_until: 0,
            busy_bits: 0,
        });
    }

    pub fn status(&self) -> u16 {
        self.engine.status()
    }

    /// Set a register without a guest write — what a direct load does in the
    /// bootloader's place ([`crate::loader::seed_flash_interface`]).
    pub fn poke(&mut self, off: u32, value: u32) {
        self.regs.poke(off, value);
    }

    /// The SCK this block's transfers run at.
    fn sck_hz(&self, clocks: &FlashClocks) -> u64 {
        flash_timing::sck_hz(clocks.mspi_hz(), self.regs.stored(CLOCK))
    }

    /// What a trigger moves on the wire: the `usr` engine's phases from
    /// `user`/`user1`/`user2` and the data lengths, on the lines `ctrl`
    /// (reads) and `user` (writes) select; a dedicated trigger's are the
    /// single-line command it stands for.
    fn phases(&self, triggered: u32) -> Phases {
        let ctrl = self.regs.stored(CTRL);
        let user = self.regs.stored(USER);
        if triggered & CMD_USR != 0 {
            let user1 = self.regs.stored(USER1);
            let user2 = self.regs.stored(USER2);
            let (read_addr, read_data) = flash_timing::read_lines(ctrl);
            let (write_addr, write_data) = flash_timing::write_lines(user);
            let bits = |off: u32| (self.regs.stored(off) & 0x3ff) + 1;
            let (addr_lines, data_bits, data_lines) = if user & USER_MISO != 0 {
                (read_addr, bits(MISO_DLEN), read_data)
            } else if user & USER_MOSI != 0 {
                (write_addr, bits(MOSI_DLEN), write_data)
            } else {
                (read_addr, 0, 1)
            };
            return Phases {
                cmd_bits: if user & USER_COMMAND != 0 {
                    ((user2 >> 28) & 0xf) + 1
                } else {
                    0
                },
                cmd_lines: flash_timing::cmd_lines(ctrl),
                addr_bits: if user & USER_ADDR != 0 {
                    ((user1 >> 26) & 0x3f) + 1
                } else {
                    0
                },
                addr_lines,
                dummy_clocks: if user & USER_DUMMY != 0 {
                    (user1 & 0x3f) + 1
                } else {
                    0
                },
                data_bits,
                data_lines,
            };
        }
        match triggered & !CMD_FLASH_PE {
            CMD_FLASH_RDSR => Phases::single(8, 0, 8),
            CMD_FLASH_WRSR => Phases::single(8, 0, if ctrl & CTRL_WRSR_2B != 0 { 16 } else { 8 }),
            CMD_FLASH_RDID => Phases::single(8, 0, 24),
            CMD_FLASH_SE | CMD_FLASH_BE => Phases::single(8, 24, 0),
            CMD_FLASH_PP => Phases::single(8, 24, self.dlen_bytes(MOSI_DLEN) * 8),
            CMD_FLASH_READ => {
                let (addr_lines, data_lines) = flash_timing::read_lines(ctrl);
                Phases {
                    cmd_bits: 8,
                    cmd_lines: 1,
                    addr_bits: 24,
                    addr_lines,
                    dummy_clocks: 0,
                    data_bits: self.dlen_bytes(MISO_DLEN) * 8,
                    data_lines,
                }
            }
            // WREN, WRDI, CE, HPM, RES, DP: the command byte alone.
            _ => Phases::single(8, 0, 0),
        }
    }

    /// The `w0..w15` buffer as bytes, little-endian per word — the layout
    /// the ROM's copy loops assume (`SPI_read_data` reads whole words out of
    /// `0x6000_3058 + 4n`).
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

    /// The flash byte address the address phase carries.
    ///
    /// `SPI_read_data` and `SPI_page_program` both write the plain byte
    /// address into `addr` (`sw a1,4(a5)`), and `user1`'s
    /// `usr_addr_bitlen` says how many bits go on the wire — 24 for a plain
    /// read, 28 for QIO's four mode bits. Which of those bits are address is
    /// the part's business, so the masking and its diagnostic are the
    /// engine's; the *offset* of `addr` is this block's.
    fn flash_addr(&mut self, cx: &mut BusCx<'_>) -> u32 {
        FlashEngine::flash_addr(self.regs.stored(ADDR), NAME, cx)
    }

    /// Bytes = `dlen + 1` bits, rounded up, capped at the 64-byte buffer.
    fn dlen_bytes(&self, off: u32) -> u32 {
        let bits = (self.regs.stored(off) & 0x3ff) + 1;
        bits.div_ceil(8).min(spi_flash::BUFFER_LEN as u32)
    }

    /// Hand one decoded transaction to the engine, and put back what belongs
    /// in this block's registers.
    ///
    /// The engine never touches `w0..w15`: it does not know where they are.
    /// A program is handed the bytes this block has already gathered, and a
    /// read comes back in the same 64-byte array for this block to scatter.
    fn run(&mut self, op: FlashOp, cx: &mut BusCx<'_>) -> FlashOutcome {
        let mut buffer = match op {
            FlashOp::Program { .. } => self.buffer_bytes(),
            _ => [0u8; spi_flash::BUFFER_LEN],
        };
        let outcome = self
            .engine
            .execute_timed(op, &mut buffer, NAME, cx, self.wire);
        if outcome == FlashOutcome::Buffer {
            self.set_buffer_bytes(&buffer);
        }
        outcome
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
                if let FlashOutcome::Word(id) = self.run(FlashOp::ReadId, cx) {
                    self.regs.poke(W0, id);
                }
            }
            CMD_FLASH_RDSR => {
                if let FlashOutcome::Status(status) = self.run(FlashOp::ReadStatus, cx) {
                    let kept = self.regs.stored(RD_STATUS) & 0xffff_0000;
                    self.regs.poke(RD_STATUS, kept | u32::from(status));
                }
            }
            CMD_FLASH_WRSR => {
                // The value is in `rd_status`'s low half — the ROM put it
                // there (`esp_rom_spiflash_write_status`). WIP stays 0 and
                // the write consumes WEL, as it does on the part.
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
            // A status write through the generic engine — how an app sets
            // the part's QE bit to switch to QIO itself (the 2026-10-17
            // DIO/QIO sitting's firmware does: `0x01` with one byte, then
            // `0x31`). The data is `w0`'s low bytes; a half the command does
            // not carry keeps what the part held. WEL and WIP are the
            // engine's, never written from here.
            op::WRITE_STATUS | op::WRITE_STATUS_HIGH => {
                if !self.phase(user, USER_MOSI, "write-data", command, cx) {
                    return;
                }
                let held = self.engine.status() & !(SR_WEL | SR_WIP);
                let data = self.regs.stored(W0);
                let bytes = self.dlen_bytes(MOSI_DLEN);
                let value = match (command, bytes) {
                    (op::WRITE_STATUS_HIGH, _) => (held & 0x00ff) | ((data as u16 & 0xff) << 8),
                    (_, 1) => (held & 0xff00) | (data as u16 & 0xff),
                    _ => data as u16,
                };
                self.run(FlashOp::WriteStatus(value), cx);
            }
            op::RDID => {
                if let FlashOutcome::Word(id) = self.run(FlashOp::ReadId, cx) {
                    self.regs.poke(W0, id);
                }
            }
            other => {
                // The engine does not guess at a command byte it does not
                // know; the refusal is this block's, and carries its name.
                if self.run(FlashOp::Unknown(other), cx) == FlashOutcome::Unknown {
                    cx.trace.note(&format!(
                        "cyc={} pc={:#010x} SPI1 usr command {other:#04x} is not modelled; \
                         no bytes moved and the buffer is unchanged",
                        cx.now, cx.pc
                    ));
                }
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
        NAME
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        if off & !3 == CMD {
            // Untimed, every trigger has already completed and cleared;
            // `mst_st` and `slv_st` read 0 (idle), which is what
            // `Wait_SPI_Idle` waits for. So `cmd` reads exactly zero. Timed
            // (`t4`), the trigger bits and a busy `mst_st` read back until
            // the transfer's wire time has passed.
            let word = match &self.timing {
                Some(t) if cx.now < t.busy_until => t.busy_bits | MST_ST_BUSY,
                _ => 0,
            };
            return lane_of(word, off, width);
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
            // Timed, the transfer's own duration: what `cmd` stays busy
            // for, and what a program's or an erase's busy time starts
            // after.
            self.wire = match &self.timing {
                Some(t) => flash_timing::transfer_cycles(
                    self.phases(triggered).clocks(),
                    self.sck_hz(&t.clocks),
                ),
                None => 0,
            };
            self.execute(triggered, cx);
            if let Some(t) = self.timing.as_mut() {
                t.busy_until = cx.now + self.wire;
                t.busy_bits = triggered;
            }
        }
        // The register file keeps no trigger: an untimed one is done, and a
        // timed one's busy bits are `timing`'s.
        self.regs.poke(CMD, 0);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::SPI1.name(off)
    }

    /// The block's grades, from the PAC, with two hand-written rows.
    ///
    /// [`RegFile::with_pac_grades`] cannot see the two registers this block
    /// intercepts in its own [`Peripheral::read`] and
    /// [`Peripheral::write`] — `pretends_about` walks the `RegFile`'s
    /// declared exceptions, and `cmd` is a hand-written match arm above it.
    /// So both are stated here, with the source:
    ///
    /// | register | grade | source |
    /// |---|---|---|
    /// | `cmd` +0x000 | `documented` | PAC `spi1::cmd`: every trigger bit is documented as cleared once the operation is done, and `mst_st`/`slv_st` are status that read idle when nothing is running. This block completes inside the write, so `cmd` reads exactly 0 — which is the state the ROM's own `Wait_SPI_Idle` (`0x4002_42e8`) spins for. The *duration* is a timing claim this block does not make (`t2`'s rung), not a register-behaviour one. |
    /// | `ctrl2` +0x010 | `documented` | The PAC calls it **write-only**, so the rule above graded it `modeled` and a `--strict-grade documented` run refused the mask ROM's own flash-timing setup. It is not write-only: `spi_common_set_flash_cs_timing` (`0x4002_497e`) read-modify-writes it **twice** — `lw a4,16(a5)` / `andi a4,a4,-993` / `ori a4,a4,64` / `sw`, then `lw` / `andi a4,a4,-32` / `ori a4,a4,1` / `sw` — so the ROM both reads it and depends on the bits it reads back. A register the mask ROM reads is readable. See `docs/defects/2026-09-09-the-pac-calls-spi-ctrl2-write-only-and-the-mask-rom-reads-it.md`. |
    /// | `rd_status` +0x02c | `documented` | The flash part's status register, not the SoC's: WIP (bit 0) and WEL (bit 1) are the SPI NOR command set's, and the ROM's `_SPI_write_enable` (`0x4002_4630`) loops until it reads WEL set — a model that left it clear would hang the first write. Bits above WEL are whatever `flash_wrsr` last wrote, because `_esp_rom_spiflash_unlock` writes block-protect bits and reads them back. |
    ///
    /// Everything else is the PAC's own rule: a named read/write register
    /// this block does not pretend about is `documented`, anything else
    /// `modeled`. Nothing in SPI1 is `measured` — that needs a committed
    /// silicon transcript naming the register, and the desk captures this
    /// repository holds are UART and RMT ones.
    ///
    /// The path M3 P2 drove through this block is the mask ROM's
    /// `esp_rom_spiflash_*` family, reached from the download console's
    /// `FLASH_BEGIN`/`FLASH_DATA`/`SPI_FLASH_MD5` commands: `cmd`, `addr`,
    /// `user`, `user1`, `user2`, `mosi_dlen`, `miso_dlen`, `rd_status` and
    /// `w0..w15` — the same registers the application's own flash reads take,
    /// which is why the flasher's path found no ungraded register here.
    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.regs.reg_grade(off)
    }

    /// The engine's status latch, then this block's register file — the
    /// order the snapshot format has always had, which is why the engine
    /// writes its half into the same buffer rather than owning the stream.
    ///
    /// Timed (`t4`), three more fields follow: the part's busy deadline,
    /// the transfer's, and the transfer's trigger bits. An untimed block
    /// never writes them, so its snapshot is byte-for-byte what it was.
    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(REGS_STATE_LEN + 2 + 20);
        self.engine.save_status(&mut out);
        out.extend_from_slice(&self.regs.save_state());
        if let Some(t) = &self.timing {
            self.engine.save_busy(&mut out);
            out.extend_from_slice(&t.busy_until.to_le_bytes());
            out.extend_from_slice(&t.busy_bits.to_le_bytes());
        }
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let Some((status, n)) = FlashEngine::load_status(bytes) else {
            log::warn!("SPI1::load_state: {} bytes is too short", bytes.len());
            return;
        };
        self.engine.restore(status);
        let rest = &bytes[n..];
        let regs_len = REGS_STATE_LEN.min(rest.len());
        self.regs.load_state(&rest[..regs_len]);
        let tail = &rest[regs_len..];
        if let Some(t) = self.timing.as_mut() {
            let (busy, deadline, bits) = match (
                FlashEngine::load_busy(tail),
                tail.get(8..16).and_then(|b| <[u8; 8]>::try_from(b).ok()),
                tail.get(16..20).and_then(|b| <[u8; 4]>::try_from(b).ok()),
            ) {
                (Some((busy, _)), Some(deadline), Some(bits)) => {
                    (busy, u64::from_le_bytes(deadline), u32::from_le_bytes(bits))
                }
                // A snapshot an untimed block took: nothing was in flight.
                _ => (0, 0, 0),
            };
            self.engine.restore_busy(busy);
            t.busy_until = deadline;
            t.busy_bits = bits;
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
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
    fn the_reset_user_register_is_what_enables_the_roms_read_command_phase() {
        // `_esp_rom_spiflash_read` clears `usr_mosi` and sets `usr_miso`;
        // `esp_rom_spi_set_rd_cmd_bit_len` sets `usr_addr` and the dummy
        // length. **Nothing sets `usr_command`** — the reset value already
        // has it, and a block that reset to zero moves no bytes on any read.
        let (mut sb, mut spi, flash) = rig();
        assert_eq!(sb.read(&mut spi, USER), 0x8000_0000);
        assert_eq!(sb.read(&mut spi, USER1), 0x5c00_0007);
        assert_eq!(sb.read(&mut spi, USER2), 0x7000_0000);
        assert_eq!(sb.read(&mut spi, CTRL), 0x002c_a00c);

        // Now drive it exactly as the ROM does, touching nothing else.
        flash.lock().unwrap().stage(0x31_0000, b"littlefs");
        let user = sb.read(&mut spi, USER);
        sb.write(&mut spi, USER, (user & 0xf7ff_ffff) | 0x1000_0000); // -mosi +miso
        let user = sb.read(&mut spi, USER);
        sb.write(&mut spi, USER, user | 0x4000_0000); // +addr
        sb.write(&mut spi, USER2, 0x7000_0003); // 8-bit command 0x03
        sb.write(&mut spi, MISO_DLEN, 16 * 8 - 1);
        sb.write(&mut spi, ADDR, 0x31_0000);
        sb.write(&mut spi, CMD, CMD_USR);
        assert_eq!(sb.read(&mut spi, W0).to_le_bytes(), *b"litt");
        assert_eq!(sb.read(&mut spi, W0 + 4).to_le_bytes(), *b"lefs");
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

    #[test]
    fn timed_cmd_stays_busy_for_the_transfer_and_wip_for_the_erase() {
        let (mut sb, mut spi, _flash) = rig();
        let clocks = Arc::new(FlashClocks::new());
        clocks.set_pcr(0x500, 0x2801_0200); // 80 MHz MSPI, as the bootloader leaves it
        spi.set_timing(clocks, NorTiming::CX1);
        sb.write(&mut spi, CLOCK, 0x8000_0000);
        // A 64-byte DIO read: 8 + 14 + 2 + 256 SCK at 80 MHz, + overhead.
        sb.write(&mut spi, CTRL, 0x00ec_200c);
        sb.write(
            &mut spi,
            USER,
            USER_COMMAND | USER_ADDR | USER_DUMMY | USER_MISO,
        );
        sb.write(&mut spi, USER1, 0x6c00_0001);
        sb.write(&mut spi, USER2, 0x7000_0000 | u32::from(op::DUAL_IO));
        sb.write(&mut spi, MISO_DLEN, 64 * 8 - 1);
        sb.now = 100;
        sb.write(&mut spi, CMD, CMD_USR);
        let done = 100 + 2 * (8 + 14 + 2 + 256) + flash_timing::TRANSFER_OVERHEAD_CYCLES;
        sb.now = done - 1;
        assert_eq!(sb.read(&mut spi, CMD), CMD_USR | MST_ST_BUSY);
        sb.now = done;
        assert_eq!(sb.read(&mut spi, CMD), 0);

        // An erase: `cmd` clears after its 32 SCK, WIP after 21 ms more.
        sb.write(&mut spi, CMD, CMD_FLASH_WREN);
        sb.now += 1_000;
        sb.write(&mut spi, CMD, CMD_FLASH_SE | CMD_FLASH_PE);
        let erase_at = sb.now;
        sb.now += 1_000;
        assert_eq!(sb.read(&mut spi, CMD), 0);
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        sb.now += 1_000;
        assert_eq!(sb.read(&mut spi, RD_STATUS) & u32::from(SR_WIP), 1);
        sb.now = erase_at + 21 * 160_000 + 1_000;
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        sb.now += 1_000;
        assert_eq!(sb.read(&mut spi, RD_STATUS) & u32::from(SR_WIP), 0);
    }

    #[test]
    fn untimed_cmd_reads_idle_at_once() {
        let (mut sb, mut spi, _flash) = rig();
        sb.write(&mut spi, CMD, CMD_FLASH_RDSR);
        assert_eq!(sb.read(&mut spi, CMD), 0);
        let state = spi.save_state();
        assert_eq!(state.len(), 2 + REGS_STATE_LEN, "no timing tail untimed");
    }
}
