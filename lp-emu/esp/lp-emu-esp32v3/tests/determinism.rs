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
    // Since M4 P1 core 1 is released by DPORT during the boot, so its hold
    // is whatever the run left it — and the snapshot carries that, not a
    // constant.
    assert_eq!(
        other.core_stalled(1),
        m.core_stalled(1),
        "core 1's hold comes back as the run left it"
    );
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
///
/// ⚠️ **M4 P3b moved the instruction count by six: 3,245,157 → 3,245,151.**
/// Not the loop — the hart. Poll point (c) had been zeroing the hart's
/// asserted-line mask on every MMIO store, so a line raised by a store was
/// taken at the next slice boundary instead of at the next instruction
/// (README, "The link, after the boot settles"). With interrupts taken where
/// silicon takes them the same prefix retires six fewer instructions on the
/// way to the same 543 bytes at the same cycle: the bytes, the sha, the
/// cycle count and the skip count did not move. Re-pinned with that cause
/// attached; a further change here is a finding again.
///
/// ⚠️ **The BLE plan's M3 (access core) moved both by +5,856: 3,245,171 →
/// 3,251,027 cycles, 3,245,151 → 3,251,007 instructions.** Not the machine —
/// the image. Found with `LP_EMU_XT_BLOCKPROF` on `origin/main` (`e226fb283`)
/// against the branch, both images run to this same line: the whole
/// difference is `boot_firmware` +5,969, `_xtensa_lx_rt_zero_fill` −60, the
/// main task's `poll` −49 and the mask ROM −4. Inside `boot_firmware` it is
/// one 7-instruction loop running 8,424 times instead of 7,572 — the
/// `stack_probe::paint` loop, which paints from the stack bottom to 1 KiB
/// below the current `sp`. It paints 852 words more because both ends
/// moved: the bottom 20 words lower (`.bss` shrank 80 B, which is also the
/// −60 in `zero_fill`), and the `sp` at the paint 832 words (3,328 B)
/// higher, because the embassy main task's `poll` frame shrank from `entry
/// a1, 4304` to `entry a1, 976` — the branch's server-loop changes took
/// temporaries off that frame (read off both ELFs' `entry` instructions,
/// not inferred; which of the branch's commits did it was not bisected).
/// 852 × 7 = 5,964, and the other five are straight-line code in
/// `boot_firmware`. The bytes changed by one line for the `.bss` reason
/// (see `PREFIX_SHA256`); the skip count did not move.
///
/// Then **+3 more when M3 merged over lean-wire (PR #791): 3,251,027 →
/// 3,251,030 cycles, 3,251,007 → 3,251,010 instructions.** Again the image,
/// and found the same way (`LP_EMU_XT_BLOCKPROF`, the M3 branch's image at
/// `458d3769e` against the merged tree's, per symbol): `boot_firmware` +2 and
/// the mask ROM +1, nothing else. `boot_firmware` came out 8 B longer and
/// its code around the `stack_probe::paint` loop is scheduled differently —
/// the loop still runs 8,424 times, and two more straight-line instructions
/// retire once each. The ROM's +1 is `uart_tx_one_char`'s TX-FIFO wait
/// (0x4000921a–0x40009222, a four-instruction poll entered 672 times): one
/// call's wait retires one more of the loop's instructions before it exits,
/// which is what the two-cycle shift upstream of it buys. Bytes, sha and
/// skips did not move.
///
/// Then **−21 with lean-wire's follow-ups (#804): 3,251,030 → 3,251,009
/// cycles, 3,251,010 → 3,250,989 instructions**, in the same change that
/// left the main stack 16 B smaller (`45360 B` → `45344 B`, the one line of
/// the 543 bytes that moved; see `PREFIX_SHA256`). A smaller stack is fewer
/// `stack_probe::paint` iterations, which is the likely home of the drop,
/// but it was NOT isolated per symbol with `LP_EMU_XT_BLOCKPROF`, unlike
/// the entries above. Skips did not move.
///
/// Then **−93 each on the merge of `origin/main` (lean-wire's follow-ups #804, 45,360 -> 45,344, and the IN-endpoint gate #805) into `lp-json-pack` (the link epoch and the transport's per-link encoding: +24 B of statics on its own): 3,251,009 → 3,250,916 cycles,
/// 3,250,989 → 3,250,896 instructions.** Measured on the merged tree with the
/// same lp-emu: the merged ELF's main stack is 45,304 B (#804 alone 45,344),
/// so the `stack_probe::paint` loop starts 40 B (10 words) higher — 70 of the
/// 93 at its 7 instructions a word. The other 23 were not attributed per
/// symbol; the skip count did not move, and the bytes moved by the one
/// `main stack` line (see `PREFIX_SHA256`).
const PREFIX_CYCLES: u64 = 3_250_916;
const PREFIX_INSTRUCTIONS: u64 = 3_250_896;
const PREFIX_IDLE_SKIPS: u64 = 0;
const PREFIX_BYTES: usize = 543;
/// Moved by the BLE plan's M3 (access core): one line of the 543 bytes,
/// `[INIT] main stack 45280 B` → `45360 B` (the server loop's future in
/// `.bss` shrank 80 B). See `boot_idle.rs`'s `PREFIX_SHA256`. Was `ea8bae30…`.
/// Moved again by lean-wire's follow-ups (#804): `45360 B` → `45344 B`.
/// Was `465c8d52…`.
/// Then `45344 B` → `45304 B` on the merge with `lp-json-pack` (its statics;
/// the merged ELF's `_stack_start - _stack_end`). #804's pin was `05b27095…`.
const PREFIX_SHA256: &str = "365c9e6d6b24f128edd9445d9bdc0227ad3bc46c2e679a002030713a7fd5b7d4";

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
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{outcome:?}"
    );
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
/// The comparisons below are made on whatever the run was — two runs of a
/// crash are still one run — and say so in `outcome`. That was not idle
/// wording: this test was written while the shipped image ended on a strict
/// stop in `LpFs::read_file` ~30k cycles after the release
/// (`docs/defects/2026-09-10-the-emulator-ran-the-rom-reset-path-on-the-app-core.md`,
/// the release modelled as a reset through the mask ROM), and it held the
/// determinism claim across the crash without widening anything. It now
/// reaches the heartbeat reply.
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
    assert_eq!(
        a, b,
        "two runs at quantum {CORE_QUANTUM_DEFAULT} are one run"
    );
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
