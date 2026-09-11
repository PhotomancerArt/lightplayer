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

/// `GPIO`'s aperture: the **block's** `0x1000`, not the generated table's
/// `+0x5cc` (`func39_out_sel_cfg`).
///
/// ⚠️ P3 and P4 had this tight at `0x600`, which is where the PAC's names
/// stop. The ROM-up path reaches past them: `SelectSpiFunction+0xa4`
/// (`0x4006_2028`) read-modify-writes **bit 31 of `0x3FF4_4F24`** while it
/// chooses between the SPI flash pins and GPIO —
///
/// ```text
/// 4006201f:  l32r   a2, (0x3ff44f24)
/// 40062022:  l32r   a3, (0x7fffffff)
/// 40062028:  l32i.n a8, a2, 0
/// 4006202a:  and    a3, a8, a3        ; the SPI-pin arm clears bit 31
/// …
/// 400620d5:  l32i.n a8, a2, 0         ; the GPIO arm sets it
/// 400620d7:  or     a3, a8, a3
/// 400620dd:  s32i.n a3, a2, 0
/// ```
///
/// — a word inside this block's `0x1000` that the `esp32` PAC does not name.
/// The register is accepted and remembered like every other here; the trace
/// prints it as `GPIO+0xf24` because there is no name to print, and a name
/// invented for it would be a datasheet claim this repository cannot make.
/// The ROM reads 0, clears a bit that is already clear, and writes it back,
/// so nothing spins.
///
/// P8 owns the GPIO fabric and may narrow this again per register; until
/// then the aperture is the block, and the words the PAC names are the ones
/// with names.
pub const GPIO_LEN: u32 = 0x1000;

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
/// `GPIO.strap` (`+0x38`) — the strapping-pin register the mask ROM reads to
/// choose a boot mode.
pub const GPIO_STRAP: u32 = 0x038;

/// What `strap` reads on the desk board: **`0x13`**, `SPI_FAST_FLASH_BOOT`.
///
/// ⚠️ **A deviation from the PAC, and an input to the run** — the third, with
/// the reset cause and the chip revision. The PAC carries no reset for this
/// register, so it would read 0, and 0 is not "no straps": it is the SDIO
/// boot mode, and the mask ROM takes it seriously. A ROM-up boot with a
/// zeroed `strap` walks into `slc_init_attach` and stops on the SLC block at
/// `0x3FF5_8040` — a whole boot path this board is not on.
///
/// The value is L0's, and the ROM's own banner is what makes it citable
/// rather than inferred. `main` (`0x4000_78E5`) loads the strap register and
/// passes it **raw** as the third argument of
///
/// ```text
/// 3ff9e99b  "\nrst:0x%x (%s),boot:0x%x (%s)\n"
/// 400078e5:  l32r   a5, (0x3ff44038)     ; GPIO.strap
/// 400078ee:  l32i.n a13, a5, 0           ; → the `boot:0x%x` argument
/// 400078fa:  call8  <ets_printf>
/// ```
///
/// and `../bench.md`'s capture of that line reads `boot:0x13
/// (SPI_FAST_FLASH_BOOT)`. So `0x13` is not a decoded mode number that had to
/// be reverse-engineered into strapping bits — it is the register, printed.
///
/// P8 owns the GPIO fabric and is where this becomes a `--strap` parameter
/// with a download-mode value beside it, the way the C6's `Strap` is.
pub const GPIO_STRAP_SPI_FAST_FLASH_BOOT: u32 = 0x13;

pub fn gpio() -> RegFile {
    RegFile::new("GPIO", GPIO_LEN)
        .with_names(regs::GPIO)
        .with_reset(GPIO_STRAP, GPIO_STRAP_SPI_FAST_FLASH_BOOT)
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

/// `RTC_IO`'s aperture, tight: the generated table runs to `+0xc8` (`date`).
pub const RTC_IO_LEN: u32 = 0xd0;

/// `RTC_IO` — **P8's block** (it is the other half of the pad fabric),
/// accept-and-remember here.
///
/// The first strict stop of the **ROM-up** path past P6's `Uart_Init`, at
/// cycle 7,430: `gpio_pad_unhold+0x1f9` (`0x4000_A67D`) reads
/// `dig_pad_hold` (`+0x74`, so `0x3FF4_8474`) to clear one pad's hold bit.
/// The ROM's own literal pool carries that address two words along from the
/// one the PAC-ignorant disassembly annotates:
///
/// ```text
/// 4000a460  74c1f93f c880f43f 7484f43f ffbfffff
///                             ^^^^^^^^ 0x3ff48474
/// ```
///
/// (⚠️ `xtensa-esp32-elf-objdump` annotates the `l32r` at `0x4000_A677` as
/// loading `0x4000_A464` = `0x3FF4_80C8`, which is RTC_CNTL's `+0xc8` and is
/// already modelled. The running machine loads `0x4000_A468` = `0x3FF4_8474`
/// instead. The two candidates are both plausible reads for this routine;
/// the address in the stop is the one the run made, and it is the one in
/// this block. Recorded rather than smoothed over.)
///
/// Every reset is the PAC's, and the ones it carries are the analog pads'
/// (`pad_dac1`/`pad_dac2` `0x8000_0000`, `xtal_32k_pad` `0x8410_0010`,
/// `touch_cfg` `0x6600_0000`, `touch_pad0` `0x5200_0000`, `touch_pad8`
/// `0x0200_0000`). **`dig_pad_hold` resets to 0** — nothing is held — which
/// is what lets `gpio_pad_unhold` read-modify-write it and move on. Nothing
/// spins here.
pub fn rtc_io() -> RegFile {
    RegFile::new("RTC_IO", RTC_IO_LEN)
        .with_names(regs::RTC_IO)
        .with_pac_grades()
}

/// `SENS`'s aperture, tight: the generated table runs to `+0x0fc`.
pub const SENS_LEN: u32 = 0x100;

/// `SENS` at `0x3FF4_8800` — the SAR/ADC block, accept-and-remember.
///
/// The **ESP-IDF second-stage bootloader's** first stop on the ROM-up path,
/// at cycle 9,284,455 and five lines into its own log: `0x4008_0B10` reads
/// `+0x2c` (`sar_read_ctrl2`). This is
/// `bootloader_random_enable()` — the `Enabling RNG early entropy source`
/// step — which puts the SAR ADCs into a free-running mode so the RNG has
/// something to stir (`bootloader_support/src/bootloader_random_esp32.c`).
///
/// It is remembered and nothing else. **The entropy is the machine's seeded
/// PRNG** (plan PD5: deterministic by design), which is loader item 5 on the
/// direct path and is the same statement here — a boot that stirred real
/// noise would not be the same run twice, and being the same run twice is
/// what this emulator is for. Every reset is the PAC's.
///
/// P3's §4.3 predicted this block: the ROM's `main` reads `SENS` through its
/// **AHB** address (`0x6000_88xx`), and the bootloader reaches the same
/// state through the DPORT one. Both decode to this block (DD38).
pub fn sens() -> RegFile {
    RegFile::new("SENS", SENS_LEN)
        .with_names(regs::SENS)
        .with_pac_grades()
}

/// `I2S0`'s aperture, tight: the generated table runs to `+0x0fc`.
pub const I2S0_LEN: u32 = 0x100;

/// `I2S0` at `0x3FF4_F000` — **the RNG's entropy path**, accept-and-remember.
///
/// The second stop the ESP-IDF bootloader makes on the ROM-up path, at cycle
/// 9,284,567 and 112 cycles after [`sens`]: `0x4008_0C38` reads `+0xb0`.
/// `bootloader_random_enable()` on this chip does not touch a random
/// register at all — it puts the SAR ADCs into a continuous mode and reads
/// them **through I2S0's DMA**, which is what stirs the hardware RNG
/// (`bootloader_support/src/bootloader_random_esp32.c`). So an audio block
/// is on the boot path, and it is on it for a reason that has nothing to do
/// with audio; nothing here models one.
///
/// Remembered and nothing else, every reset the PAC's. **The entropy is the
/// machine's seeded PRNG** (plan PD5), which is loader item 5 on the direct
/// path and the same statement here: a boot that stirred real noise would
/// not be the same run twice.
pub fn i2s0() -> RegFile {
    RegFile::new("I2S0", I2S0_LEN)
        .with_names(regs::I2S0)
        .with_pac_grades()
}

/// `RMT`'s aperture: the **register block plus its channel RAM**.
///
/// The generated table runs to `+0x0fc` (`date`), but the block's window does
/// not stop there: the eight channels' 64-word transmit buffers start at
/// [`crate::memmap::periph::RMT_RAM`] (`0x3FF5_6800`, so `+0x800`) and run
/// `8 * 64 * 4 = 0x800` bytes to `+0x1000`. One aperture covers both, which
/// is what the firmware needs — `lp-ws281x` writes its symbols straight into
/// the RAM window and its configuration into the registers.
pub const RMT_LEN: u32 = 0x1000;

/// `RMT` at `0x3FF5_6000` — **M4's block**, accept-and-remember here.
///
/// The **last** strict stop of both boot paths, and the only block left
/// between `[INIT] flash filesystem mounted` and the idle heartbeat: at cycle
/// 5,640,047 on the direct load and 65,360,003 on the ROM-up path,
/// `boot_firmware+0x39a8` reads `ch0conf1` (`+0x024`) — the read half of the
/// read-modify-write esp-hal's `Channel::new` does when `init_board` claims
/// the first of the board's wire channels.
///
/// Every reset is the PAC's, and this block is the one place in M3 where
/// that matters for more than a spin: `chNconf0` resets to `0x3110_0002` and
/// `chNconf1` to `0x0000_0f20`, so the divider, the memory-block count and
/// the idle level esp-hal read-modify-writes are the part's own, not zero.
/// `chNstatus` (`+0x060`, read-only in the PAC) resets to 0 and nothing
/// spins on it: esp-hal's transmit path waits on the **interrupt**
/// (`int_raw.chN_tx_end`), not on the status word, so a channel that is
/// configured and never started simply never finishes — which is exactly
/// what M3's single-core, no-waveform machine should look like.
///
/// # What this is NOT
///
/// **No waveform is produced and none is decoded.** M3's pin class stays
/// `modeled` for that reason (the crate README's table). A routed RMT signal
/// reaches [`super::gpio`]'s fabric route and then nothing drives it, so
/// [`lp_emu_esp_common::pins::Fabric::signal_level`] stays low for every
/// `RMT_SIG_n` — which is what makes `no peripheral SIGNAL reaches a pad` an
/// assertable boot fact rather than an absence nobody checked. M4 is the
/// phase that gives the channels their time base, their symbol reader over
/// the RAM window, `int_raw.chN_tx_end` and the WS281x decoder behind them.
pub fn rmt() -> RegFile {
    RegFile::new("RMT", RMT_LEN)
        .with_names(regs::RMT)
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
            (gpio(), regs::GPIO),
            (io_mux(), regs::IO_MUX),
            (spi("SPI1"), regs::SPI0),
            (spi("SPI0"), regs::SPI0),
            (rtc_io(), regs::RTC_IO),
            (sens(), regs::SENS),
            (i2s0(), regs::I2S0),
            (rmt(), regs::RMT),
        ]
    }

    /// Where an accept block's power-on state differs from what the PAC
    /// states, and why. Anything not on this list is a bug in the seeding
    /// or an undocumented hand exception — the whole point of the sweep.
    ///
    /// `(block, offset, what this machine reads instead, why)`.
    const DEVIATIONS: &[(&str, u32, u32, &str)] = &[
        (
            "GPIO",
            GPIO_STRAP,
            GPIO_STRAP_SPI_FAST_FLASH_BOOT,
            "the strapping pins are a board fact, and the PAC carries no reset for them. \
             Zero is not `no straps`: it is the SDIO boot mode, and the mask ROM walks into \
             `slc_init_attach` on it. The desk board's ROM banner prints the register raw \
             as `boot:0x13 (SPI_FAST_FLASH_BOOT)` — see GPIO_STRAP_SPI_FAST_FLASH_BOOT",
        ),
        (
            "APB_CTRL",
            0x07c,
            0x8000_0000,
            "date bit 31 is esp-hal's eco_bit2 — the top bit of the chip's major revision \
             (efuse/esp32/mod.rs major_chip_version), which no eFuse word on this part \
             carries. The revision is an input to the run, like the reset cause; the desk \
             board is v3.1 and the PAC's reset for date is 0",
        ),
    ];

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
