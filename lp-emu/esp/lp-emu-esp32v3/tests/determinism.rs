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
    AppSource, BootMode, CORE_QUANTUM_DEFAULT, Esp32V3Builder, Machine, Outcome, StopCondition,
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
        mmu.iter()
            .any(|e| *e != lp_emu_esp32v3::cache::MMU_UNMAPPED),
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

// ---------------------------------------------------------------------------
// M4 P1: two cores, two quanta, and the single-core run that must not move
// ---------------------------------------------------------------------------

/// The 543-byte `[INIT]` prefix run's three counters on `origin/main` at
/// `75486b114` (M3 P8), measured with the M3 single-hart loop **before** the
/// two-core loop replaced it: the run to `[INIT] I/O task spawned` ends
/// before the firmware starts core 1, so it is the single-core identity the
/// phase file asks for — byte-identical output, the same cycle count, the
/// same instruction count and the same idle-skip count.
const PREFIX_CYCLES: u64 = 3_245_171;
const PREFIX_INSTRUCTIONS: u64 = 3_245_157;
const PREFIX_IDLE_SKIPS: u64 = 0;
const PREFIX_BYTES: usize = 543;
const PREFIX_SHA256: &str = "ea8bae305953ef613f68a97fb84919378f33b37eb5623dcb970e8dce2b7343e7";

/// **The single-core safety net.** A run in which core 1 never starts is
/// the run M3 produced: same bytes, same sha, same cycles, same
/// instructions, same skips. The quantum is the loop's window bound now and
/// the M3 loop had none below 8,192, so an equal cycle count here is the
/// claim that windows change nothing a single hart can observe.
#[test]
#[ignore = "needs the shipped image; `just test-emu-esp32v3-boot`"]
fn the_single_core_prefix_is_unchanged() {
    let Some(elf) = elf() else { return };
    let mut m = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .strict(true)
        .build()
        .expect("builds");
    let outcome = m.run_until(&StopCondition {
        exit_on: Some("[INIT] I/O task spawned".to_string()),
        ..StopCondition::after_micros(200_000)
    });
    assert!(matches!(outcome, Outcome::ExitMatched { .. }), "{outcome:?}");
    assert!(m.core_stalled(1), "core 1 was never started in this run");
    let bytes = m.uart0().bytes();
    assert_eq!(bytes.len(), PREFIX_BYTES);
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), PREFIX_SHA256);
    assert_eq!(
        (m.cycles(), m.instructions(), m.idle_skips()),
        (PREFIX_CYCLES, PREFIX_INSTRUCTIONS, PREFIX_IDLE_SKIPS),
        "the three counters origin/main's single-hart loop produced"
    );
    assert_eq!(m.core_instructions(1), 0);
}

/// What a dual-core run is, for the purpose of "the same run": the bytes,
/// both harts' counters, both pcs, the skips, the parks, and a fingerprint
/// of every RAM region.
#[derive(Debug, PartialEq, Eq)]
struct DualRun {
    sha256: String,
    bytes: usize,
    cycles: u64,
    instructions: (u64, u64),
    pcs: (u32, u32),
    idle_skips: u64,
    wfi_ends: (u64, u64),
    memory: Vec<u64>,
    outcome: String,
}

impl DualRun {
    fn of(m: &Machine, outcome: &Outcome) -> Self {
        let bytes = m.uart0().bytes();
        Self {
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes: bytes.len(),
            cycles: m.cycles(),
            instructions: (m.core_instructions(0), m.core_instructions(1)),
            pcs: (m.harts[0].pc(), m.harts[1].pc()),
            idle_skips: m.idle_skips(),
            wfi_ends: (m.wfi_ends(0), m.wfi_ends(1)),
            memory: m.snapshot().regions.iter().map(|r| fnv1a(r)).collect(),
            outcome: format!("{outcome:?}"),
        }
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The P8 gate script, so the run reaches the idle heartbeat and its reply.
fn stop_all_script() -> lp_emu_esp_common::ScriptedSource {
    lp_emu_esp_common::ScriptedSource::new().after(
        "[INIT] I/O task spawned",
        lp_emu_esp32v3::memmap::CYCLES_PER_US * 1_000,
        b"M!{\"id\":1,\"msg\":\"stopAllProjects\"}\n",
    )
}

/// The shipped image, direct-loaded onto the merged chip, strict, with core
/// 1 released by the firmware, at `quantum` cycles per window, to the
/// heartbeat reply or the deadline.
///
/// ⚠️ On the shipped image at the time of writing this run ends on the
/// strict stop in `LpFs::read_file` that
/// `docs/defects/2026-09-10-the-app-cores-rom-boot-rewrites-heap-region-0.md`
/// describes, ~30k cycles after the release. The comparisons below are
/// made on whatever the run was — two runs of a crash are still one run —
/// and say so in `outcome`.
fn dual(quantum: u64, elf: &std::path::Path, chip: &std::path::Path) -> (Machine, Outcome) {
    let len = std::fs::metadata(chip).expect("the merged image").len() as u32;
    let mut m = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(FlashBacking::Copy(chip.to_path_buf()))
        .flash_len(len)
        .strict(true)
        .core_quantum(quantum)
        .uart0_script(stop_all_script())
        .build()
        .expect("builds");
    let outcome = m.run_until(&StopCondition {
        exit_on: Some("\"id\":1,\"msg\":\"stopAllProjects\"".to_string()),
        ..StopCondition::after_micros(DIRECT_US)
    });
    (m, outcome)
}

/// Two runs at the default quantum are one run: the UART sha, the byte
/// count, the clock, both instruction counts, both pcs, the skips, the
/// parks and the memory fingerprint.
#[test]
#[ignore = "needs the shipped image and espflash; `just test-emu-esp32v3-boot`"]
fn two_runs_identical_dual_core() {
    let (Some(elf), Some(chip)) = (elf(), merged()) else {
        return;
    };
    let (a, oa) = dual(CORE_QUANTUM_DEFAULT, &elf, &chip);
    let (b, ob) = dual(CORE_QUANTUM_DEFAULT, &elf, &chip);
    let (a, b) = (DualRun::of(&a, &oa), DualRun::of(&b, &ob));
    println!("quantum {CORE_QUANTUM_DEFAULT}: {a:?}");
    assert_eq!(a, b, "two runs at quantum {CORE_QUANTUM_DEFAULT} are one run");
}

/// The same at `--core-quantum 64`.
#[test]
#[ignore = "needs the shipped image and espflash; `just test-emu-esp32v3-boot`"]
fn two_runs_identical_dual_core_quantum_64() {
    let (Some(elf), Some(chip)) = (elf(), merged()) else {
        return;
    };
    let (a, oa) = dual(64, &elf, &chip);
    let (b, ob) = dual(64, &elf, &chip);
    let (a, b) = (DualRun::of(&a, &oa), DualRun::of(&b, &ob));
    println!("quantum 64: {a:?}");
    assert_eq!(a, b, "two runs at quantum 64 are one run");
}

/// **The honest form of D3.** Two quanta are two interleavings, so their
/// cycle counts may legitimately differ — the counters are **not** asserted
/// equal here, and the test prints both. What may not differ is anything the
/// guest can observe about itself: the console.
///
/// **One line is excepted, and named:** `[stack] heartbeat: high-water N B`
/// is the main stack's deepest point, which is a measurement the firmware
/// makes of *where an interrupt landed in its own call tree* — an
/// interrupt-timing observable. The quantum moves interrupt arrival by up
/// to one window, so the figure may move with it; on the diagnostic
/// firmware M4 P1 ran to the heartbeat, quantum 256 read 16220 B and
/// quantum 64 read 16348 B, and every other byte of the 2,799-byte console
/// was equal. Silicon's own reading of the same figure differs from this
/// machine's by 960 B for the same reason (`tests/boot_idle.rs`,
/// `STACK_HIGH_WATER_GAP`). A quantum that changed **any other** byte would
/// be a race the model is hiding, and this test would fail on it rather
/// than widen.
#[test]
#[ignore = "needs the shipped image and espflash; `just test-emu-esp32v3-boot`"]
fn the_two_quanta_agree_on_what_the_guest_sees() {
    let (Some(elf), Some(chip)) = (elf(), merged()) else {
        return;
    };
    let (a, oa) = dual(CORE_QUANTUM_DEFAULT, &elf, &chip);
    let (b, ob) = dual(64, &elf, &chip);
    println!(
        "quantum {CORE_QUANTUM_DEFAULT}: cycles={} instructions={:?} idle={} {oa:?}",
        a.cycles(),
        (a.core_instructions(0), a.core_instructions(1)),
        a.idle_skips()
    );
    println!(
        "quantum 64: cycles={} instructions={:?} idle={} {ob:?}",
        b.cycles(),
        (b.core_instructions(0), b.core_instructions(1)),
        b.idle_skips()
    );
    let mask = |text: String| -> String {
        text.lines()
            .map(|l| match l.find("[stack] heartbeat: high-water ") {
                Some(at) => format!(
                    "{}[stack] heartbeat: high-water <interrupt-timing>",
                    &l[..at]
                ),
                None => l.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let (ta, tb) = (mask(a.uart0().text()), mask(b.uart0().text()));
    assert_eq!(
        ta, tb,
        "the two quanta printed different consoles (the stack high-water line excepted)"
    );
}
