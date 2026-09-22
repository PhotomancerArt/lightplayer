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
//! `Saved PC` inside the bootloader. **P3 owns the pair at the end** (AC3,
//! AC4): the cure poked onto the bus between two reboots frees the board
//! *because the LP domain survived the reset*, and a power cycle hands the
//! same wedged board straight back *because the induced gate word is the
//! power-on value*. Those two facts together are what tells P3's domain model
//! apart from P2's accident of seeding — see
//! [`the_cure_survives_a_reboot_and_the_next_boot_reaches_the_app`] and
//! [`a_power_cycle_hands_the_induced_board_back_induced`].
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
use lp_emu_esp32c6::periph::lp_peri::{CLK_EN, CLK_EN_RESET, LP_ANA_I2C_BIT, RESET_EN};
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

// ---- P3: the cure, and the power cycle ---------------------------------

/// The line the ROM prints for a power-on, from the same table as
/// [`TG0_BANNER`]. `1` is the value `__pre_init` compares against before it
/// zeroes `.rtc_fast.persistent`.
const POWERON_BANNER: &str = "rst:0x1 (POWERON)";

/// Long enough for exactly **one** watchdog cycle and no more: the flash-boot
/// bite lands at 325 000 us, so a 400 000 us budget carries one reboot and
/// then runs out. That makes "the state of the board after its first reboot"
/// a deterministic place to stand, rather than "somewhere in the loop".
const ONE_CYCLE_US: u64 = 400_000;

/// How long the cured board is given to boot and reach its first heartbeat.
/// `rom_up_boot` reaches `[INIT] fw-esp32 initialized` inside two emulated
/// seconds and the first `[stack] heartbeat` lands at five, so six is the
/// heartbeat with room.
const APP_US: u64 = 6_000_000;

/// The app's own first line, and the line that says it is *running* rather
/// than initialising. Both are `rom_up_boot`'s.
const APP_FIRST: &str = "[INIT] Initializing board";
const HEARTBEAT: &str = "[stack] heartbeat: high-water ";

/// **The flasher's cure, poked onto the bus** — `lpa_link`'s
/// `host_esp32_flash.rs:606-622` and its browser twin, bench-proven
/// 2026-09-06/08 and the only thing short of a power cycle that frees a
/// wedged board:
///
/// ```text
/// LPPERI_CLK_EN   |= 1 << 29     turn the analog master's clock back on
/// LPPERI_RESET_EN |= 1 << 29     assert its reset
/// LPPERI_RESET_EN &= ~(1 << 29)  release it
/// ```
///
/// Three word writes through [`Esp32C6Machine::poke_word`], which is the
/// bus's own decode — the same path a ROM-download-mode `write_reg` would
/// take when T4 gives the emulator one. Nothing reaches behind a block.
fn poke_the_cure(machine: &mut Esp32C6Machine) {
    let clk = base::RNG + CLK_EN;
    let reset = base::RNG + RESET_EN;
    let was = machine.peek_word(clk).expect("LP_PERI answers the bus");
    assert_eq!(
        was & LP_ANA_I2C_BIT,
        0,
        "the board is not wedged: clk_en {was:#010x} already has bit 29"
    );
    assert!(machine.poke_word(clk, was | LP_ANA_I2C_BIT));
    let held = machine.peek_word(reset).expect("LP_PERI answers the bus");
    assert!(machine.poke_word(reset, held | LP_ANA_I2C_BIT));
    assert!(machine.poke_word(reset, held & !LP_ANA_I2C_BIT));
    // What the flasher then re-reads, and the reason the cure is three
    // writes rather than one: the clock alone does not clear a latched busy.
    let ctrl = machine
        .peek_word(base::LP_I2C_ANA_MST + I2C0_CTRL)
        .expect("LP_I2C_ANA_MST answers the bus");
    assert_eq!(
        ctrl & BUSY,
        0,
        "after the cure the master still reads busy ({ctrl:#010x})"
    );
}

/// Run the induced board until its first watchdog reboot, and hand it back
/// mid-wedge — the place both P3 tests start from.
fn at_the_first_reboot(name: &str) -> Option<Esp32C6Machine> {
    let mut machine = match build(INDUCED_CLK_EN, true) {
        Ok(m) => m,
        Err(reason) => {
            skip_notice(name, &reason);
            return None;
        }
    };
    let outcome = machine.run_until(&StopCondition::after_micros(ONE_CYCLE_US).exit_on(BANNER));
    let text = console(&machine);
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the first cycle should have run its budget out: {outcome:?}\n{text}"
    );
    assert_eq!(
        machine.reboots(),
        1,
        "exactly one watchdog cycle fits in {ONE_CYCLE_US} us:\n{text}"
    );
    assert_eq!(machine.power_cycles(), 0, "nothing cut the power");
    assert_eq!(
        text.matches(TG0_BANNER).count(),
        1,
        "the second boot's banner is the watchdog's:\n{text}"
    );
    assert!(
        !text.contains(BANNER),
        "neither boot reached the bootloader's banner:\n{text}"
    );
    Some(machine)
}

/// **AC3, and the first half of the domain model.** The cure poked onto the
/// bus between two reboots frees the board: the next boot prints
/// `2nd stage bootloader` and runs the app to its first heartbeat.
///
/// **Why this is the assertion that matters.** P2's loop already closed, but
/// for an accident: the induced `clk_en` *is* the power-on value, so a
/// whole-snapshot restore put the wedge back rather than clearing it. Nothing
/// about that run distinguished "the LP domain survived the reset" from "the
/// restore happened to restore a wedged board". This does. The cure is
/// written into LP-domain state and then a plain `reboot()` follows it — so a
/// clean boot afterwards is only possible if the reboot **kept** those
/// blocks. Before P3 this test's `reboot()` would have restored `LPPERI` from
/// the power-on snapshot, thrown the cure away, and re-hung.
///
/// The sequence is also the flasher's own, in order: hold the board, write
/// the three registers, reset it. `lpa_link` does exactly this over
/// ROM-download mode, and the reset is espflash's `--after hard-reset`.
#[test]
#[ignore = "needs the reference merged image; `just test-emu-c6`"]
fn the_cure_survives_a_reboot_and_the_next_boot_reaches_the_app() {
    let Some(mut machine) =
        at_the_first_reboot("the_cure_survives_a_reboot_and_the_next_boot_reaches_the_app")
    else {
        return;
    };
    let before = console(&machine).len();

    poke_the_cure(&mut machine);
    // A chip reset, the kind a flasher asks for when it is done writing.
    // Immediately, with no guest time in between, so that everything after
    // this point in the console belongs to the boot that follows the cure.
    assert!(
        machine.reboot(Strap::App, ResetCause::UsbUartHpSys),
        "the machine can reboot (reboot_on_reset kept a power-on snapshot)"
    );
    assert_eq!(machine.power_cycles(), 0, "no power was cut, ever");

    let outcome = machine.run_until(&StopCondition::after_micros(APP_US).exit_on(HEARTBEAT));
    let text = console(&machine);
    let after = &text[before..];
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "the cured board never heartbeat: {outcome:?}\n{after}"
    );

    // 1. The bootloader introduced itself — the line AC1 said must not appear
    //    and AC3 says must.
    let banner = after
        .lines()
        .find(|l| l.contains(BANNER))
        .unwrap_or_else(|| panic!("no `{BANNER}` after the cure:\n{after}"));
    // 2. …on a boot whose own banner is the reset we asked for, not another
    //    watchdog bite.
    assert!(
        after.contains("rst:0x15 (USB_UART_HPSYS)"),
        "the boot after the cure is the chip reset's:\n{after}"
    );
    assert!(
        !after.contains(TG0_BANNER),
        "the watchdog bit again after the cure:\n{after}"
    );
    // 3. …and the app ran. `[INIT]` is it starting; the heartbeat is it
    //    running.
    let first = after
        .lines()
        .find(|l| l.contains(APP_FIRST))
        .unwrap_or_else(|| panic!("the app never started:\n{after}"));
    let beat = after
        .lines()
        .find(|l| l.contains(HEARTBEAT))
        .expect("the run stopped on it");
    // 4. The cure is still in the LP domain, which is the whole point.
    let clk = machine
        .peek_word(base::RNG + CLK_EN)
        .expect("LP_PERI answers the bus");
    assert_ne!(
        clk & LP_ANA_I2C_BIT,
        0,
        "the reboot threw the cure away: clk_en is {clk:#010x}, and the LP \
         domain was restored rather than kept"
    );
    assert_eq!(
        machine.reboots(),
        2,
        "one watchdog cycle, then the cure and one chip reset"
    );

    println!("AC3, the boot after the cure:");
    println!("  {}", banner.trim());
    println!("  {}", first.trim());
    println!("  {}", beat.trim());
    println!(
        "  LPPERI_CLK_EN {clk:#010x} — bit 29 set, kept across the reboot \
         (power-on value was {INDUCED_CLK_EN:#010x})"
    );
}

/// **AC4, and the second half of the domain model.** A power cycle takes the
/// LP domain with it — so it undoes the cure, and hands the wedged board
/// straight back.
///
/// This is the test that makes AC3's claim mean something. If `reboot()` and
/// `power_cycle()` did the same thing, AC3 could not tell whether the cure
/// survived or the snapshot happened to hold it; here the *same* cure, on the
/// *same* board, is thrown away by the *other* restart, and the board re-hangs
/// exactly as it did before anyone touched it.
///
/// **A note on AC4 as the plan spelled it.** The plan's table reads
/// "`power_cycle()` from the loop → next boot is `rst:0x1 (POWERON)` and
/// clean". The banner half holds and is asserted below. The word *clean* does
/// not, and cannot: `--lpperi-clk-en` seeds the induced gate word into the
/// power-on snapshot, so a power cycle restores an **induced** board. That is
/// not the emulator being unfaithful — it is what the bench saw. Unplugging a
/// wedged XIAO and plugging it back in did nothing, because the gate is written
/// by the firmware in its flash on every boot; the board on the desk needed
/// the *cure*, and a power cycle only helped when it interrupted the firmware
/// before it gated the clock again. The emulator models the induced state as a
/// power-on condition, so a power cycle reproduces it. `power_cycle()` on a
/// *clean* board is a clean boot, which is the `--lpperi-clk-en`-less default
/// and what `machine.rs`'s unit tests cover.
#[test]
#[ignore = "needs the reference merged image; `just test-emu-c6`"]
fn a_power_cycle_hands_the_induced_board_back_induced() {
    let Some(mut machine) = at_the_first_reboot("a_power_cycle_hands_the_induced_board_back_induced")
    else {
        return;
    };
    let before = console(&machine).len();

    // The same cure as AC3's, written into the same LP registers.
    poke_the_cure(&mut machine);
    assert!(machine.power_cycle(Strap::App), "the supply comes back");
    assert_eq!(machine.power_cycles(), 1);
    assert_eq!(machine.reboots(), 2, "a power cycle is a restart too");
    assert_eq!(
        machine.reset_cause(),
        ResetCause::PowerOn,
        "the ROM will read a power-on out of LP_CLKRST"
    );

    // The cure is gone: the LP domain went with the supply, and what came
    // back is the power-on value — which for this board is the induced one.
    let clk = machine
        .peek_word(base::RNG + CLK_EN)
        .expect("LP_PERI answers the bus");
    assert_eq!(
        clk, INDUCED_CLK_EN,
        "a power cycle restores the POWER-ON gate word, cure and all"
    );

    let outcome = machine.run_until(&StopCondition::after_micros(ONE_CYCLE_US).exit_on(BANNER));
    let text = console(&machine);
    let after = &text[before..];
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the board should have wedged again, not {outcome:?}\n{after}"
    );
    assert!(
        !after.contains(BANNER),
        "the board booted after a power cycle, so the wedge was NOT in the \
         power-on state:\n{after}"
    );

    // The banner the ROM printed for the power cycle, and the `Saved PC` it
    // did NOT print: the crash recorder reads zero on a cold chip
    // (`beqz a1` at 0x40018ad0), which is why `ResetCause::PowerOn` does not
    // record one.
    let poweron = after
        .lines()
        .find(|l| l.contains(POWERON_BANNER))
        .unwrap_or_else(|| panic!("no `{POWERON_BANNER}` after the power cycle:\n{after}"));
    let lines: Vec<&str> = after.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains(POWERON_BANNER))
        .expect("found above");
    assert!(
        !lines[at + 1].contains("Saved PC"),
        "a power-on boot printed a `Saved PC`: {}",
        lines[at + 1]
    );

    // …and then the watchdog shot it again, which is the loop closing for the
    // second reason: not because a reset preserved the wedge, but because the
    // wedge is what this board powers on into.
    let bites = after.matches(TG0_BANNER).count();
    assert_eq!(
        bites, 1,
        "one more watchdog cycle after the power cycle:\n{after}"
    );
    assert_eq!(machine.reboots(), 3);
    assert_eq!(machine.power_cycles(), 1, "one of the three was the supply");

    println!("AC4, the boot after the power cycle:");
    println!("  {}", poweron.trim());
    println!("  {}", lines[at + 1].trim());
    println!("  LPPERI_CLK_EN {clk:#010x} — the cure undone, bit 29 clear again");
    for line in after.lines().filter(|l| l.contains(TG0_BANNER)) {
        println!("  {}", line.trim());
    }
}
