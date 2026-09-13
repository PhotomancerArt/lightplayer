//! **The first thing P03 does, and it could have been the last.**
//!
//! `m6/notes.md` §2.3 measured the S3 image's mnemonic and special-register
//! census against the classic's. Eighteen kinds are used by the shipped S3
//! image and absent from the classic's, so **no earlier phase of this plan
//! has run one on a hart**: M1 widened `lp-xt-inst` and built `mach`, and
//! whether every widened encoding reached the *executor* was not something
//! the planning pass could establish.
//!
//! So before a memory map, before a bus, before a machine: assemble each of
//! them with [`lp_xt_inst::encode`] — never by analogy, never as a hex
//! literal (M0's `rev8` lesson and the `lsi`→`ssai` misdecode) — run it on a
//! bare [`XtHart`] over a RAM-only [`SocBus`], and assert the architectural
//! effect.
//!
//! ⚠️ **A missing arm is a finding, not something to improvise.** A
//! half-modelled `s32c1i` — an atomic that is not atomic — is exactly the
//! lie this plan exists to remove, so a gap is reported and the director
//! decides whether it becomes a phase of its own.
//!
//! # The census this file covers, and the result
//!
//! | what | sites in the image | result |
//! |---|---:|---|
//! | `s32c1i` + `wsr.scompare1` | 176 (the most-executed SR) | arm present, both outcomes |
//! | `wsr.atomctl` | 1 | arm present |
//! | `wsr.intset` | 6 | arm present (software lines only, per the RM) |
//! | `esync` | 1 | arm present |
//! | `wdtlb` / `witlb` | 1 each | arm present, read back through `rdtlb1`/`ritlb1` |
//! | `rsr.dbreakc1` / `rsr.dbreaka1` / `wsr.ibreaka0` | 4 / 1 / 1 | arms present |
//! | `rsqrt0.s` | 1 | arm present |
//! | **`salt` / `saltu`** | 6, **all outside a sized symbol** | ⚠️ **no arm — the finding.** See [`salt_and_saltu`] |
//!
//! And the three absences that make this chip *simpler* than the classic,
//! which the README repeats: **no `rsil` at all** (the classic has 82 — the
//! S3 synchronises with `s32c1i`), no `f64*` emulation block (the classic
//! has 908 sites) and no `loop*`.

use lp_emu_core::Bus;
use lp_emu_esp_common::SocBus;
use lp_emu_esp_common::bus::RamRegion;
use lp_xt_emu::cpu::CPENABLE_FPU;
use lp_xt_emu::mach::interrupt::{IntKind, IntLine};
use lp_xt_emu::mach::trap::NUM_INTERRUPTS;
use lp_xt_emu::mach::{CoreConfig, HartFault, SliceEnd, XtHart};
use lp_xt_inst::{AtomicLsOp, FReg, FpRrOp, Inst, NullaryOp, Reg, SpecialReg, SrOp, TlbOp, encode};

/// The S3's own IRAM: `vectors_seg`'s base, `0x4037_0000 + RESERVE_ICACHE`
/// (`third_party/esp-hal/ld/esp32s3/memory.x:25`). The map is P03's next
/// commit; this file names the address it will name so the gap test runs the
/// instructions where the chip runs them.
const RAM_BASE: u32 = 0x4037_8000;
const RAM_LEN: u32 = 0x8000;
/// The vector table, at the bottom of the region.
const VEC: u32 = RAM_BASE;
/// Where each program is assembled.
const CODE: u32 = RAM_BASE + 0x1000;
/// The word the synchronising accesses and the TLB tests work on.
const DATA: u32 = RAM_BASE + 0x2000;

/// A software CPU-interrupt line, so `wsr.intset` has something it is
/// architecturally allowed to raise: the RM's `INTSET` sets **software**
/// lines only (`lp-xt-emu/src/mach/interrupt.rs:161-168`), and the real
/// line numbers are the S3's interrupt table, which is P04's.
const IRQ_SOFTWARE: u8 = 7;

fn config() -> CoreConfig {
    let mut interrupts = [IntLine::UNUSED; NUM_INTERRUPTS];
    interrupts[usize::from(IRQ_SOFTWARE)] = IntLine::new(1, IntKind::Software);
    CoreConfig {
        reset_pc: CODE,
        reset_vecbase: VEC,
        // Not a hart index and not the S3's number — this file builds no
        // machine. `machine::core_config` is where the chip's `PRID` lives.
        prid: 0,
        interrupts,
    }
}

/// A bare hart over one flat, executable, writable region. No peripheral, no
/// MMIO window, no alias: the point is the instruction, not the chip.
fn bare() -> (XtHart<SocBus>, SocBus) {
    let mut bus = SocBus::new();
    bus.reserve_guest_span(RAM_BASE, RAM_LEN);
    bus.add_region(RamRegion::new("gap-test-ram", RAM_BASE, RAM_LEN).executable());
    // Two, as on this part: `XCHAL_NUM_DBREAK` is 2 on LX6 and LX7, and the
    // second slot is what `rsr.dbreakc1` / `rsr.dbreaka1` reach.
    bus.set_watchpoint_slots(2);
    let hart = XtHart::new(0, config());
    (hart, bus)
}

/// Assemble `program` at `at`, through the bus's own host placement path.
fn asm(bus: &mut SocBus, at: u32, program: &[Inst]) {
    let mut bytes = Vec::new();
    for inst in program {
        bytes.extend_from_slice(&encode(inst));
    }
    bus.load_image(at, &bytes).expect("placing the program");
}

/// `break 1, 15` — what every program here ends with, so a run stops at a
/// known pc instead of running off into zeros.
fn brk() -> Inst {
    Inst::Break(1, 15)
}

fn a(n: u8) -> Reg {
    Reg::new(n)
}

fn f(n: u8) -> FReg {
    FReg::new(n)
}

fn movi(reg: u8, value: i32) -> Inst {
    Inst::Movi(a(reg), value)
}

fn wsr(reg: SpecialReg, from: u8) -> Inst {
    Inst::Sr(SrOp::Wsr, reg, a(from))
}

fn rsr(reg: SpecialReg, into: u8) -> Inst {
    Inst::Sr(SrOp::Rsr, reg, a(into))
}

/// Run `program` at [`CODE`] to its `break`, and say where it stopped.
fn run(hart: &mut XtHart<SocBus>, bus: &mut SocBus, program: &[Inst]) -> SliceEnd {
    asm(bus, CODE, program);
    hart.set_pc(CODE);
    hart.run_slice(bus, 1_000)
}

/// Assert the program reached its `break` rather than faulting on the way.
fn ran_to_break(end: SliceEnd) {
    match end {
        SliceEnd::Ebreak { .. } => {}
        SliceEnd::Fault(HartFault::UnsupportedInstruction { pc, word, len }) => panic!(
            "ISA GAP: the hart has no arm for the word at {pc:#010x} ({word:#010x}, {len} B). \
             This is an M1-shaped finding — report it, do not improvise an arm."
        ),
        other => panic!("the program did not reach its break: {other:?}"),
    }
}

fn word_at(bus: &mut SocBus, address: u32) -> u32 {
    bus.read_word(address).expect("reading guest memory") as u32
}

fn set_word(bus: &mut SocBus, address: u32, value: u32) {
    bus.load_image(address, &value.to_le_bytes())
        .expect("seeding guest memory");
}

// ---------------------------------------------------------------------------
// 1. `s32c1i` + `SCOMPARE1` — 176 sites, the most-executed SR in the image
// ---------------------------------------------------------------------------

/// **The one that matters most.** The S3 has no `rsil` anywhere; every
/// critical section in the shipped image is a compare-and-swap against
/// `SCOMPARE1`. Both outcomes are asserted, because the *failing* arm is
/// what the guest's `critical-section` reads to know it lost the race — an
/// `s32c1i` that always stored would deadlock nothing and corrupt everything.
#[test]
fn s32c1i_swaps_only_against_scompare1_and_always_returns_the_old_word() {
    // Both constants fit `movi`'s twelve signed bits, so the program needs
    // no literal pool and the test asserts the swap rather than its own
    // arithmetic.
    const HELD: u32 = 0x111;
    const WANTED: u32 = 0x222;
    const OTHER: u32 = 0x333;

    // The match: memory holds what SCOMPARE1 holds, so the store lands.
    let (mut hart, mut bus) = bare();
    set_word(&mut bus, DATA, HELD);
    hart.cpu_mut().set_a(2, DATA);
    hart.cpu_mut().set_a(3, WANTED);
    let end = run(
        &mut hart,
        &mut bus,
        &[
            movi(4, HELD as i32),
            wsr(SpecialReg::Scompare1, 4),
            Inst::AtomicLs(AtomicLsOp::S32c1i, a(3), a(2), 0),
            brk(),
        ],
    );
    ran_to_break(end);
    assert_eq!(word_at(&mut bus, DATA), WANTED, "matched: the swap stored");
    assert_eq!(
        hart.cpu().a(3),
        HELD,
        "and the register carries the OLD word back"
    );

    // The mismatch: memory holds something else, so nothing is stored and
    // the register still carries what was really there.
    let (mut hart, mut bus) = bare();
    set_word(&mut bus, DATA, OTHER);
    hart.cpu_mut().set_a(2, DATA);
    hart.cpu_mut().set_a(3, WANTED);
    let end = run(
        &mut hart,
        &mut bus,
        &[
            movi(4, HELD as i32),
            wsr(SpecialReg::Scompare1, 4),
            Inst::AtomicLs(AtomicLsOp::S32c1i, a(3), a(2), 0),
            brk(),
        ],
    );
    ran_to_break(end);
    assert_eq!(
        word_at(&mut bus, DATA),
        OTHER,
        "unmatched: memory is untouched"
    );
    assert_eq!(
        hart.cpu().a(3),
        OTHER,
        "and the register carries what was really there — how the guest \
         tells a lost race from a won one"
    );
}

/// `SCOMPARE1` round-trips, which is what makes the arm above testable from
/// the guest's side too.
#[test]
fn scompare1_round_trips() {
    let (mut hart, mut bus) = bare();
    let end = run(
        &mut hart,
        &mut bus,
        &[
            movi(2, 0x5a5),
            wsr(SpecialReg::Scompare1, 2),
            rsr(SpecialReg::Scompare1, 3),
            brk(),
        ],
    );
    ran_to_break(end);
    assert_eq!(hart.cpu().a(3), 0x5a5);
    assert_eq!(hart.sr().scompare1, 0x5a5);
}

// ---------------------------------------------------------------------------
// 2. `ATOMCTL` — one site, and it is what gates `s32c1i` on silicon
// ---------------------------------------------------------------------------

/// `wsr.atomctl` / `rsr.atomctl` round-trip.
///
/// ⚠️ **What this asserts and what it does not.** `ATOMCTL` selects, per
/// memory class, whether `s32c1i` is an internal RCW, a bus RCW or an
/// exception. This hart implements `s32c1i` as one indivisible read-modify-
/// write **whatever `ATOMCTL` holds** — a single-hart machine has no other
/// observer, so the protocol the guest can see is identical. The register is
/// accept-and-remember, and that is stated rather than left for a reader to
/// discover: a machine that grew a second hart sharing memory would have to
/// revisit it.
#[test]
fn atomctl_round_trips_and_is_accept_and_remember() {
    let (mut hart, mut bus) = bare();
    let end = run(
        &mut hart,
        &mut bus,
        &[
            // `0x15` — the value ESP-IDF writes: RCW for every class.
            movi(2, 0x15),
            wsr(SpecialReg::Atomctl, 2),
            rsr(SpecialReg::Atomctl, 3),
            brk(),
        ],
    );
    ran_to_break(end);
    assert_eq!(hart.cpu().a(3), 0x15);
    assert_eq!(hart.sr().atomctl, 0x15);
}

// ---------------------------------------------------------------------------
// 3. `wsr.intset` — the guest raises its own interrupt, six sites
// ---------------------------------------------------------------------------

/// The S3 image raises its own CPU interrupt through `INTSET` (six sites);
/// the classic image never does.
///
/// The RM restricts `INTSET` to **software** lines, and this hart enforces
/// that — so the test asserts both halves: a software line is raised, and a
/// line of another kind written in the same word is not.
#[test]
fn wsr_intset_raises_the_guests_own_software_line_and_only_a_software_line() {
    let (mut hart, mut bus) = bare();
    let end = run(
        &mut hart,
        &mut bus,
        &[
            // Bit 7 (the software line) and bit 6 (unused in this config,
            // therefore not a software line) in one word.
            movi(2, (1 << IRQ_SOFTWARE) | (1 << 6)),
            wsr(SpecialReg::Interrupt, 2),
            rsr(SpecialReg::Interrupt, 3),
            brk(),
        ],
    );
    ran_to_break(end);
    assert_eq!(
        hart.cpu().a(3),
        1 << IRQ_SOFTWARE,
        "the software line latched; the non-software bit in the same word did not"
    );

    // And `INTCLEAR` takes it back off, which is the other half of the
    // protocol the guest's own doorbell uses.
    let end = run(
        &mut hart,
        &mut bus,
        &[
            movi(2, 1 << IRQ_SOFTWARE),
            wsr(SpecialReg::Intclear, 2),
            rsr(SpecialReg::Interrupt, 3),
            brk(),
        ],
    );
    ran_to_break(end);
    assert_eq!(hart.cpu().a(3), 0, "cleared");
}

// ---------------------------------------------------------------------------
// 4. `esync` — one site
// ---------------------------------------------------------------------------

/// `esync` retires and the instruction after it runs. There is nothing else
/// to assert: on this machine every store is visible to the next load with
/// no buffer in between, so the barrier is architecturally a no-op — and the
/// thing that would break a boot is not "`esync` did nothing", it is "`esync`
/// stopped the hart".
#[test]
fn esync_retires() {
    let (mut hart, mut bus) = bare();
    let end = run(
        &mut hart,
        &mut bus,
        &[
            movi(2, 1),
            Inst::Nullary(NullaryOp::Esync),
            movi(3, 2),
            brk(),
        ],
    );
    ran_to_break(end);
    assert_eq!(hart.cpu().a(2), 1);
    assert_eq!(hart.cpu().a(3), 2, "the instruction after the barrier ran");
}

// ---------------------------------------------------------------------------
// 5. `wdtlb` / `witlb` — one site each; the S3's MMU is reachable from the ISA
// ---------------------------------------------------------------------------

/// A TLB write is remembered and reads back through its matching read.
///
/// The region is the top three bits of the address register
/// (`lp-xt-emu/src/mach/exec.rs:390-410`), so `0x4037_8000` is region 2.
/// Instruction and data TLBs are separate arrays, and the test writes
/// different attributes to each to prove they are.
#[test]
fn wdtlb_and_witlb_write_the_region_attribute_each_reads_back() {
    let (mut hart, mut bus) = bare();
    let region_addr = 0x4000_0000u32; // region 2
    let end = run(
        &mut hart,
        &mut bus,
        &[
            movi(2, 0x4),
            Inst::Slli(a(2), a(2), 28), // a2 = 0x4000_0000
            movi(3, 0x3),               // the data attribute
            Inst::Tlb(TlbOp::Wdtlb, a(3), a(2)),
            movi(4, 0x5), // a different instruction attribute
            Inst::Tlb(TlbOp::Witlb, a(4), a(2)),
            Inst::Tlb(TlbOp::Rdtlb1, a(5), a(2)),
            Inst::Tlb(TlbOp::Ritlb1, a(6), a(2)),
            brk(),
        ],
    );
    ran_to_break(end);
    assert_eq!(hart.cpu().a(2), region_addr, "the test's own address");
    assert_eq!(
        hart.cpu().a(5),
        region_addr | 0x3,
        "rdtlb1 returns the VPN with the attribute wdtlb wrote"
    );
    assert_eq!(
        hart.cpu().a(6),
        region_addr | 0x5,
        "and the instruction TLB is a separate array"
    );
}

// ---------------------------------------------------------------------------
// 6. The second DBREAK slot, and IBREAKA0
// ---------------------------------------------------------------------------

/// `rsr.dbreaka1` / `rsr.dbreakc1` (4 + 1 sites) and `wsr.ibreaka0` (1).
///
/// The **second** DBREAK slot is what this exercises: the classic image only
/// ever touched slot 0. A write to either half of the pair re-arms the bus's
/// watchpoint, so this is also the check that slot 1 exists on the bus
/// (`bus.set_watchpoint_slots(2)` above — `XCHAL_NUM_DBREAK` on LX6/LX7).
#[test]
fn the_second_dbreak_slot_and_ibreaka0_round_trip() {
    let (mut hart, mut bus) = bare();
    let end = run(
        &mut hart,
        &mut bus,
        &[
            movi(2, 0x4),
            Inst::Slli(a(2), a(2), 28), // a2 = 0x4000_0000, a plausible address
            wsr(SpecialReg::Dbreaka1, 2),
            // DBREAKC: a store break on four bytes. The value is the test's,
            // not a claim about what the image writes.
            movi(3, 0x3c),
            wsr(SpecialReg::Dbreakc1, 3),
            wsr(SpecialReg::Ibreaka0, 2),
            rsr(SpecialReg::Dbreaka1, 4),
            rsr(SpecialReg::Dbreakc1, 5),
            rsr(SpecialReg::Ibreaka0, 6),
            brk(),
        ],
    );
    ran_to_break(end);
    assert_eq!(hart.cpu().a(4), 0x4000_0000, "DBREAKA1");
    assert_eq!(hart.cpu().a(5), 0x3c, "DBREAKC1");
    assert_eq!(hart.cpu().a(6), 0x4000_0000, "IBREAKA0");
}

// ---------------------------------------------------------------------------
// 7. `rsqrt0.s` — one site
// ---------------------------------------------------------------------------

/// `rsqrt0.s` produces a reciprocal-square-root **seed**, not a result.
///
/// The assertion is a bound, not a value: the hart's answer comes from the
/// measured ROM table (`lp-xt-emu/src/fp_rom.rs`), and pinning the exact
/// word here would only restate that table — a test that cannot fail for the
/// reason it was written. What a boot depends on is that the seed is finite,
/// positive and close enough for the Newton step that follows it, so that is
/// what is asserted.
#[test]
fn rsqrt0_s_seeds_a_reciprocal_square_root() {
    for input in [1.0f32, 4.0, 0.25, 100.0] {
        let (mut hart, mut bus) = bare();
        // Bit 0 of CPENABLE gates the FPU. `machine::CPENABLE_RESET` is a
        // builder parameter on this chip (A4) and is deliberately not
        // assumed here: this test arms the bit itself.
        hart.cpu_mut().cpenable = CPENABLE_FPU;
        set_word(&mut bus, DATA, input.to_bits());
        hart.cpu_mut().set_a(2, DATA);
        let end = run(
            &mut hart,
            &mut bus,
            &[
                Inst::Load(lp_xt_inst::LoadOp::L32i, a(3), a(2), 0),
                Inst::Wfr(f(0), a(3)),
                Inst::FpRr(FpRrOp::Rsqrt0S, f(1), f(0)),
                Inst::Rfr(a(4), f(1)),
                brk(),
            ],
        );
        ran_to_break(end);
        let seed = f32::from_bits(hart.cpu().a(4));
        let want = 1.0f32 / input.sqrt();
        assert!(
            seed.is_finite() && seed > 0.0,
            "rsqrt0.s({input}) = {seed}, which is not a usable seed"
        );
        let relative = ((seed - want) / want).abs();
        assert!(
            relative < 0.02,
            "rsqrt0.s({input}) = {seed}, want ~{want} (relative error {relative})"
        );
    }
}

// ---------------------------------------------------------------------------
// 8. `salt` / `saltu` — THE FINDING
// ---------------------------------------------------------------------------

/// ⚠️ **`salt` and `saltu` have no arm, and this test does not add one.**
///
/// `lp-xt-inst` has no `Inst` variant for either — so they cannot be
/// assembled by [`lp_xt_inst::encode`], which is the only way this file is
/// allowed to produce a word. Writing the encoding down by analogy is
/// precisely M0's `rev8` mistake (`rev8` on RV32 is `0x698`, on RV64
/// `0x6b8`, and the arm transcribed by analogy was wrong for seven of
/// fourteen `Zb*` forms), so the gap is **reported, not filled**.
///
/// What the inventory says about the risk (`m6/notes.md` §2.3): the shipped
/// S3 image has **six** apparent `salt`/`saltu` sites and **every one of them
/// is outside a sized code symbol** — interleaved literal-pool bytes that
/// `objdump` mis-decodes, the same phantom the 3,354 apparent `ee.*` sites
/// turned out to be (§2.2). So no *executed* instruction in this image is one.
///
/// What this test asserts instead is the property that makes the gap safe to
/// carry: **a word this hart cannot decode is a named stop, never a silent
/// wrong answer.** If a future image really does execute a `salt`, a strict
/// bring-up run says which word at which pc rather than quietly doing
/// something else.
#[test]
fn salt_and_saltu() {
    let (mut hart, mut bus) = bare();
    hart.set_strict_unsupported(true);
    // Not an encoding of anything: `0x00_00_ff` has `op0 = 0xf`, which the
    // base-ISA length rule reads as a 24-bit instruction and the decoder
    // refuses. It stands in for "a word from an extension this hart does not
    // implement" — the class `salt`/`saltu` belong to until an arm exists.
    bus.load_image(CODE, &[0xff, 0xff, 0xff])
        .expect("placing the undecodable word");
    hart.set_pc(CODE);
    match hart.run_slice(&mut bus, 10) {
        SliceEnd::Fault(HartFault::UnsupportedInstruction { pc, word, .. }) => {
            assert_eq!(pc, CODE, "the stop names the pc");
            assert_ne!(word, 0, "and the word");
        }
        other => panic!(
            "an undecodable word must be a named stop under --strict-bus's \
             unsupported-opcode policy, not {other:?}"
        ),
    }
}

// ---------------------------------------------------------------------------
// 9. The absences, asserted as absences
// ---------------------------------------------------------------------------

/// The three things that make the S3 **simpler** than the classic, kept here
/// as a live check rather than only as prose in the README.
///
/// `rsil` is the one that matters: the classic image has 82 sites and the S3
/// image has none, because the S3 synchronises with `s32c1i` instead. The
/// hart implements `rsil` anyway (it is core ISA), and this asserts that —
/// the claim is about the *image*, not about the hart, and a test that
/// pretended otherwise would be measuring the wrong thing.
#[test]
fn rsil_still_works_even_though_the_s3_image_never_uses_it() {
    let (mut hart, mut bus) = bare();
    // Read before, so the assertion is "a2 holds what PS held" and not a
    // restatement of `PS_RESET` — which on this hart is INTLEVEL 15, and
    // hard-coding that here would make the test about the reset value.
    let before = hart.ps();
    let end = run(
        &mut hart,
        &mut bus,
        &[Inst::Rsil(a(2), 3), rsr(SpecialReg::Ps, 3), brk()],
    );
    ran_to_break(end);
    assert_eq!(hart.ps() & 0xf, 3, "PS.INTLEVEL is what rsil set");
    assert_eq!(
        hart.cpu().a(2),
        before,
        "and a2 holds the PS that was there before"
    );
}
