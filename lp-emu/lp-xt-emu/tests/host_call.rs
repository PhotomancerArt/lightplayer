//! The host-engine substrate: a shared memory window the host and guest both
//! see, and a call path that can pass a full argument list and return two words.
//!
//! This is what a host emulation engine needs in order to invoke compiled
//! shader code: the shader's vmctx / uniforms / globals live in host memory, and
//! its entry points take more arguments than the six windowed argument
//! registers hold.
//!
//! Payload bytes are built with `lp_xt_inst::encode` — the objdiff-verified
//! encoder (10,969 instructions, 0 mismatches against objdump), not hand
//! assembly. The other suites in this directory use golden vectors from
//! FINDINGS.md; those are fixed programs, while these need specific register and
//! stack-offset shapes, so they are assembled from the encoder instead of
//! transcribed by hand.

use std::sync::{Arc, Mutex};

use lp_xt_emu::board::BoardProfile;
use lp_xt_emu::emu::{CallOutcome, OUT_ARG_REG_COUNT};
use lp_xt_emu::memory::{EXC_INSTR_FETCH_ERROR, SHARED_DBUS_BASE};
use lp_xt_emu::{Emulator, NoopTracer, TrapKind};
use lp_xt_inst::{Inst, LoadOp, NullaryOp, Reg, StoreOp, encode};

/// Callee frame size for the test programs. Any multiple of 8 works; the
/// incoming stack-argument offsets below are relative to it.
const FRAME: u32 = 32;

fn a(n: u8) -> Reg {
    Reg::new(n)
}

fn asm(insts: &[Inst]) -> Vec<u8> {
    insts.iter().flat_map(encode).collect()
}

/// A shared arena of `len` zero bytes, plus the emulator it is attached to at
/// [`SHARED_DBUS_BASE`], with `code` loaded at the profile's I-bus code base.
/// Returns `(emu, arena, entry)`.
fn emu_with_shared(
    profile: BoardProfile,
    len: usize,
    code: &[u8],
) -> (Emulator, Arc<Mutex<Vec<u8>>>, u32) {
    let arena = Arc::new(Mutex::new(vec![0u8; len]));
    let mut emu = Emulator::with_profile(profile);
    emu.mem.add_shared(SHARED_DBUS_BASE, Arc::clone(&arena));
    let entry = emu.profile.code_ibus_base();
    emu.mem.load_bytes(entry, code);
    (emu, arena, entry)
}

fn word(arena: &Arc<Mutex<Vec<u8>>>, off: usize) -> u32 {
    let g = arena.lock().unwrap();
    u32::from_le_bytes(g[off..off + 4].try_into().unwrap())
}

fn set_word(arena: &Arc<Mutex<Vec<u8>>>, off: usize, v: u32) {
    let mut g = arena.lock().unwrap();
    g[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn expect_ok(out: CallOutcome) -> (u32, u32) {
    match out {
        CallOutcome::Ok { lo, hi } => (lo, hi),
        CallOutcome::Trap(t) => panic!("unexpected trap: {t:?}"),
    }
}

/// The guest loads from and stores into the host's buffer, and the host sees it
/// — the property the whole vmctx/uniform/global plumbing rests on.
///
/// `f(ptr, v) = *ptr`, with `v` stored to `ptr[1]`.
#[test]
fn guest_and_host_share_the_same_bytes() {
    let code = asm(&[
        Inst::Entry(a(1), FRAME),
        Inst::Load(LoadOp::L32i, a(4), a(2), 0),
        Inst::Store(StoreOp::S32i, a(3), a(2), 4),
        Inst::MovN(a(2), a(4)),
        Inst::Nullary(NullaryOp::Retw),
    ]);
    let (mut emu, arena, entry) = emu_with_shared(BoardProfile::esp32s3(), 256, &code);

    set_word(&arena, 0, 0xDEAD_BEEF);
    let out = emu.run_loaded_with_args(
        entry,
        &[SHARED_DBUS_BASE, 0x1234_5678],
        &mut NoopTracer,
        None,
    );

    // Host write -> guest read.
    assert_eq!(expect_ok(out).0, 0xDEAD_BEEF);
    // Guest write -> host read.
    assert_eq!(word(&arena, 4), 0x1234_5678);
}

/// The shared window is data-only. Fetching from it is the hardware
/// `InstrFetchError` path — jumping into the vmctx is a bug, and the emulator
/// must model it as one rather than executing whatever bytes are there.
#[test]
fn shared_region_is_not_fetchable() {
    let (mut emu, _arena, _entry) = emu_with_shared(BoardProfile::esp32s3(), 256, &[]);
    match emu.run_loaded_with_args(SHARED_DBUS_BASE, &[0], &mut NoopTracer, None) {
        CallOutcome::Trap(t) => {
            assert_eq!(t.kind, TrapKind::Exception);
            assert_eq!(t.cause, EXC_INSTR_FETCH_ERROR);
        }
        CallOutcome::Ok { .. } => panic!("fetch from the shared region should trap"),
    }
}

/// `SHARED_DBUS_BASE` must be free on **both** board profiles. This is the
/// assertion that keeps the chosen address honest as profiles change; a comment
/// claiming "this address is unmapped" would rot silently.
#[test]
fn shared_base_is_free_on_both_profiles() {
    for profile in [BoardProfile::esp32s3(), BoardProfile::esp32()] {
        let name = profile.name;
        let arena = Arc::new(Mutex::new(vec![0u8; 256 * 1024]));
        let mut emu = Emulator::with_profile(profile);
        emu.mem.add_shared(SHARED_DBUS_BASE, arena);
        assert_eq!(
            emu.mem.read_u32(SHARED_DBUS_BASE).ok(),
            Some(0),
            "shared region unreadable on {name}"
        );
    }
}

#[test]
#[should_panic(expected = "overlaps the D-bus range")]
fn overlapping_shared_region_is_rejected() {
    let profile = BoardProfile::esp32s3();
    let base = profile.code_dbus_base;
    let mut emu = Emulator::with_profile(profile);
    emu.mem
        .add_shared(base, Arc::new(Mutex::new(vec![0u8; 256])));
}

#[test]
#[should_panic(expected = "overlaps the I-bus image")]
fn shared_region_overlapping_an_ibus_image_is_rejected() {
    let profile = BoardProfile::esp32s3();
    let base = profile.code_ibus_base();
    let mut emu = Emulator::with_profile(profile);
    emu.mem
        .add_shared(base, Arc::new(Mutex::new(vec![0u8; 256])));
}

/// Eight arguments: six in `a10..a15` (arriving as `a2..a7`) and two in the
/// caller's outgoing stack area, which the callee reads at `[SP + FRAME + 4*k]`
/// because its ENTRY moved SP down by `FRAME`.
///
/// The arguments are distinct powers of two and the program sums all eight, so
/// **any** misplacement — a wrong stack base, a duplicated register, a dropped
/// argument — changes the sum. A wrong stack-argument base is otherwise a silent
/// wrong-value bug rather than a crash, which is why this test exists.
#[test]
fn eight_arguments_land_in_the_right_places() {
    let mut insts = vec![Inst::Entry(a(1), FRAME)];
    for r in 3..=7u8 {
        insts.push(Inst::AddN(a(2), a(2), a(r)));
    }
    for k in 0..2u32 {
        insts.push(Inst::Load(LoadOp::L32i, a(8), a(1), FRAME + 4 * k));
        insts.push(Inst::AddN(a(2), a(2), a(8)));
    }
    insts.push(Inst::Nullary(NullaryOp::Retw));
    let code = asm(&insts);
    let (mut emu, _arena, entry) = emu_with_shared(BoardProfile::esp32s3(), 256, &code);

    let args = [1u32, 2, 4, 8, 16, 32, 64, 128];
    assert_eq!(args.len(), OUT_ARG_REG_COUNT + 2);
    let (lo, _) = expect_ok(emu.run_loaded_with_args(entry, &args, &mut NoopTracer, None));
    assert_eq!(lo, 255, "one or more arguments landed in the wrong place");
}

/// The register-only path (no outgoing stack area) still works, and the caller
/// SP is not disturbed when there are no stack arguments.
#[test]
fn six_arguments_use_registers_only() {
    let mut insts = vec![Inst::Entry(a(1), FRAME)];
    for r in 3..=7u8 {
        insts.push(Inst::AddN(a(2), a(2), a(r)));
    }
    insts.push(Inst::Nullary(NullaryOp::Retw));
    let code = asm(&insts);
    let (mut emu, _arena, entry) = emu_with_shared(BoardProfile::esp32s3(), 256, &code);

    let (lo, _) =
        expect_ok(emu.run_loaded_with_args(entry, &[1, 2, 4, 8, 16, 32], &mut NoopTracer, None));
    assert_eq!(lo, 63);
}

/// A two-scalar return uses the whole result bank: callee `a2`/`a3` arrive as
/// the caller's `a10`/`a11`.
#[test]
fn two_word_return_carries_both_halves() {
    let code = asm(&[
        Inst::Entry(a(1), FRAME),
        Inst::Movi(a(2), 0x111),
        Inst::Movi(a(3), 0x222),
        Inst::Nullary(NullaryOp::Retw),
    ]);
    let (mut emu, _arena, entry) = emu_with_shared(BoardProfile::esp32s3(), 256, &code);

    let (lo, hi) = expect_ok(emu.run_loaded_with_args(entry, &[0], &mut NoopTracer, None));
    assert_eq!((lo, hi), (0x111, 0x222));
}

/// An sret return needs no emulator feature at all: the buffer pointer is just
/// the first argument (callee `a2`), and the buffer lives in the shared arena so
/// the host reads the result straight out of it. Pinning that here is the point
/// — it is the path every vector-returning shader entry takes.
#[test]
fn sret_pointer_is_just_the_first_argument() {
    let mut insts = vec![Inst::Entry(a(1), FRAME)];
    for (k, v) in [11i32, 22, 33].into_iter().enumerate() {
        insts.push(Inst::Movi(a(4), v));
        insts.push(Inst::Store(StoreOp::S32i, a(4), a(2), 4 * k as u32));
    }
    insts.push(Inst::Nullary(NullaryOp::Retw));
    let code = asm(&insts);
    let (mut emu, arena, entry) = emu_with_shared(BoardProfile::esp32s3(), 256, &code);

    // args: [sret buffer, vmctx] — the sret pointer displaces the vmctx word.
    let sret = SHARED_DBUS_BASE + 64;
    expect_ok(emu.run_loaded_with_args(entry, &[sret, 0xABCD], &mut NoopTracer, None));
    assert_eq!(
        [word(&arena, 64), word(&arena, 68), word(&arena, 72)],
        [11, 22, 33]
    );
}

/// A store that runs off the end of the shared window faults rather than
/// silently wrapping into the host's `Vec` bounds check.
#[test]
fn access_past_the_shared_window_faults() {
    let (emu, _arena, _entry) = emu_with_shared(BoardProfile::esp32s3(), 256, &[]);
    assert!(emu.mem.read_u32(SHARED_DBUS_BASE + 256).is_err());
    assert!(emu.mem.read_u32(SHARED_DBUS_BASE + 252).is_ok());
}
