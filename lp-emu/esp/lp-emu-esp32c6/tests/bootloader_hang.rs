//! The first-flash bootloader hang, reproduced.
//!
//! A board whose previous firmware gated `LPPERI_CLK_EN` bit 29
//! (`LP_ANA_I2C_CK_EN`) hangs the ESP-IDF second-stage bootloader on
//! `LP_I2C_ANA_MST.i2c0_ctrl` bit 25 (busy) inside
//! `bootloader_clock_configure()`, **before its first console line**. Silicon
//! does it every 0.4 s for ever; this file asks the emulator to do it once,
//! and asks a clean board of the same image to boot as it always has.
//!
//! `docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`,
//! `docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md`.
//!
//! P1 owns AC1 and the control; **P2 owns the loop** (AC2) — the MWDT0
//! flash-boot watchdog ending each hang with `rst:0x7 (TG0_WDT_HPSYS)` and a
//! `Saved PC` inside the bootloader. P3 extends it with the cure between two
//! reboots (AC3) and the power cycle (AC4).
//!
//! `#[ignore]`d for the usual reason: it needs the reference merged image
//! (`test_support`), so `just test-emu-c6` is what runs it.

use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::image::{ImageSegment, MergedImage};
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, Uart0Sink,
};
use lp_emu_esp32c6::memmap::periph as base;
use lp_emu_esp32c6::periph::lp_i2c_ana_mst::{BUSY, I2C0_CTRL};
use lp_emu_esp32c6::periph::lp_peri::{CLK_EN_RESET, LP_ANA_I2C_BIT};
use lp_emu_esp32c6::test_support::{ReferenceImage, merged_image, reference_image, skip_notice};

/// The image both halves boot: the shipped feature set at the commit the
/// silicon `boot-idle-flash` transcript was captured from, the same one
/// `rom_up_boot` uses. Its second-stage bootloader is espflash 3.3.0's
/// bundled one — the binary the defect disassembled.
const IMAGE: ReferenceImage = ReferenceImage::SHIPPED_USB_SILICON;

/// `LPPERI_CLK_EN` as the bench induced it by hand on 2026-09-08 to
/// reproduce the production failure (the first-flash defect's "Confirmed on
/// hardware through Studio"). Bit 29 clear; everything else is a plausible
/// gate word.
const INDUCED_CLK_EN: u32 = 0x5f00_0000;

/// The bootloader's first console line, and the one AC1 says must not
/// appear.
const BANNER: &str = "2nd stage bootloader";

/// Two seconds of emulated time. Silicon's loop period is ≈0.4 s, and on a
/// clean board this image prints [`BANNER`] within the first 25 ms.
const GATE_US: u64 = 2_000_000;

/// Half a second more, to ask whether the hart is still in the same three
/// instructions.
const LATER_US: u64 = 2_500_000;

/// Silicon's loop period: the wedged XIAO "boot-loops about every 0.4 s"
/// (`docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md`, an
/// eyeballed figure, not an instrumented one). AC2 asks the emulator to land
/// within 2× of it.
const SILICON_LOOP_US: u64 = 400_000;

/// The line the ROM prints for a TIMG0 watchdog reset, from its own
/// reset-reason table at `0x4004_a8e8`.
const TG0_BANNER: &str = "rst:0x7 (TG0_WDT_HPSYS)";

/// `Saved PC:0x4086ed7a`, printed by the wedged XIAO on every one of its
/// reset cycles (both defect entries). The reference image's second-stage
/// bootloader is the **same binary** — espflash 3.3.0's bundled one, pinned
/// by `scripts/emu/build-merged-image.sh` — so the address is a fact about
/// a program this test can check itself against, the way `rom_up_boot`
/// compares the three `load:` lines literally. If this assertion fails, the
/// bootloader changed; nothing about the model did.
const SILICON_SAVED_PC: u32 = 0x4086_ed7a;

/// The segment the defect's ROM printed that `Saved PC` inside:
/// `load:0x4086e610,len:0x2d68`.
const SILICON_SPIN_SEGMENT: (u32, u32) = (0x4086_e610, 0x2d68);

/// `entry 0x4086c410` on the wedged board. (A clean XIAO's *own* factory
/// bootloader is a different build and prints `entry 0x4086c110`; this
/// image's is the wedged board's.)
const SILICON_ENTRY: u32 = 0x4086_c410;

struct Run {
    machine: Esp32C6Machine,
    outcome: Outcome,
}

/// A ROM-up boot of the reference merged image with `clk_en` seeded, run to
/// the banner or the gate.
fn boot(clk_en: u32) -> Result<Run, String> {
    let mut machine = build(clk_en, false)?;
    let outcome = machine.run_until(&StopCondition::after_micros(GATE_US).exit_on(BANNER));
    Ok(Run { machine, outcome })
}

/// The machine both halves boot, not yet run. The builder incantation is
/// `rom_up_boot`'s, with two lines added. `reboot_on_reset` is what turns the
/// MWDT0 reset the hang provokes into a reboot instead of the end of the run.
fn build(clk_en: u32, reboot_on_reset: bool) -> Result<Esp32C6Machine, String> {
    let merged = merged_image(&IMAGE)?;
    let elf = reference_image(&IMAGE)?;
    let len = std::fs::metadata(&merged)
        .map_err(|e| format!("{}: {e}", merged.display()))?
        .len() as u32;
    Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        // A symbol table and a cross-check reference; the bytes the machine
        // runs come out of the chip.
        .app(AppSource::Path(elf))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(merged))
        .flash_len(len)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .uart0(Uart0Sink::Memory)
        .usb_host(lp_emu_esp32c6::machine::UsbHost::Attached { draining: true })
        .lp_peri_clk_en(clk_en)
        .reboot_on_reset(reboot_on_reset)
        .build()
        .map_err(|e| e.to_string())
}

/// The bootloader's own segments, read out of the merged image's header
/// rather than hard-coded: the reference image's bootloader is not
/// necessarily the build the defect's board carried.
fn bootloader_segments() -> Vec<ImageSegment> {
    let bytes = std::fs::read(merged_image(&IMAGE).expect("the merged image")).expect("readable");
    MergedImage::parse(&bytes)
        .expect("the merged image parses")
        .bootloader
        .segments
}

fn bootloader_entry() -> u32 {
    let bytes = std::fs::read(merged_image(&IMAGE).expect("the merged image")).expect("readable");
    MergedImage::parse(&bytes)
        .expect("the merged image parses")
        .bootloader
        .entry
}

/// Which loaded segment holds `pc`, and what the image says about it.
fn segment_of(segments: &[ImageSegment], pc: u32) -> Option<(usize, &ImageSegment)> {
    segments
        .iter()
        .enumerate()
        .find(|(_, s)| pc >= s.vaddr && pc < s.vaddr + s.len)
}

fn console(machine: &Esp32C6Machine) -> String {
    String::from_utf8_lossy(&machine.usb_sj().bytes()).into_owned()
}

/// **AC1.** The induced board hangs in the bootloader, on this register,
/// before the bootloader says anything.
#[test]
#[ignore = "needs the reference merged image; `just test-emu-c6`"]
fn an_induced_board_hangs_in_the_bootloader_on_the_lp_analog_masters_busy_bit() {
    let mut run = match boot(INDUCED_CLK_EN) {
        Ok(run) => run,
        Err(reason) => {
            skip_notice(
                "an_induced_board_hangs_in_the_bootloader_on_the_lp_analog_masters_busy_bit",
                &reason,
            );
            return;
        }
    };

    // 1. The run ended the way the wedged board's boot did: no banner, no
    //    fault, no strict-bus refusal — **the flash-boot watchdog**. Until
    //    P2 this was the emulated deadline, because the MWDT's expiry was
    //    not modelled and an emulated wedge spun for ever. `reboot_on_reset`
    //    is off here, so the reset ends the run and names itself; the loop
    //    test below is the one that lets it reboot.
    let Outcome::Reset { cycle, source, .. } = run.outcome else {
        panic!(
            "the induced board should have been reset by MWDT0, not {:?}\nconsole:\n{}",
            run.outcome,
            console(&run.machine)
        )
    };
    assert_eq!(source, "TIMG0 MWDT flash-boot protection");
    let us = cycle / lp_emu_esp32c6::memmap::CYCLES_PER_US;
    println!("the flash-boot watchdog bit at cycle {cycle} ({us} us)");
    assert!(
        us.abs_diff(SILICON_LOOP_US) < SILICON_LOOP_US,
        "the first hang lasted {us} us; silicon's whole loop was ≈{SILICON_LOOP_US} us"
    );

    // 2. …and the bootloader never introduced itself.
    let text = console(&run.machine);
    assert!(
        !text.contains(BANNER),
        "the induced board printed `{BANNER}`:\n{text}"
    );
    // It did get as far as the ROM handing over, which is what makes this a
    // *bootloader* hang rather than a chip that never started.
    assert!(
        text.contains("ESP-ROM:esp32c6-"),
        "the ROM banner is missing, so this is not the hang:\n{text}"
    );
    assert!(
        text.contains("entry "),
        "the ROM never jumped to the bootloader:\n{text}"
    );

    // 3. The hart is inside a bootloader segment — the address a `Saved PC`
    //    would name. The bounds come from the image's own header.
    let segments = bootloader_segments();
    let pc = run.machine.pc();
    let (n, seg) = segment_of(&segments, pc).unwrap_or_else(|| {
        panic!(
            "the spin PC {pc:#010x} is in none of the bootloader's segments: {}",
            segments
                .iter()
                .map(|s| format!("{:#010x}..+{:#x}", s.vaddr, s.len))
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    let entry = bootloader_entry();
    println!(
        "spin PC {pc:#010x} in bootloader segment {n} (the {}{} `load:` line): \
         {:#010x}..{:#010x}, len {:#x}; entry {entry:#010x}",
        n + 1,
        match n + 1 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
        seg.vaddr,
        seg.vaddr + seg.len,
        seg.len,
    );

    // …and it is the segment the wedged board's ROM named, in the image
    // whose bootloader is that board's. Three facts, one program.
    assert_eq!(
        (seg.vaddr, seg.len),
        SILICON_SPIN_SEGMENT,
        "the spin segment is not the one the defect recorded — the reference \
         image's bootloader is not espflash 3.3.0's any more"
    );
    assert_eq!(entry, SILICON_ENTRY, "a different second-stage bootloader");
    assert!(
        pc.abs_diff(SILICON_SAVED_PC) <= 8,
        "our spin stopped at {pc:#010x}; the wedged board's ROM printed \
         `Saved PC:{SILICON_SAVED_PC:#010x}`. The same three instructions, \
         or a different loop?"
    );

    // 4. The register it is spinning on. The wedged board read
    //    `i2c0_ctrl = 0x0200_0e6d`: busy over `{block 0x6d, register 0x0e}`,
    //    a read — the bootloader's first `regi2c` transaction.
    let ctrl = run
        .machine
        .peek_word(base::LP_I2C_ANA_MST + I2C0_CTRL)
        .expect("LP_I2C_ANA_MST answers the bus");
    assert_ne!(
        ctrl & BUSY,
        0,
        "i2c0_ctrl {ctrl:#010x} is not busy, so this is some other hang"
    );
    println!("LP_I2C_ANA_MST.i2c0_ctrl = {ctrl:#010x}");

    // …and the address is live in a register, which is what the defect's
    // disassembly shows: `addi a0, a4, 0x400` then `lw a5, 0x0(a0)`.
    let regs = run.machine.registers();
    assert!(
        regs.iter().any(|r| *r == base::LP_I2C_ANA_MST),
        "no register holds {:#010x} at the reset: {regs:#010x?}",
        base::LP_I2C_ANA_MST
    );

    // 5. It is a *spin*: half a second later the hart has not left those
    //    few instructions. Nothing re-armed the watchdog after the reset it
    //    was not allowed to perform, so this stretch runs to its deadline.
    let outcome = run
        .machine
        .run_until(&StopCondition::after_micros(LATER_US).exit_on(BANNER));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "{outcome:?} half a second on"
    );
    let later = run.machine.pc();
    assert!(
        later.abs_diff(pc) <= 16,
        "the hart moved from {pc:#010x} to {later:#010x}: not a spin"
    );
    assert_eq!(
        segment_of(&segments, later).map(|(n, _)| n),
        Some(n),
        "and it is still in segment {n}"
    );
}

/// The control: the same image, the same run, a clean board's `clk_en`.
/// Without this the test above only says "something went wrong".
#[test]
#[ignore = "needs the reference merged image; `just test-emu-c6`"]
fn the_same_image_on_a_clean_board_reaches_the_bootloaders_banner() {
    let run = match boot(CLK_EN_RESET) {
        Ok(run) => run,
        Err(reason) => {
            skip_notice(
                "the_same_image_on_a_clean_board_reaches_the_bootloaders_banner",
                &reason,
            );
            return;
        }
    };
    let text = console(&run.machine);
    assert!(
        matches!(run.outcome, Outcome::ExitMatched { .. }),
        "a clean board should have printed `{BANNER}`: {:?}\n{text}",
        run.outcome
    );
    let line = text
        .lines()
        .find(|l| l.contains(BANNER))
        .expect("the banner line");
    println!("clean board: {}", line.trim());

    // The gate is down on the induced board and up here, which is the whole
    // difference between the two runs.
    assert_ne!(CLK_EN_RESET & LP_ANA_I2C_BIT, 0);
    assert_eq!(INDUCED_CLK_EN & LP_ANA_I2C_BIT, 0);
}

/// **AC2, P2's half.** With the reset performed instead of reported, the
/// induced board does what the XIAO on the desk did for an hour: boot, wedge,
/// get shot by MWDT0, and come back to the same wedge, printing
/// `rst:0x7 (TG0_WDT_HPSYS)` and the same `Saved PC` every time.
///
/// **Why the second boot re-hangs, which is the thing to be sure of.** Until
/// P3 every reboot is a whole restore of the power-on snapshot, so an
/// LP-domain gate would be *undone* by one — except that the induced
/// `clk_en` **is** the power-on value (P1 seeds it with `RegFile::poke` in
/// `LpPeri::new`, before the snapshot is taken). So the restore puts the
/// induced board back, not a clean one, and the loop closes for the right
/// reason by accident of the seeding rather than by the domain model. P3's
/// AC4 is what tells the two apart: there, a `power_cycle()` must ALSO come
/// back induced while the cure survives a plain `reboot()`.
///
/// AC2's other half — that `--reboot-on-reset` is what the CLI offers for
/// this — is P3's.
#[test]
#[ignore = "needs the reference merged image; `just test-emu-c6`"]
fn the_induced_board_boot_loops_on_the_flash_boot_watchdog_with_silicons_banner() {
    let mut machine = match build(INDUCED_CLK_EN, true) {
        Ok(m) => m,
        Err(reason) => {
            skip_notice(
                "the_induced_board_boot_loops_on_the_flash_boot_watchdog_with_silicons_banner",
                &reason,
            );
            return;
        }
    };

    // One run, one budget, and let it loop. **How the period is measured:**
    // the budget is an absolute cycle bound that a reboot rebases onto the
    // new clock (`machine.rs`, the `remaining` rebase), and a reboot puts the
    // clock back to zero — so the leftover at the deadline is
    // `budget - reboots × period` and the period falls out exactly, without
    // needing a marker in the console. It also means the period is measured
    // from reset vector to watchdog bite, the ROM's own boot **inside** it,
    // which is the same span silicon's "every 0.4 s" covers.
    let budget = GATE_US * lp_emu_esp32c6::memmap::CYCLES_PER_US;
    let outcome = machine.run_until(&StopCondition::after_micros(GATE_US).exit_on(BANNER));
    let text = console(&machine);
    let Outcome::Deadline { cycle: leftover } = outcome else {
        panic!("the loop should have run the budget out, not ended: {outcome:?}\n{text}")
    };
    let reboots = machine.reboots();
    assert!(
        reboots >= 3,
        "AC2 wants three consecutive boots; got {reboots} reboots:\n{text}"
    );
    let period = (budget - leftover) / reboots;
    let period_us = period / lp_emu_esp32c6::memmap::CYCLES_PER_US;
    assert!(
        !text.contains(BANNER),
        "one of these boots reached the bootloader's banner:\n{text}"
    );

    // Every reset says it was TIMG0's, in the ROM's own words. The first boot
    // of all prints the run's own `--reset-cause` instead, so there are
    // exactly as many of these as there were reboots.
    let banners = text.matches(TG0_BANNER).count() as u64;
    assert_eq!(
        banners, reboots,
        "one `{TG0_BANNER}` per reboot; found {banners} for {reboots}:\n{text}"
    );
    assert_eq!(
        text.matches("rst:0x15 (USB_UART_HPSYS)").count(),
        1,
        "only the FIRST boot is the run's own reset cause:\n{text}"
    );

    // And every `Saved PC` is an address inside the bootloader segment the
    // spin lives in — the assertion the defect's own reading rests on
    // ("`Saved PC:0x4086ed7a` is a **bootloader** address, not an app one").
    let segments = bootloader_segments();
    let saved: Vec<u32> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("Saved PC:0x"))
        .filter_map(|hex| u32::from_str_radix(&hex[..8.min(hex.len())], 16).ok())
        .collect();
    assert_eq!(
        saved.len() as u64,
        reboots,
        "one `Saved PC` per reboot, and none on the cold first boot:\n{text}"
    );
    for pc in &saved {
        let (n, seg) = segment_of(&segments, *pc)
            .unwrap_or_else(|| panic!("Saved PC {pc:#010x} is in no bootloader segment"));
        assert_eq!(
            (seg.vaddr, seg.len),
            SILICON_SPIN_SEGMENT,
            "Saved PC {pc:#010x} landed in segment {n}, not the spin's"
        );
        assert!(
            pc.abs_diff(SILICON_SAVED_PC) <= 8,
            "Saved PC {pc:#010x} against the board's {SILICON_SAVED_PC:#010x}"
        );
    }
    // The board printed **one** address for an hour; the emulator prints two,
    // `0x4086ed7c` and `0x4086ed7e`, and that is the model being honest rather
    // than wrong. Silicon's ASSIST_DEBUG recorder samples the real pdebug PC
    // continuously, so it captures whichever of the loop's three instructions
    // the hart was on at the reset edge, and on that board it was always the
    // `lw` at `…7a`. The emulator writes the PC at the **slice boundary**
    // where the machine takes the reset request, which is quantised to
    // `MAX_SLICE_CYCLES` — so it lands on the `and` or the `bnez` instead.
    // The assertion is therefore "the same three instructions", which is what
    // the defect's reading actually rests on.
    let (lo, hi) = (
        *saved.iter().min().expect("at least one"),
        *saved.iter().max().expect("at least one"),
    );
    assert!(
        hi - lo <= 8,
        "these are not all one loop's worth of instructions: {saved:#010x?}"
    );

    println!(
        "{reboots} reboots in {GATE_US} us emulated ({leftover} cycles left): \
         loop period {period} cycles = {period_us} us, against silicon's \
         ≈{SILICON_LOOP_US} us"
    );
    for line in text
        .lines()
        .filter(|l| l.contains(TG0_BANNER) || l.contains("Saved PC:"))
        .take(4)
    {
        println!("  {}", line.trim());
    }
    assert!(
        period_us * 2 >= SILICON_LOOP_US && period_us <= SILICON_LOOP_US * 2,
        "the loop period is {period_us} us, not within 2× of silicon's \
         ≈{SILICON_LOOP_US} us"
    );
}
