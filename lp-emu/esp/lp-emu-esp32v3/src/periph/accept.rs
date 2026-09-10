//! The accept-and-remember blocks: a [`RegFile`] each, seeded from the PAC's
//! reset values, with the exceptions the boot path needs written down as
//! overrides — **each carrying its evidence beside it.**
//!
//! # The two rules this file exists to hold
//!
//! **A reset value comes from the PAC's `Resettable`.** [`RegFile::with_names`]
//! seeds every register of a block from the generated `regs/<block>.rs`
//! table, so a block reads what the part reads before anyone writes it. A
//! `with_reset` in this file is a **deviation from the PAC**, and the test
//! `the_only_deviations_from_the_pacs_resets_are_the_listed_ones` is the
//! list. The sweep that produced the rule is
//! `docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`:
//! an accept block seeded with the value a spin happened to want looks
//! exactly like one seeded from the PAC, right up to the moment a different
//! image spins on a different value.
//!
//! **An exception carries its evidence beside it.** Not "the boot spins
//! here", but *why the value is what it is*: a ROM disassembly line, a PAC
//! field doc, a linker constant, an esp-hal source line. A spin no pin table
//! can justify is an **E-premise stop** — reported in the phase report, not
//! answered with an invented value.
//!
//! # Which phase owns which block
//!
//! Every block here is a probe. The phase that gives it behaviour is named
//! on the block, and the order the strict run needed them in is the ledger
//! in `docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`.

use lp_emu_esp_common::RegFile;

use crate::regs;

/// `DPORT`'s aperture: the generated table runs to `+0xffc` (`date`), and
/// the next block (`AES`) is at `+0x1000`. The flash MMU page tables at
/// `0x3FF1_0000` / `0x3FF1_2000` are **not** inside it — they are raw arrays
/// P4 declares from the ROM's own `cache_flash_mmu_set`
/// (`crate::memmap::FLASH_MMU_PRO`).
pub const DPORT_LEN: u32 = 0x1000;

/// `DPORT` — **P4's block**, accept-and-remember here.
///
/// The first strict stop of the direct load: twenty-nine instructions in,
/// `esp_hal::soc::xtensa::esp32_init` → `interrupt::setup_interrupts` starts
/// clearing the APP core's interrupt map (`core_1_intr_map[0]` at `+0x218`,
/// `Write Word 0x3ff00218` at cycle 29). Everything `esp_hal::init` does to
/// this block on the way to `main` — both cores' interrupt maps, the
/// peripheral clock/reset gates (`perip_clk_en`/`perip_rst_en`, `+0xc0`/
/// `+0xc4`), `cpu_per_conf` for `CpuClock::max()`, the software interrupts
/// (`cpu_intr_from_cpu[0..4]`) — is written and, where read back, read back
/// as written.
///
/// What an accept block cannot do for it, and P4 does: route
/// `core_0_intr_map[src]` writes into `CpuIntMatrix::asserted`, hold core 1
/// through `appcpu_ctrl_c.appcpu_runstall`, and give `pro_cache_ctrl.
/// pro_cache_enable` (bit 3) the cache-off fetch stop D4 defines against it.
/// Every reset value is the PAC's (`esp32-0.40.2/src/dport.rs`, 49 non-zero
/// resets); no exception is needed to reach the next stop.
pub fn dport() -> RegFile {
    RegFile::new("DPORT", DPORT_LEN)
        .with_names(regs::DPORT)
        .with_pac_grades()
}

/// `APB_CTRL`'s aperture, tight: the generated table runs to `+0x7c`
/// (`date`).
pub const APB_CTRL_LEN: u32 = 0x80;

/// `APB_CTRL.date` (`+0x7c`) — and **bit 31 of it is a chip-revision bit**.
pub const APB_CTRL_DATE: u32 = 0x07c;

/// `APB_CTRL` — the phase file's first-named accept candidate, and the
/// third strict stop of the direct load, 109,663 cycles in:
/// `fw_esp32v3::boot_firmware+0x29a` (the inlined `esp_hal::init` →
/// `Clocks::init`) reads `sysclk_conf` at `+0x00`, whose `pre_div_cnt`
/// field is the APB pre-divider `esp-hal`'s clock tree reads and then
/// re-writes (`soc/esp32/clocks.rs:435-437`, `modify(|_, w|
/// w.pre_div_cnt().bits(…))`); the four `*_tick_conf` registers after it are
/// written outright (`:479-529`). Nothing spins on this block and nothing
/// in the shipped image reads a bit back that hardware would have changed,
/// so accept-and-remember with the PAC's resets (`sysclk_conf` =
/// `0x0000_2000`, `xtal_tick_conf` = `0x27`, …) is the whole model. P5's
/// accept list, and P5 keeps it an accept block.
///
/// # The one deviation: `date` bit 31 is a chip-revision bit
///
/// esp-hal's `major_chip_version` (`esp-hal-1.1.1/src/efuse/esp32/mod.rs`)
/// is
///
/// ```text
/// let eco_bit0 = read_field_le::<u32>(CHIP_VER_REV1);            // eFuse block 0 bit 111
/// let eco_bit1 = read_field_le::<u32>(CHIP_VER_REV2);            // eFuse block 0 bit 180
/// let eco_bit2 = (APB_CTRL::regs().date().read().bits() & 0x80000000) >> 31;
/// match (eco_bit2 << 2) | (eco_bit1 << 1) | eco_bit0 { 1 => 1, 3 => 2, 7 => 3, _ => 0 }
/// ```
///
/// so **the top bit of a revision this chip's eFuse block cannot express
/// lives here**, and a v3 part reads it set. The PAC's reset for `date` is 0
/// (the table carries none), because an SVD cannot know which stepping it is
/// describing. The revision is an input to the run — the same kind of input
/// [`rtc_cntl`]'s reset cause is — so this block takes it from the same
/// [`crate::loader::EfuseIdentity`] the eFuse view is built from, and the
/// deviation is listed.
///
/// Nothing else in the register changes: the low 31 bits stay the PAC's.
pub fn apb_ctrl(id: crate::loader::EfuseIdentity) -> RegFile {
    let (_, _, eco2) = crate::periph::efuse::eco_bits(id.chip_major);
    let date = regs::APB_CTRL.reset(APB_CTRL_DATE).unwrap_or(0) | (u32::from(eco2) << 31);
    RegFile::new("APB_CTRL", APB_CTRL_LEN)
        .with_names(regs::APB_CTRL)
        .with_reset(APB_CTRL_DATE, date)
        .with_pac_grades()
}

/// The analog I2C master's aperture: eight command/status words, one per
/// `host_id` (`0x6000_E000 + 4·host_id`; the BBPLL is host 4, the highest
/// esp-idf names for this chip is 7). Tight on purpose — a ninth host is a
/// strict stop, not a silent zero — and the ROM's other literals in the
/// block (`+0x50`, `+0x5c`, `+0x80`, the PHY's `ANA_CONFIG` words) are
/// outside it until a boot reaches them.
pub const I2C_ANA_MST_LEN: u32 = 0x20;

/// The analog I2C master's register names. **Hand-written**, because the
/// `esp32` PAC has no block at this address; the layout is the mask ROM's:
///
/// ```text
/// 40004168 <rom_chip_i2c_writeReg>:          (block, host_id, reg, data)
/// 4000416b:  l32r  a9, (0x01000000)          ; bit 24: write
/// 40004177:  l32r  a9, (0x18003800)
/// 40004183:  add.n a9, a3, a9                ; + host_id
/// 4000418b:  slli  a9, a9, 2                 ; ×4 → 0x6000E000 + 4·host_id
/// 40004180:  slli  a4, a4, 8                 ; reg  << 8
/// 40004188:  slli  a8, a5, 16                ; data << 16
/// 40004197:  s32i.n a2, a9, 0                ; write the command word
/// 4000419c:  l32i.n a8, a9, 0
/// 4000419e:  bany  a8, a10(0x02000000), -5   ; spin while bit 25 (busy)
///
/// 40004110 <rom_chip_i2c_readReg>:  same word, no bit 24; after the spin,
/// 40004141:  extui a2, a2, 16, 8             ; data = bits 23:16
/// ```
///
/// So one word per host: `[7:0]` slave address, `[15:8]` register, `[23:16]`
/// data, `[24]` write, `[25]` busy.
pub static I2C_ANA_MST_NAMES: lp_emu_esp_common::regnames::RegNames =
    lp_emu_esp_common::regnames::RegNames {
        block: "i2c_ana_mst",
        entries: &[
            (0x000, "host0"),
            (0x004, "host1"),
            (0x008, "host2"),
            (0x00c, "host3"),
            (0x010, "host4_bbpll"),
            (0x014, "host5"),
            (0x018, "host6"),
            (0x01c, "host7"),
        ],
        resets: &[],
        access: &[],
    };

/// `I2C_ANA_MST` — the analog I2C master on the **AHB bus**
/// ([`crate::memmap::MMIO_AHB_BASE`]): accept-and-remember here, **P5's**
/// `{block, register}` store later.
///
/// The fifth strict stop of the direct load, 143,014 cycles in:
/// `rom_chip_i2c_writeReg+0x2f` (`0x4000_4197`) writes `0x6000_E010` —
/// `esp_hal::soc::esp32::clocks` programming the BBPLL through
/// `rom_i2c_writeReg` (`clocks.rs:222-264`, `I2C_BBPLL_*.write_reg`). It was
/// "outside every region and every declared window" until the AHB window
/// was declared; the memmap carries the evidence.
///
/// # No exception, and why that is honest for this image
///
/// The ROM's spin is on **bit 25** (busy), which the guest never writes —
/// the command word it stores has bits 24 and below only — so a block that
/// remembers what was written answers the spin with 0 on the first read.
/// Nothing is pinned. The shipped image only **writes** through this master
/// (every `regi2c` use in `clocks.rs` is a `write_reg`); a `readReg` would
/// get back bits 23:16 of the last word written to that host, which is the
/// C6's *one data register* defect in waiting
/// (`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`)
/// and P5's reason to give it the `{block, register}` store the C6's
/// `i2c_ana_mst` has.
///
/// There are no PAC resets to seed: the PAC does not know this block. Every
/// register here is *modeled*.
pub fn i2c_ana_mst() -> RegFile {
    let rf = RegFile::new("I2C_ANA_MST", I2C_ANA_MST_LEN)
        .with_names(I2C_ANA_MST_NAMES)
        .with_pac_grades();
    // `with_pac_grades` calls a register with no access entry read-write and
    // therefore *documented*; nothing documents these, so every word is
    // demoted by hand to what it is.
    (0..I2C_ANA_MST_LEN / 4).fold(rf, |rf, i| {
        rf.with_grade(4 * i, lp_emu_esp_common::periph::RegGrade::Modeled)
    })
}

/// `GPIO`'s aperture, tight: the generated table runs to `+0x5cc`
/// (`func39_out_sel_cfg`).
pub const GPIO_LEN: u32 = 0x600;

/// `GPIO` — **P8's block** (the 40-pad fabric), accept-and-remember here.
///
/// The seventh strict stop of the direct load, 3,564,113 cycles in:
/// `boot_firmware+0xe02` writes `func14_in_sel_cfg` (`+0x168`) — the GPIO
/// matrix routing `U0RXD_IN` (signal 14) from pad 3, which is
/// `Uart::new(…).with_rx(peripherals.GPIO3)` in `board/esp32v3/init.rs`.
/// The matrix's `func*_in_sel_cfg` / `func*_out_sel_cfg` words, `enable`,
/// `out`, and the per-pin `pin*` words are written and read back as
/// written; every reset is the PAC's (the table carries none, so the block
/// reads 0 before it is written).
///
/// What an accept block does *not* do: `strap` (`+0x38`, read-only) reads
/// **0** — the PAC's reset for a register whose value is the board's pins at
/// reset. The mask ROM's `main` reads it fifteen times to choose its boot
/// mode, and the C6's `Gpio::new(strap_word)` is the shape P7/P8 give it
/// (the desk board boots `boot:0x13 (SPI_FAST_FLASH_BOOT)`, L0). `in_`
/// (`+0x3c`) reads 0 for the same reason: no pad is driven from outside
/// until the fabric exists.
pub fn gpio() -> RegFile {
    RegFile::new("GPIO", GPIO_LEN)
        .with_names(regs::GPIO)
        .with_pac_grades()
}

/// `UART0`'s aperture, tight: the generated table runs to `+0x7c` (`id`).
pub const UART0_LEN: u32 = 0x80;

/// `UART0` — **P6's block**, accept-and-remember here, and the reason P6
/// exists: a hello that comes out of an accept block is not a hello.
///
/// The eighth strict stop of the direct load, 3,564,269 cycles in:
/// `esp_hal::soc::…::clocks::UartInstance::configure_function_clock+0x98`
/// reads `conf0` (`+0x20`) to set `tick_ref_always_on` — the first touch of
/// `Uart::new(peripherals.UART0, Config::default().with_baudrate(921_600))`
/// in `board/esp32v3/init.rs`, which then programs `clkdiv`, `conf0`,
/// `conf1`, `mem_conf`, the interrupt registers and the FIFO resets, and
/// reads back what it wrote.
///
/// Every reset is the PAC's (`clkdiv` = `0x2b6`, `conf0` = `0x0800_001c`,
/// `conf1` = `0x6060`, `status` = 0, …). No exception is carried, and the
/// two spins the boot has are answered by the PAC's zeros:
///
/// - the mask ROM's `uart_tx_one_char` (`0x4000_9200`, which `esp-println`
///   calls for every byte) spins while `status & 0x0080_0000` — bit 23,
///   the top bit of `txfifo_cnt`, i.e. "128 bytes queued" — and a count
///   that is always 0 never blocks;
/// - esp-hal's `write_bytes` reads `status.txfifo_cnt` for room the same
///   way.
///
/// So **every byte the boot prints falls into a register that remembers
/// only the last one**, and the run reaches its `[INIT]` lines without a
/// single one leaving the chip. That is the threshold the phase file names
/// and the finding P6 is built on: UART0 needs `engine::uart` — the FIFO,
/// the shifter at baud, `txfifo_cnt` counting down, the host stream — not a
/// pin. `status` is read-only in the PAC and *modeled* here; nothing in it
/// is measured.
pub fn uart0() -> RegFile {
    RegFile::new("UART0", UART0_LEN)
        .with_names(regs::UART0)
        .with_pac_grades()
}

/// `IO_MUX`'s aperture, tight: `pin_ctrl` and the 36 pad words the PAC
/// names, the last at `+0x90` (`gpio24`).
pub const IO_MUX_LEN: u32 = 0x94;

/// `IO_MUX` — **P8's block**, accept-and-remember here.
///
/// The ninth strict stop of the direct load, 3,568,671 cycles in:
/// `boot_firmware+0x1553` reads `gpio1` (`+0x88`) — the U0TXD pad's
/// configuration word, for `Uart::new(…).with_tx(peripherals.GPIO1)`. The
/// pad words are read-modify-written (function select, pull-ups, drive)
/// and read back as written. The PAC carries **no** reset for this block,
/// so every pad reads 0 until the firmware configures it; nothing spins
/// here. P8 pushes each pad's `fun_ie` into the signal fabric as the pad's
/// input enable, the way the C6's `io_mux` does.
pub fn io_mux() -> RegFile {
    RegFile::new("IO_MUX", IO_MUX_LEN)
        .with_names(regs::IO_MUX)
        .with_pac_grades()
}

/// An SPI controller's aperture: the generated table runs to `+0x3fc`
/// (`date`); the PAC gives SPI0..SPI3 one `RegisterBlock`, so one length and
/// one table serve them all.
pub const SPI_LEN: u32 = 0x400;

/// `SPI1` — the flash controller esp-storage drives — and `SPI0`, the
/// cache's own flash port; **P7's blocks**, accept-and-remember here, and
/// the direct load's last two P3 stops.
///
/// `SPI0` is the eleventh, 72 cycles after SPI1: `esp_rom_spiflash_wait_idle
/// +0x1a` (`0x4008_38AA`) reads `SPI0.ext2` (`+0xf8`), whose `st` field is
/// the controller's state machine — the ROM's idle wait polls both
/// controllers' `ext2.st == 0` before it touches the flash, and the PAC's
/// reset (0) is *idle*, so the wait passes without a pin.
///
/// The tenth strict stop of the direct load, 3,644,210 cycles in:
/// `esp_rom_spiflash_read+0xc` (`0x4008_3C98`, in the app's own `.rwtext` —
/// esp-storage's IRAM copy of the read routine, not the mask ROM's) reads
/// `ctrl` (`+0x08`), the first touch of the flash read that mounts `lpfs`
/// (`[INIT] flash filesystem mounted` is the line after it on silicon).
///
/// Every reset is the PAC's — notably `user` (`+0x1c`) = `0x8000_0040`
/// with `usr_command` set, the value whose absence was the C6's
/// accept-block defect
/// (`docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`).
/// No exception is carried, and none could be: a flash read sets `cmd.
/// usr` (`+0x00`, bit 18) and spins until hardware clears it when the
/// transfer is done, and a block that remembers holds it set forever. That
/// spin is where the direct load stands at the end of P3
/// (`tests/boot.rs`), and it is the phase file's own example of a stop that
/// names its owner: *"SPI1 `CMD` write: no flash chip"* — P7, on
/// `engine::spi_flash`, with the chip the loader's `chip_size` already
/// describes.
pub fn spi(name: &'static str) -> RegFile {
    RegFile::new(name, SPI_LEN)
        .with_names(regs::SPI0)
        .with_pac_grades()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::EfuseIdentity;
    use lp_emu_esp_common::{Peripheral, Sandbox};

    /// The accept blocks, each with the generated table it is built from.
    fn every_block() -> Vec<(RegFile, lp_emu_esp_common::regnames::RegNames)> {
        vec![
            (dport(), regs::DPORT),
            (apb_ctrl(EfuseIdentity::default()), regs::APB_CTRL),
            (i2c_ana_mst(), I2C_ANA_MST_NAMES),
            (gpio(), regs::GPIO),
            (uart0(), regs::UART0),
            (io_mux(), regs::IO_MUX),
            (spi("SPI1"), regs::SPI0),
            (spi("SPI0"), regs::SPI0),
        ]
    }

    /// Where an accept block's power-on state differs from what the PAC
    /// states, and why. Anything not on this list is a bug in the seeding
    /// or an undocumented hand exception — the whole point of the sweep.
    ///
    /// `(block, offset, what this machine reads instead, why)`.
    const DEVIATIONS: &[(&str, u32, u32, &str)] = &[(
        "APB_CTRL",
        0x07c,
        0x8000_0000,
        "date bit 31 is esp-hal's eco_bit2 — the top bit of the chip's major revision \
             (efuse/esp32/mod.rs major_chip_version), which no eFuse word on this part \
             carries. The revision is an input to the run, like the reset cause; the desk \
             board is v3.1 and the PAC's reset for date is 0",
    )];

    #[test]
    fn the_only_deviations_from_the_pacs_resets_are_the_listed_ones() {
        let mut unlisted = Vec::new();
        for (block, names) in every_block() {
            let name = Peripheral::name(&block);
            for (off, _) in names.entries {
                if *off >= block.len_bytes() {
                    // Not mapped by this machine: reads 0 for the guest
                    // whatever the part does.
                    continue;
                }
                let want = names.reset(*off).unwrap_or(0);
                let got = block.stored(*off);
                if got == want {
                    continue;
                }
                match DEVIATIONS
                    .iter()
                    .find(|(b, o, _, _)| *b == name && o == off)
                {
                    Some((_, _, expected, _)) => assert_eq!(
                        got, *expected,
                        "{name}+{off:#05x} is a listed deviation, but it reads {got:#010x} \
                         rather than the {expected:#010x} the list says"
                    ),
                    None => unlisted.push(format!(
                        "  {name}+{off:#05x} {}: reads {got:#010x}, the PAC says {want:#010x}",
                        names.name(*off).unwrap_or("?")
                    )),
                }
            }
        }
        assert!(
            unlisted.is_empty(),
            "these accept-block registers do not read what the PAC says, and are not on \
             DEVIATIONS:\n{}",
            unlisted.join("\n")
        );
    }

    /// The other half: every listed deviation is real, and has a reason. A
    /// stale entry would otherwise sit here excusing something that no
    /// longer happens; an entry with no reason is the rule this phase exists
    /// to hold, broken.
    #[test]
    fn every_listed_deviation_is_one_and_has_a_reason() {
        for (name, off, _, why) in DEVIATIONS {
            let (block, names) = every_block()
                .into_iter()
                .find(|(b, _)| Peripheral::name(b) == *name)
                .unwrap_or_else(|| panic!("DEVIATIONS names `{name}`, which is not a block"));
            assert!(!why.is_empty(), "{name}+{off:#05x} has no reason");
            assert_ne!(
                block.stored(*off),
                names.reset(*off).unwrap_or(0),
                "{name}+{off:#05x} agrees with the PAC now; take it off the list"
            );
        }
    }

    /// Every block carries its generated names, so a trace line is readable
    /// without a second lookup, and every block is graded.
    #[test]
    fn the_blocks_carry_their_names_and_grades() {
        for (block, names) in every_block() {
            let (off, expected) = names.entries[0];
            assert_eq!(block.reg_name(off), Some(expected));
            assert!(
                block.reg_grade(off).is_some(),
                "`{}` publishes no grade table",
                Peripheral::name(&block)
            );
        }
    }

    #[test]
    fn the_analog_master_answers_the_roms_busy_spin_without_a_pin() {
        let mut sb = Sandbox::new();
        let mut m = i2c_ana_mst();
        assert_eq!(m.reg_name(0x010), Some("host4_bbpll"));
        // `rom_chip_i2c_writeReg(0x66, 4, 3, 0x1c)`: the word the ROM stores.
        let word = (1 << 24) | (0x1c << 16) | (3 << 8) | 0x66;
        sb.write(&mut m, 0x010, word);
        assert_eq!(
            sb.read(&mut m, 0x010) & (1 << 25),
            0,
            "busy is a bit the guest never sets, so remembering answers the spin"
        );
        assert_eq!(sb.read(&mut m, 0x010), word);
        // Every register is modeled: the PAC does not know this block.
        assert_eq!(
            m.reg_grade(0x010),
            Some(lp_emu_esp_common::periph::RegGrade::Modeled)
        );
    }

    #[test]
    fn dport_remembers_the_interrupt_map_it_is_written() {
        let mut sb = Sandbox::new();
        let mut d = dport();
        assert_eq!(d.reg_name(0x218), Some("core_1_intr_map0"));
        sb.write(&mut d, 0x218, 16);
        assert_eq!(sb.read(&mut d, 0x218), 16);
        // D4's register, at the PAC's reset: `pro_cache_enable` (bit 3) is
        // whatever the PAC says, and P4 reads it, not this file.
        assert_eq!(d.reg_name(0x040), Some("pro_cache_ctrl"));
        assert_eq!(d.stored(0x040), regs::DPORT.reset(0x040).unwrap_or(0));
    }
}
