//! **A run is a pure function of its inputs**, and a snapshot is the run.
//!
//! Two claims, and they are different ones:
//!
//! 1. *Determinism.* Two runs of the same image with the same seed agree on
//!    every observable — the console's sha256 and byte count, the cycle
//!    count, the instruction count, where each hart ended up, and what the
//!    bus refused. Nothing in this machine reads the host clock except
//!    `--wall-timeout`, which can end a run and never change one.
//! 2. *Snapshot fidelity.* A snapshot taken mid-run and restored into a
//!    **fresh** machine reproduces the second half exactly. That is a
//!    stronger claim than "restore into the same machine": it says the
//!    snapshot carries everything, rather than the machine happening to still
//!    hold some of it.
//!
//! ⚠️ **The console is empty in M6 P03, and the comparison is here anyway.**
//! The S3's console is USB-Serial-JTAG and nothing drives it until P05, so
//! the sha compared below is the sha of zero bytes. The test is written
//! against `machine.console()` rather than against a constant precisely so
//! that P05 changes nothing here: the day a byte appears, this comparison
//! starts meaning what it says without a line being edited.

use sha2::{Digest, Sha256};

use lp_emu_esp32s3::machine::{
    AppSource, BootFrame, Esp32S3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32s3::{memmap, test_support};
use lp_xt_inst::{AluRrr, BrZ, CallOp, Inst, Reg, encode};

/// Inside the SRAM1 I-bus view, for the reason `tests/boot.rs` argues at
/// length: a windowed call cannot cross a 1 GiB region, so code that calls
/// the mask ROM must share its top two address bits.
const CODE: u32 = 0x4037_9000;

fn a(n: u8) -> Reg {
    Reg::new(n)
}

fn assemble(program: &[Inst]) -> Vec<u8> {
    program.iter().flat_map(encode).collect()
}

fn call_offset(call_pc: u32, target: u32) -> i32 {
    (target as i32 - ((call_pc & !3) as i32 + 4)) >> 2
}

/// A run with no firmware image: twenty frames of windowed recursion, spilled
/// and reloaded through the mask ROM's own window vectors.
///
/// Enough moving parts that a machine which was *not* deterministic would
/// show it — a register ring that wraps, exception vectors in the ROM, and
/// several hundred instructions — and it needs no toolchain, so it runs in
/// every `cargo test`.
fn recursive_machine(quantum: u64) -> Machine {
    const DEPTH: u32 = 20;
    let mut machine = Esp32S3Builder::new()
        .strict(true)
        .core_quantum(quantum)
        .build()
        .expect("a machine");
    let f_at = CODE + 0x40;
    let f = [
        Inst::Entry(a(1), 32),
        Inst::BranchZ(BrZ::Beqz, a(2), 11),
        Inst::Addi(a(10), a(2), -1),
        Inst::Call(CallOp::Call8, -3),
        Inst::Rrr(AluRrr::Add, a(2), a(2), a(10)),
        Inst::Nullary(lp_xt_inst::NullaryOp::Retw),
        Inst::Movi(a(2), 0),
        Inst::Nullary(lp_xt_inst::NullaryOp::Retw),
    ];
    machine
        .bus_mut()
        .load_image(f_at, &assemble(&f))
        .expect("placing f");
    machine
        .bus_mut()
        .load_image(
            CODE,
            &assemble(&[
                Inst::Call(CallOp::Call8, call_offset(CODE, f_at)),
                Inst::Break(1, 15),
            ]),
        )
        .expect("placing the caller");
    machine
        .seed_boot_state(CODE, BootFrame::rom_pro_stack(machine.rom()))
        .expect("seeding");
    machine
        .break_at_address(CODE + 3)
        .expect("claiming the caller's break");
    machine.harts[0].cpu_mut().set_a(10, DEPTH);
    machine
}

/// Everything about a run that must not vary. Compared as a whole so a
/// failure names the field rather than the first assertion that happened to
/// be written.
#[derive(Debug, PartialEq, Eq)]
struct Observed {
    console_sha: String,
    console_len: usize,
    cycles: u64,
    instructions: u64,
    per_core: [u64; 2],
    pc: [u32; 2],
    a10: u32,
    idle_skips: u64,
    hook_calls: u64,
    first_violation: Option<String>,
    outcome: String,
}

fn observe(machine: &Machine, outcome: &Outcome) -> Observed {
    let bytes = machine.console().bytes();
    Observed {
        console_sha: format!("{:x}", Sha256::digest(&bytes)),
        console_len: bytes.len(),
        cycles: machine.cycles(),
        instructions: machine.instructions(),
        per_core: [machine.core_instructions(0), machine.core_instructions(1)],
        pc: [machine.harts[0].pc(), machine.harts[1].pc()],
        a10: machine.harts[0].cpu().a(10),
        idle_skips: machine.idle_skips(),
        hook_calls: machine.hook_calls(),
        first_violation: machine.first_strict_violation().map(|v| format!("{v:?}")),
        outcome: format!("{outcome:?}"),
    }
}

fn run_to(machine: &mut Machine, cycle: u64) -> Outcome {
    machine.run_until(&StopCondition {
        stop_cycle: Some(cycle),
        ..Default::default()
    })
}

/// Run to `cycle` and read every observable off the machine afterwards.
fn run_and_observe(machine: &mut Machine, cycle: u64) -> Observed {
    let outcome = run_to(machine, cycle);
    observe(machine, &outcome)
}

/// Two runs, the same everything.
#[test]
fn two_identical_runs_are_the_same_run() {
    let mut first = recursive_machine(256);
    let a = run_and_observe(&mut first, 1_000_000);
    let mut second = recursive_machine(256);
    let b = run_and_observe(&mut second, 1_000_000);
    assert_eq!(a, b);
    assert!(a.instructions > 200, "and it was a real run: {a:?}");
    println!(
        "DETERMINISM: console sha {} ({} B), {} cycles, {} instructions, a10={}",
        a.console_sha, a.console_len, a.cycles, a.instructions, a.a10
    );
}

/// **Two quanta are two interleavings, and what the guest can observe about
/// itself must not move.**
///
/// With slot 1 held the quantum cannot change anything at all here, and that
/// is worth pinning rather than assuming: a loop that gave a held core a
/// window, or that let the window bound leak into the clock, would show up as
/// a cycle count that tracked `--core-quantum`.
#[test]
fn the_quantum_does_not_change_what_the_guest_sees() {
    let mut small = recursive_machine(1);
    let a = run_and_observe(&mut small, 1_000_000);
    let mut large = recursive_machine(8_192);
    let b = run_and_observe(&mut large, 1_000_000);
    assert_eq!(a.console_sha, b.console_sha);
    assert_eq!(a.instructions, b.instructions);
    assert_eq!(a.cycles, b.cycles);
    assert_eq!(a.a10, b.a10);
    assert_eq!(a.first_violation, b.first_violation);
}

/// A snapshot taken mid-run, restored into a **fresh** machine, reproduces
/// the second half.
///
/// The fresh machine is built the same way and then has the snapshot put into
/// it, so anything the snapshot forgot would show as a difference. The
/// snapshot's own bookkeeping is checked too: its `cycle()` is the machine's
/// clock at the moment it was taken, and its `core_1_control` is the register
/// that holds slot 1.
#[test]
fn a_snapshot_restored_into_a_fresh_machine_reproduces_the_second_half() {
    // Long enough that the ring has wrapped and the ROM's window handlers
    // have run before the cut.
    const CUT: u64 = 200;
    const END: u64 = 1_000_000;

    let mut original = recursive_machine(256);
    run_to(&mut original, CUT);
    let snap = original.snapshot();
    assert_eq!(snap.cycle(), original.cycles(), "the snapshot's own clock");
    assert!(
        snap.core_1_control.holds_core1(),
        "and the register that holds slot 1 rode with it"
    );
    assert_eq!(snap.core_quantum, original.core_quantum());
    assert!(snap.bytes() > 0);

    let finished = run_and_observe(&mut original, END);

    let mut fresh = recursive_machine(256);
    fresh.restore(&snap);
    assert_eq!(
        fresh.cycles(),
        snap.cycle(),
        "the restore put the clock back where the snapshot was taken"
    );
    let replayed = run_and_observe(&mut fresh, END);

    assert_eq!(
        finished, replayed,
        "the restored machine ran the same second half"
    );
    println!(
        "SNAPSHOT: cut at cycle {}, {} B of regions; replay matched on {} cycles / {} \
         instructions",
        snap.cycle(),
        snap.bytes(),
        replayed.cycles,
        replayed.instructions
    );
}

/// A restore adopts the snapshot's quantum and says so, because a run's
/// future depends on it.
#[test]
fn a_restore_adopts_the_snapshots_quantum() {
    let mut original = recursive_machine(64);
    run_to(&mut original, 100);
    let snap = original.snapshot();
    assert_eq!(snap.core_quantum, 64);

    let mut other = recursive_machine(1_024);
    assert_eq!(other.core_quantum(), 1_024);
    other.restore(&snap);
    assert_eq!(other.core_quantum(), 64);
}

// ---------------------------------------------------------------------------
// The shipped image. `#[ignore]`d — `just test-emu-esp32s3-boot` builds it.
// ---------------------------------------------------------------------------

fn shipped(strict: bool) -> Option<Machine> {
    let elf = test_support::fw_esp32s3_image().ok()?;
    Some(
        Esp32S3Builder::new()
            .app(AppSource::Path(elf))
            .strict(strict)
            .build()
            .expect("the shipped image direct-loads"),
    )
}

/// Two runs of the shipped image are the same run.
///
/// ⚠️ **The assertion about the stop inverted in M6 P05**: P04's version
/// asserted `first_violation.is_some()`, because the console was the one
/// block nothing modelled and its refusal was that phase's deliverable. P05
/// models the console, so a strict run of this image now refuses nothing —
/// it runs to the deadline, spinning on `SPI1.cmd.usr`, which is **P06's**
/// stop and not a bus refusal at all (`tests/boot_idle.rs` pins it).
///
/// The console sha is the load-bearing half either way, and it is now a sha
/// of something: P04's console was empty, and P05 gives it 253 bytes.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn two_runs_of_the_shipped_image_agree_on_everything() {
    let (Some(mut first), Some(mut second)) = (shipped(true), shipped(true)) else {
        test_support::skip_notice(
            "two_runs_of_the_shipped_image_agree_on_everything",
            "no image",
        );
        return;
    };
    let deadline = 300_000 * memmap::CYCLES_PER_US;
    let a = run_and_observe(&mut first, deadline);
    let b = run_and_observe(&mut second, deadline);
    assert_eq!(a, b);
    assert!(
        a.first_violation.is_none(),
        "P04's stop was the console and P05 models it: nothing is refused any more, and a \
         refusal here is a block a later phase has to name — got {:?}",
        a.first_violation
    );
    println!(
        "SHIPPED DETERMINISM: console sha {} ({} B), {} cycles, {} instructions, stop {}",
        a.console_sha,
        a.console_len,
        a.cycles,
        a.instructions,
        a.first_violation.as_deref().unwrap_or("none")
    );
}

/// A snapshot of the shipped image's own run, restored into a fresh machine,
/// reproduces the second half — including the strict stop.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn a_snapshot_of_the_shipped_image_replays() {
    let (Some(mut original), Some(mut fresh)) = (shipped(false), shipped(false)) else {
        test_support::skip_notice("a_snapshot_of_the_shipped_image_replays", "no image");
        return;
    };
    // Before the ROM's cache path spins: far enough in that real work has
    // happened, early enough that the cut is not inside a poll.
    const CUT: u64 = 30;
    let deadline = 1_000 * memmap::CYCLES_PER_US;

    run_to(&mut original, CUT);
    let snap = original.snapshot();
    let finished = run_and_observe(&mut original, deadline);

    fresh.restore(&snap);
    let replayed = run_and_observe(&mut fresh, deadline);
    assert_eq!(finished, replayed);
    println!(
        "SHIPPED SNAPSHOT: cut at cycle {}, {} B; replay matched on {} cycles / {} instructions",
        snap.cycle(),
        snap.bytes(),
        replayed.cycles,
        replayed.instructions
    );
}
