//! **The plan's inviolable invariant, made mechanical.**
//!
//! A run is a pure function of the instruction stream and its scripted input.
//! The wall clock only ever *ends* a run (`--wall-timeout`), never changes
//! one, and no peripheral in this machine reads a host clock. So two runs of
//! one image agree on every byte and on both counters, and a run interrupted
//! and resumed from a snapshot agrees with one that was never interrupted.
//!
//! The three assertions, and what each is for:
//!
//! | test | what it would catch |
//! |---|---|
//! | [`two_runs_identical_direct`] | a peripheral that read `Instant::now`, a `HashMap` iteration order that reached a value, an uninitialised byte |
//! | [`two_runs_identical_rom_up`] | the same, through the real mask ROM and the real IDF bootloader — 71 million instructions rather than 11 |
//! | [`snapshot_restore_identity`] | a piece of machine state [`Snapshot`] does not carry. The two runs are the same run; the only difference is that one of them went through the struct |
//!
//! ⚠️ **The counters are compared as well as the bytes, and that is not
//! belt-and-braces.** Two runs can print identical consoles and take
//! different numbers of instructions to do it — an interrupt taken one slice
//! later, a spin that ran an extra turn — and a transcript comparison would
//! call that identical. `cycles` and `instructions` are what says it was the
//! same *run* and not merely the same output.
//!
//! `#[ignore]`d for the usual reason ([`lp_emu_esp32v3::test_support`]): a
//! plain `cargo test --workspace` must never start a cross-target firmware
//! build. `just test-emu-esp32v3-boot` builds the ELF, runs `espflash` on it
//! and names both files.

use lp_emu_esp32v3::flash::FlashBacking;
use lp_emu_esp32v3::machine::{
    AppSource, BootMode, Esp32V3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32v3::test_support::{fw_esp32v3_image, merged_chip_image, skip_notice};
use sha2::{Digest, Sha256};

/// Far enough in for the console, the filesystem mount and the io_task, and
/// short enough that four of these runs are seconds rather than minutes.
const DIRECT_US: u64 = 200_000;
/// The ROM-up path spends its first quarter-second in the mask ROM and the
/// bootloader's segment loads before the application's first instruction.
const ROM_UP_US: u64 = 700_000;

/// What a run is, for the purpose of "the same run": the bytes, and the two
/// counters that say it took the same path to them.
#[derive(Debug, PartialEq, Eq)]
struct Run {
    sha256: String,
    bytes: usize,
    cycles: u64,
    instructions: u64,
    pc: u32,
    idle_skips: u64,
}

impl Run {
    fn of(m: &Machine) -> Self {
        let bytes = m.uart0().bytes();
        Self {
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes: bytes.len(),
            cycles: m.cycles(),
            instructions: m.instructions(),
            pc: m.harts[0].pc(),
            idle_skips: m.idle_skips(),
        }
    }
}

fn merged() -> Option<std::path::PathBuf> {
    match merged_chip_image() {
        Ok(p) => Some(p),
        Err(reason) => {
            skip_notice("determinism", &reason);
            None
        }
    }
}

fn elf() -> Option<std::path::PathBuf> {
    match fw_esp32v3_image() {
        Ok(p) => Some(p),
        Err(reason) => {
            skip_notice("determinism", &reason);
            None
        }
    }
}

fn build(mode: BootMode, elf: Option<&std::path::Path>, chip: &std::path::Path) -> Machine {
    let len = std::fs::metadata(chip).expect("the merged image").len() as u32;
    let mut b = Esp32V3Builder::new()
        .boot_mode(mode)
        .flash(FlashBacking::Copy(chip.to_path_buf()))
        .flash_len(len)
        .strict(true);
    if let Some(elf) = elf {
        b = b.app(AppSource::Path(elf.to_path_buf()));
    }
    b.build().expect("builds")
}

/// Two runs of the direct load are one run.
#[test]
#[ignore = "needs the shipped image and espflash; `just test-emu-esp32v3-boot`"]
fn two_runs_identical_direct() {
    let (Some(elf), Some(chip)) = (elf(), merged()) else {
        return;
    };
    let mut a = build(BootMode::Direct, Some(&elf), &chip);
    a.run_until(&StopCondition::after_micros(DIRECT_US));
    let mut b = build(BootMode::Direct, Some(&elf), &chip);
    b.run_until(&StopCondition::after_micros(DIRECT_US));

    let (a, b) = (Run::of(&a), Run::of(&b));
    assert!(a.bytes > 500, "the boot printed a console: {a:?}");
    assert_eq!(a, b, "two runs of the direct load are one run");
}

/// And two runs from the reset vector, through the real ROM and the real
/// ESP-IDF second-stage bootloader.
#[test]
#[ignore = "needs espflash; `just test-emu-esp32v3-boot`"]
fn two_runs_identical_rom_up() {
    let Some(chip) = merged() else { return };
    let mut a = build(BootMode::RomUp, None, &chip);
    a.run_until(&StopCondition::after_micros(ROM_UP_US));
    let mut b = build(BootMode::RomUp, None, &chip);
    b.run_until(&StopCondition::after_micros(ROM_UP_US));

    let (a, b) = (Run::of(&a), Run::of(&b));
    assert!(
        a.bytes > 1_000,
        "the ROM banner and the bootloader log: {a:?}"
    );
    assert_eq!(a, b, "two runs of the ROM-up boot are one run");
}

/// **Snapshot in the middle, restore, carry on — and it is the same run.**
///
/// One machine runs to half the deadline and snapshots. It then carries on to
/// the full deadline, which is the reference. The same snapshot is restored
/// into a **fresh machine built the same way**, which carries on to the same
/// deadline — and the two second halves are compared: the bytes the guest
/// printed after the snapshot point, the cycle count, the instruction count,
/// the pc and the idle skips.
///
/// The restore is into a *new* machine rather than back into the same one on
/// purpose. Restoring into the machine the snapshot came from would pass even
/// if the struct carried nothing at all, because everything it failed to
/// carry would still be sitting there.
///
/// ⚠️ **The console log is host-side and does not ride in the snapshot**, by
/// design: `Snapshot` is machine state, and the sink the bytes went to
/// belongs to whoever built the machine. So the restored run's log holds the
/// second half only, and that is exactly what makes it the right thing to
/// compare against the reference's tail.
#[test]
#[ignore = "needs the shipped image and espflash; `just test-emu-esp32v3-boot`"]
fn snapshot_restore_identity() {
    let (Some(elf), Some(chip)) = (elf(), merged()) else {
        return;
    };

    let mut subject = build(BootMode::Direct, Some(&elf), &chip);
    let half = subject.run_until(&StopCondition::after_micros(DIRECT_US / 2));
    assert!(
        matches!(half, Outcome::Deadline { .. }),
        "the first half ends on its deadline and not on a fault: {half:?}"
    );
    let snap = subject.snapshot();
    assert!(snap.bytes() > 1_000_000, "the regions are in it");
    let taken_at = snap.cycle();
    let before = subject.uart0().bytes().len();

    // The reference: the same machine, straight on.
    subject.run_until(&StopCondition::after_micros(DIRECT_US));
    let reference = Run::of(&subject);
    let reference_tail = subject.uart0().bytes()[before..].to_vec();

    // The subject: a fresh machine, the snapshot, and the same second half.
    let mut resumed = build(BootMode::Direct, Some(&elf), &chip);
    resumed.restore(&snap);
    assert_eq!(resumed.cycles(), taken_at, "time came back with it");
    assert!(
        resumed.uart0().bytes().is_empty(),
        "the console log is host-side and does not ride in the snapshot"
    );
    resumed.run_until(&StopCondition::after_micros(DIRECT_US));
    let resumed_run = Run::of(&resumed);

    assert!(
        !reference_tail.is_empty(),
        "the second half printed something to compare"
    );
    assert_eq!(
        resumed.uart0().bytes(),
        reference_tail,
        "the second half's bytes, byte for byte"
    );
    assert_eq!(
        (
            resumed_run.cycles,
            resumed_run.instructions,
            resumed_run.pc,
            resumed_run.idle_skips
        ),
        (
            reference.cycles,
            reference.instructions,
            reference.pc,
            reference.idle_skips
        ),
        "a run that went through a snapshot is the run that did not"
    );
}

/// The pieces of machine state that are **not** hart state and not a region,
/// each asserted through the struct rather than assumed to ride in it.
///
/// The flash MMU and the cache-control words live in a `ClassicCache` behind
/// a handle the machine and three views share; the CPU stall key is composed
/// from two RTC_CNTL fields and one DPORT field; the interrupt matrix is the
/// bus's own. Any of them could have been left out of [`Snapshot`] without a
/// test noticing, because a restore into the same machine finds them already
/// right.
#[test]
#[ignore = "needs espflash; `just test-emu-esp32v3-boot`"]
fn the_snapshot_carries_the_state_that_is_not_a_register() {
    let Some(chip) = merged() else { return };
    let mut m = build(BootMode::RomUp, None, &chip);
    // Far enough for the bootloader to have programmed the flash MMU and
    // turned the cache on.
    m.run_until(&StopCondition::after_micros(ROM_UP_US));

    let mmu = m.flash_mmu_entries();
    assert!(
        mmu.iter().any(|e| *e != lp_emu_esp32v3::cache::MMU_UNMAPPED),
        "the bootloader mapped something"
    );
    let cache_on = m.cache().lock().expect("cache").enabled(0);
    let stall = (m.stall_key().key(0), m.stall_key().key(1));
    let matrix = m.bus().matrix().save_state();

    let snap = m.snapshot();
    let mut other = build(BootMode::RomUp, None, &chip);
    other.restore(&snap);

    assert_eq!(other.flash_mmu_entries(), mmu, "the flash MMU tables");
    assert_eq!(
        other.cache().lock().expect("cache").enabled(0),
        cache_on,
        "the cache-enable bit"
    );
    assert_eq!(
        (other.stall_key().key(0), other.stall_key().key(1)),
        stall,
        "the CPU stall key, both halves"
    );
    assert_eq!(other.bus().matrix().save_state(), matrix, "the matrix");
    assert!(other.core_stalled(1), "core 1 is still held");
    // And the DBREAK slots: M1 P6 disarms the watchpoints on a restore and
    // re-arms them from the restored hart's own `DBREAK` registers, so the
    // count is the hart's and not a second copy that could disagree.
    assert_eq!(
        other.harts[0].breakpoints().dbreaka,
        snap.harts[0].breakpoints().dbreaka,
        "DBREAKA"
    );
    assert_eq!(
        other.harts[0].breakpoints().dbreakc,
        snap.harts[0].breakpoints().dbreakc,
        "DBREAKC"
    );
}
