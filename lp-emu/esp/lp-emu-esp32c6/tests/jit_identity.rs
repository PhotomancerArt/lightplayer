//! M7 P3: a translated core produces the interpreter's answer, whatever it
//! translates.
//!
//! Three runs of the same guest — interpreted, translated, and translated with
//! **every instruction sent back through the escape hatch** — have to agree on
//! the register file, the cycle count, the retired-instruction count, the pc
//! and guest memory.
//!
//! The third run is the one that matters, and it is kept here as a permanent
//! test rather than as a stage that was passed. `Emit::NOTHING` emits no guest
//! semantics at all: the module is a dispatcher, a budget check and a
//! `step_one` call per instruction. If it agrees with the interpreter then a
//! translator that emits *some* instructions and escapes the rest cannot be
//! wrong either — it can only be slow. That is R9's "no completeness cliff",
//! and it is the single claim this milestone rests on.
//!
//! Tiny guests, no firmware, milliseconds. The whole-image proof is
//! `scripts/emu/oracle-sweep.sh` against the pinned reference images; this is
//! the part that runs on every machine and in every CI job, with no toolchain
//! and no ELF.
#![cfg(feature = "jit")]

use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, StopCondition};
use lp_emu_esp32c6::memmap;
use lp_emu_jit::translate::Emit;

const CODE: u32 = memmap::HP_SRAM_BASE + 0x1000;
/// Scratch the guest stores to and loads from — plain RAM, so a translated
/// access stays inline behind the permission byte.
const DATA: u32 = memmap::HP_SRAM_BASE + 0x2000;
/// A register the guest writes through the bus, so the MMIO import is on the
/// path rather than merely emitted. `GPIO_OUT_W1TS` sets pad bits and needs no
/// enable sequence to accept a store.
const MMIO: u32 = 0x6009_1008;
const EBREAK: u32 = 0x0010_0073;

// --- the assembler, deliberately by hand ------------------------------------

fn lui(rd: u32, imm: u32) -> u32 {
    (imm & 0xffff_f000) | (rd << 7) | 0x37
}
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}
fn op(funct7: u32, funct3: u32, rd: u32, rs1: u32, rs2: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | 0x33
}
fn add(rd: u32, rs1: u32, rs2: u32) -> u32 {
    op(0, 0, rd, rs1, rs2)
}
fn sub(rd: u32, rs1: u32, rs2: u32) -> u32 {
    op(0x20, 0, rd, rs1, rs2)
}
fn xor(rd: u32, rs1: u32, rs2: u32) -> u32 {
    op(0, 4, rd, rs1, rs2)
}
fn sltu(rd: u32, rs1: u32, rs2: u32) -> u32 {
    op(0, 3, rd, rs1, rs2)
}
fn mul(rd: u32, rs1: u32, rs2: u32) -> u32 {
    op(1, 0, rd, rs1, rs2)
}
fn divu(rd: u32, rs1: u32, rs2: u32) -> u32 {
    op(1, 5, rd, rs1, rs2)
}
fn remu(rd: u32, rs1: u32, rs2: u32) -> u32 {
    op(1, 7, rd, rs1, rs2)
}
fn slli(rd: u32, rs1: u32, shamt: u32) -> u32 {
    (shamt << 20) | (rs1 << 15) | (0b001 << 12) | (rd << 7) | 0x13
}
fn srai(rd: u32, rs1: u32, shamt: u32) -> u32 {
    (0x20 << 25) | (shamt << 20) | (rs1 << 15) | (0b101 << 12) | (rd << 7) | 0x13
}
fn lw(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (0b010 << 12) | (rd << 7) | 0x03
}
fn lbu(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (0b100 << 12) | (rd << 7) | 0x03
}
fn sw(rs2: u32, rs1: u32, imm: i32) -> u32 {
    let imm = imm as u32 & 0xfff;
    ((imm >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | ((imm & 0x1f) << 7) | 0x23
}
fn sb(rs2: u32, rs1: u32, imm: i32) -> u32 {
    let imm = imm as u32 & 0xfff;
    ((imm >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (0b000 << 12) | ((imm & 0x1f) << 7) | 0x23
}
/// `bne rs1, rs2, offset`.
fn bne(rs1: u32, rs2: u32, offset: i32) -> u32 {
    let o = offset as u32;
    (((o >> 12) & 1) << 31)
        | (((o >> 5) & 0x3f) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (0b001 << 12)
        | (((o >> 1) & 0xf) << 8)
        | (((o >> 11) & 1) << 7)
        | 0x63
}
fn jal(rd: u32, offset: i32) -> u32 {
    let o = offset as u32;
    (((o >> 20) & 1) << 31)
        | (((o >> 1) & 0x3ff) << 21)
        | (((o >> 11) & 1) << 20)
        | (((o >> 12) & 0xff) << 12)
        | (rd << 7)
        | 0x6f
}
fn jalr(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x67
}
fn li(rd: u32, value: u32) -> [u32; 2] {
    let lo = (value & 0xfff) as i32;
    let lo = if lo >= 0x800 { lo - 0x1000 } else { lo };
    let hi = value.wrapping_sub(lo as u32);
    [lui(rd, hi), addi(rd, rd, lo)]
}

/// A guest that touches every class the translator emits.
///
/// `s0` (x8) holds `DATA`, `s1` (x9) holds `MMIO`. The loop runs ten times,
/// accumulating in `t0`..`t3` through the ALU and the M extension, storing and
/// re-loading through RAM, and storing through the bus once per iteration so
/// the MMIO import and the "leave after a store" exit are on the path rather
/// than merely emitted. It ends with a subroutine call and return, so `jal`
/// and `jalr` are covered too.
fn guest() -> (Vec<u32>, u32) {
    let mut p = Vec::new();
    p.extend(li(8, DATA));
    p.extend(li(9, MMIO));
    p.push(addi(5, 0, 0)); // t0 = 0
    p.push(addi(6, 0, 1)); // t1 = 1
    p.push(addi(7, 0, 10)); // t2 = 10, the loop counter
    let top = p.len();
    p.push(add(5, 5, 6)); // t0 += t1
    p.push(slli(28, 6, 3)); // t3 = t1 << 3
    p.push(srai(28, 28, 1)); // t3 >>= 1 (arithmetic)
    p.push(xor(5, 5, 28)); // t0 ^= t3
    p.push(mul(28, 5, 6)); // t3 = t0 * t1
    p.push(divu(29, 28, 7)); // t4 = t3 / t2   (t2 is never zero here)
    p.push(remu(30, 28, 7)); // t5 = t3 % t2
    p.push(sltu(31, 29, 30)); // t6 = t4 <u t5
    p.push(sw(5, 8, 0)); // *DATA = t0
    p.push(sb(31, 8, 4)); // *(DATA+4) = t6 (byte)
    p.push(lw(28, 8, 0)); // t3 = *DATA
    p.push(lbu(29, 8, 4)); // t4 = *(DATA+4), zero-extended
    p.push(add(5, 5, 29)); // t0 += t4
    p.push(sub(5, 5, 31)); // t0 -= t6
    p.push(sw(6, 9, 0)); // the bus sees this one
    p.push(addi(6, 6, 3)); // t1 += 3
    p.push(addi(7, 7, -1)); // t2 -= 1
    let branch = p.len();
    p.push(0); // placeholder: bne t2, x0, top
    // A call and a return, so `jal`/`jalr` are on the path.
    let call = p.len();
    p.push(0); // placeholder: jal ra, sub
    p.push(EBREAK);
    let sub_at = p.len();
    p.push(add(5, 5, 5)); // t0 += t0
    p.push(jalr(0, 1, 0)); // ret

    let at = |i: usize| CODE + 4 * i as u32;
    p[branch] = bne(7, 0, at(top).wrapping_sub(at(branch)) as i32);
    p[call] = jal(1, at(sub_at).wrapping_sub(at(call)) as i32);
    (p, CODE)
}

/// What a run left behind, as one comparable value.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Outcome {
    regs: [i32; 32],
    pc: u32,
    cycles: u64,
    retired: u64,
    data: Vec<u8>,
}

fn place(m: &mut Esp32C6Machine, at: u32, program: &[u32]) {
    let bytes: Vec<u8> = program.iter().flat_map(|w| w.to_le_bytes()).collect();
    m.bus.load_image(at, &bytes).unwrap();
}

/// Run the guest with the given emission policy, or with no translated core
/// at all when `policy` is `None`.
fn run(policy: Option<Emit>) -> (Outcome, Option<String>) {
    let mut m = Esp32C6Builder::bare().build().unwrap();
    let (program, entry) = guest();
    place(&mut m, entry, &program);
    m.harts[0].set_pc(entry);

    if let Some(policy) = policy {
        // A hand-supplied seed list, which is the other half of what P3's
        // scope allows: no ELF here, so no symbols to seed from.
        let model = m.harts[0].cycle_model();
        let report = lp_emu_esp32c6::jit::install(
            &mut m.harts[0],
            &mut m.bus,
            &[entry],
            256,
            256,
            model,
            policy,
            None,
        )
        .expect("the guest translates");
        assert!(report.blocks > 0, "something was translated");
        if policy == Emit::NOTHING {
            assert_eq!(
                report.native_insts, 0,
                "the all-escape build emits no guest semantics at all"
            );
            assert!(report.escaped_insts > 0);
        } else {
            assert_eq!(
                report.escaped_insts, 0,
                "with everything emitted, nothing should reach the escape hatch"
            );
        }
    }

    m.run_until(&StopCondition::after_micros(2_000));
    let hart = &m.harts[0];
    let outcome = Outcome {
        regs: *hart.regs(),
        pc: hart.pc(),
        cycles: hart.cycle_count(),
        retired: hart.instruction_count(),
        // Straight out of the arena: a `Bus::read_byte` here would be the
        // host issuing guest accesses after the run and would show up as a
        // difference between three runs that did the same thing.
        data: {
            let base = m.bus.guest_arena_base();
            let at = (DATA - base) as usize;
            m.bus.guest_arena()[at..at + 16].to_vec()
        },
    };
    (outcome, m.harts[0].translated_core_report())
}

#[test]
fn a_translated_core_produces_the_interpreters_answer() {
    let (interpreted, none) = run(None);
    assert!(none.is_none(), "no core, no report");
    // The guest must actually have run, or three identical nothings would
    // pass this test.
    assert!(interpreted.retired > 100, "{interpreted:?}");
    assert_ne!(interpreted.regs[5], 0, "t0 accumulated");

    let (translated, report) = run(Some(Emit::EVERYTHING));
    assert_eq!(
        translated, interpreted,
        "a translated core is not architectural state"
    );
    let report = report.expect("--jit-report has something to print");
    assert!(report.contains("escape_hatch 0"), "{report}");
}

/// The proof the milestone rests on: a module that emits **no** guest
/// semantics, hands every single instruction to the interpreter through
/// `step_one`, and still produces the same everything.
#[test]
fn the_all_escape_build_is_byte_identical_and_uses_the_hatch_for_everything() {
    let (interpreted, _) = run(None);
    let (escaped, report) = run(Some(Emit::NOTHING));
    assert_eq!(escaped, interpreted);

    let report = report.expect("--jit-report has something to print");
    assert!(
        report.contains("100.0 % static escape"),
        "every instruction should be escaped: {report}"
    );
    assert!(
        !report.contains("escape_hatch 0,"),
        "and the hatch should have fired: {report}"
    );
}

/// A core that is never entered changes nothing either — the seam's own
/// promise, checked with a seed the guest never reaches.
#[test]
fn a_core_whose_entries_are_never_reached_is_invisible() {
    let (interpreted, _) = run(None);
    let mut m = Esp32C6Builder::bare().build().unwrap();
    let (program, entry) = guest();
    place(&mut m, entry, &program);
    m.harts[0].set_pc(entry);
    // Translate the subroutine only, seeded past the entry point, then run
    // from the entry: the blocks exist and are simply never entered at their
    // own pcs first.
    let model = m.harts[0].cycle_model();
    lp_emu_esp32c6::jit::install(
        &mut m.harts[0],
        &mut m.bus,
        &[entry + 4 * (program.len() as u32 - 2)],
        256,
        256,
        model,
        Emit::EVERYTHING,
        None,
    )
    .expect("the subroutine translates");
    m.run_until(&StopCondition::after_micros(2_000));
    assert_eq!(*m.harts[0].regs(), interpreted.regs);
    assert_eq!(m.harts[0].cycle_count(), interpreted.cycles);
    assert_eq!(m.harts[0].instruction_count(), interpreted.retired);
}

// --- M7b P2: polling point (c) runs inside the stay -------------------------
//
// A store is a polling point. Until P2 it was also an **exit**: the stay ended
// at every MMIO store and the hart ran the poll out in its own loop. Now the
// poll runs inside the stay, through the store's own host call, and the two
// tests below are the two answers it can give — "the hart moved" and "it did
// not".
//
// These runs use the full peripheral set (`Esp32C6Builder::new`), not
// `bare()`: the doorbell needs a real `INTPRI`, a real interrupt matrix and a
// real `PLIC_MX` behind the MMIO windows, and on `bare()` the same stores land
// on an unmapped window that raises nothing.

/// The doorbell: `INTPRI.CPU_INTR_FROM_CPU0`. A store of 1 raises interrupt
/// source `FROM_CPU_INTR0` (22) from **inside the store**, which is the shape
/// polling point (c) exists for.
const DOORBELL: u32 = memmap::periph::INTPRI + 0x90;
/// `INTERRUPT_CORE0`'s map register for source 22: which CPU interrupt the
/// matrix routes it to.
const ROUTE_FROM_CPU0: u32 = memmap::periph::INTERRUPT_CORE0 + 4 * 22;
/// `PLIC_MX`'s enable mask, and the priority of CPU interrupt 7. The reset
/// threshold is 1, so 2 is takeable.
const PLIC_ENABLE: u32 = memmap::periph::PLIC_MX;
const PLIC_PRI7: u32 = memmap::periph::PLIC_MX + 0x10 + 4 * 7;
/// The CPU interrupt the doorbell is routed to.
const CPU_INT: u32 = 7;
/// The trap handler: a marker and a tight self-loop, so nothing the handler
/// does can move `mepc` after the interrupt set it.
const HANDLER: u32 = memmap::HP_SRAM_BASE + 0x3000;

/// A guest that arms the interrupt matrix and then rings the doorbell from
/// the **middle of a block**.
///
/// Returns the program and the pc of the instruction after the store, which
/// is what `mepc` has to be: the store retired, so the hart would have
/// executed that one next.
fn doorbell_guest() -> (Vec<u32>, u32) {
    let mut p = Vec::new();
    // Route source 22 to CPU interrupt 7, give it priority 2, enable it.
    p.extend(li(8, ROUTE_FROM_CPU0));
    p.push(addi(5, 0, CPU_INT as i32));
    p.push(sw(5, 8, 0));
    p.extend(li(8, PLIC_PRI7));
    p.push(addi(5, 0, 2));
    p.push(sw(5, 8, 0));
    p.extend(li(8, PLIC_ENABLE));
    p.push(addi(5, 0, 1 << CPU_INT));
    p.push(sw(5, 8, 0));
    // The store under test, with work either side of it so it is mid-block.
    p.extend(li(9, DOORBELL));
    p.push(addi(6, 0, 1));
    p.push(addi(28, 0, 0));
    p.push(addi(29, 0, 0));
    let ring = p.len();
    p.push(sw(6, 9, 0)); // ring: this raises the line from inside the store
    p.push(addi(28, 28, 7)); // t3 — must NOT have run when the trap is taken
    p.push(addi(29, 29, 9)); // t4 — likewise
    p.push(EBREAK);
    let after_ring = CODE + 4 * (ring as u32 + 1);
    (p, after_ring)
}

/// What one doorbell run left behind, including the trap state the polling
/// point produced.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Trapped {
    regs: [i32; 32],
    pc: u32,
    cycles: u64,
    retired: u64,
    mepc: u32,
    mcause: u32,
}

fn run_doorbell(policy: Option<Emit>) -> (Trapped, u32, Option<String>) {
    let mut m = Esp32C6Builder::new().build().unwrap();
    let (program, after_ring) = doorbell_guest();
    place(&mut m, CODE, &program);
    // A marker and a self-loop: once the handler is entered nothing else can
    // trap, so `mepc` still holds what the interrupt put there.
    place(&mut m, HANDLER, &[addi(30, 0, 0x55), jal(0, 0)]);
    m.harts[0].set_pc(CODE);
    // What the ROM leaves behind on real silicon: `MPP = 3`, `MPIE`, `MIE`
    // (`csr::MSTATUS_BOOT`). Without `MIE` the poll wakes a `wfi` and
    // delivers nothing, which is a different test.
    let boot = lp_riscv_emu::mach::csr::MSTATUS_BOOT;
    assert!(m.harts[0].set_csr_raw(lp_riscv_emu::mach::csr::MSTATUS, boot));
    assert!(m.harts[0].set_csr_raw(lp_riscv_emu::mach::csr::MTVEC, HANDLER));
    assert!(m.harts[0].set_csr_raw(lp_riscv_emu::mach::csr::MIE, 1 << CPU_INT));

    if let Some(policy) = policy {
        let model = m.harts[0].cycle_model();
        let report = lp_emu_esp32c6::jit::install(
            &mut m.harts[0],
            &mut m.bus,
            &[CODE],
            256,
            256,
            model,
            policy,
            None,
        )
        .expect("the guest translates");
        assert!(report.blocks > 0, "something was translated");
    }
    m.run_until(&StopCondition::after_micros(200));
    let hart = &m.harts[0];
    let out = Trapped {
        regs: *hart.regs(),
        pc: hart.pc(),
        cycles: hart.cycle_count(),
        retired: hart.instruction_count(),
        mepc: hart.csr().mepc,
        mcause: hart.csr().mcause,
    };
    (out, after_ring, m.harts[0].translated_core_report())
}

/// **Test 1.** A store that raises a line with `MIE` set takes the trap at
/// exactly the instruction the interpreter takes it at, with the same `mepc`
/// — and the translated run gets there without leaving the stay.
#[test]
fn a_store_that_raises_an_interrupt_traps_where_the_interpreter_traps() {
    let (interpreted, after_ring, none) = run_doorbell(None);
    assert!(none.is_none(), "no core, no report");
    // Worth nothing unless the interrupt was actually taken, at the store.
    assert_eq!(
        interpreted.mepc, after_ring,
        "the store retired and the trap was taken before the next \
         instruction: {interpreted:?}"
    );
    assert_eq!(
        interpreted.mcause,
        0x8000_0000 | CPU_INT,
        "an interrupt, not an exception: {interpreted:?}"
    );
    assert_eq!(
        (interpreted.regs[28], interpreted.regs[29]),
        (0, 0),
        "the two instructions after the store did not retire: {interpreted:?}"
    );
    assert_eq!(interpreted.regs[30], 0x55, "the handler ran");

    let (translated, _, report) = run_doorbell(Some(Emit::EVERYTHING));
    assert_eq!(
        translated, interpreted,
        "the poll delivers at the same instruction, with the same mepc"
    );
    let report = report.expect("--jit-report has something to print");
    // The polling point ran inside the stay, and this one moved the hart.
    assert!(
        report.contains("after_store 0,"),
        "no stay left for a polling point: {report}"
    );
    assert!(!report.contains("polls 0 ("), "the poll ran: {report}");
    assert!(
        !report.contains("(0 left the stay)"),
        "and at least one of them moved the hart: {report}"
    );
}

/// The all-escape build has to agree too: there the store is the
/// interpreter's own, so the trap comes out of `step_one` rather than out of
/// the poll import, and the answer must still be the same.
#[test]
fn the_all_escape_build_takes_the_same_interrupt() {
    let (interpreted, _, _) = run_doorbell(None);
    let (escaped, _, _) = run_doorbell(Some(Emit::NOTHING));
    assert_eq!(escaped, interpreted);
}

/// **Test 2.** A store that raises nothing the hart has to act on no longer
/// ends the stay at all: the exit class is gone, and the polling point that
/// replaced it answered "carry on" every single time.
#[test]
fn a_store_that_raises_nothing_no_longer_ends_the_stay() {
    let (_, report) = run(Some(Emit::EVERYTHING));
    let report = report.expect("--jit-report has something to print");
    assert!(
        report.contains("after_store 0,"),
        "no stay leaves for a polling point any more: {report}"
    );
    assert!(
        !report.contains("polls 0 ("),
        "and the polling point still ran, once per store: {report}"
    );
    assert!(
        report.contains("(0 left the stay)"),
        "none of them had anything to do: {report}"
    );
}

// --- the SYSTIMER's published reads (M7b P3) -------------------------------
//
// These runs use `Esp32C6Builder::new()`, not `bare()`: on `bare()` the same
// accesses land on a window with no SYSTIMER behind it, the machine publishes
// nothing, and the test would pass while testing nothing.

const ST: u32 = memmap::periph::SYSTIMER;
const ST_CONF: i32 = 0x00;
const ST_UNIT0_OP: i32 = 0x04;
const ST_UNIT0_LOAD_HI: i32 = 0x0c;
const ST_UNIT0_LOAD_LO: i32 = 0x10;
const ST_UNIT0_VALUE_HI: i32 = 0x40;
const ST_UNIT0_VALUE_LO: i32 = 0x44;
const ST_UNIT1_VALUE_LO: i32 = 0x4c;
const ST_UNIT0_LOAD: i32 = 0x5c;
const ST_OP_UPDATE: u32 = 1 << 30;
const ST_CONF_RESET: u32 = 0x4600_0000;
const ST_UNIT0_WORK_EN: u32 = 1 << 30;

/// esp-hal's `read_count`, written out: the `unit0_op` store that latches the
/// count, then `unit0_op`, `unit0_value.lo`, `unit0_value.hi` and
/// `unit0_value.lo` again — the five accesses that are 79.3 % of the run's
/// MMIO. `s0` holds the block's base; the answers accumulate into `s2`/`s3`
/// so a wrong one is a wrong register at the end.
///
/// `filler` instructions after it move the cycle the **next** latch store
/// happens at, so the sequence is exercised at `now` values that are not
/// multiples of `CYCLES_PER_TICK`.
fn read_count(p: &mut Vec<u32>, filler: usize) {
    p.extend(li(5, ST_OP_UPDATE));
    p.push(sw(5, 8, ST_UNIT0_OP));
    p.push(lw(6, 8, ST_UNIT0_OP));
    p.push(lw(7, 8, ST_UNIT0_VALUE_LO));
    p.push(lw(28, 8, ST_UNIT0_VALUE_HI));
    p.push(lw(29, 8, ST_UNIT0_VALUE_LO));
    p.push(add(18, 18, 7));
    p.push(xor(19, 19, 28));
    p.push(add(18, 18, 6));
    p.push(xor(19, 19, 29));
    for _ in 0..filler {
        p.push(addi(20, 20, 1));
    }
}

/// A guest that reads the system timer the way the firmware does, across every
/// case the published-read path has to get right:
///
/// 1. a plain sweep at four different cycle residues;
/// 2. the same across the **52-bit wrap**, reached by loading unit 0 near the
///    top of its range;
/// 3. unit 0 **stopped** by a `conf` write — the count is then a frozen
///    constant, not `now / 10 + offset` — and started again;
/// 4. a **unit 1** read and a **byte** read of `unit0_value.lo`, neither of
///    which the block may serve.
fn systimer_guest() -> Vec<u32> {
    let mut p = Vec::new();
    p.extend(li(8, ST));
    p.push(addi(18, 0, 0));
    p.push(addi(19, 0, 0));
    p.push(addi(20, 0, 0));

    for filler in 0..4 {
        read_count(&mut p, filler);
    }

    // Near the top of the 52 bits, so the next few ticks wrap.
    p.extend(li(5, 0x000f_ffff));
    p.push(sw(5, 8, ST_UNIT0_LOAD_HI));
    p.extend(li(5, 0xffff_ffe0));
    p.push(sw(5, 8, ST_UNIT0_LOAD_LO));
    p.push(addi(5, 0, 1));
    p.push(sw(5, 8, ST_UNIT0_LOAD));
    for filler in 0..8 {
        read_count(&mut p, filler);
    }

    // Stopped: `count` is `frozen[0]`, and the latch store still latches it.
    p.extend(li(5, ST_CONF_RESET & !ST_UNIT0_WORK_EN));
    p.push(sw(5, 8, ST_CONF));
    for filler in 0..3 {
        read_count(&mut p, filler);
    }
    p.extend(li(5, ST_CONF_RESET));
    p.push(sw(5, 8, ST_CONF));
    for filler in 0..3 {
        read_count(&mut p, filler);
    }

    // Neither of these is a published word read.
    p.push(lw(21, 8, ST_UNIT1_VALUE_LO));
    p.push(lbu(22, 8, ST_UNIT0_VALUE_LO));
    p.push(EBREAK);
    p
}

fn run_systimer(policy: Option<Emit>, trace: bool) -> (Outcome, Option<String>, String) {
    let sink = lp_emu_esp_common::trace::SharedBuffer::new();
    let mut builder = Esp32C6Builder::new();
    if trace {
        builder = builder.trace(Box::new(sink.clone()), vec!["SYSTIMER".to_string()]);
    }
    let mut m = builder.build().unwrap();
    place(&mut m, CODE, &systimer_guest());
    m.harts[0].set_pc(CODE);
    if let Some(policy) = policy {
        let model = m.harts[0].cycle_model();
        let report = lp_emu_esp32c6::jit::install(
            &mut m.harts[0],
            &mut m.bus,
            &[CODE],
            256,
            256,
            model,
            policy,
            None,
        )
        .expect("the guest translates");
        assert!(report.blocks > 0, "something was translated");
    }
    m.run_until(&StopCondition::after_micros(2_000));
    let hart = &m.harts[0];
    let out = Outcome {
        regs: *hart.regs(),
        pc: hart.pc(),
        cycles: hart.cycle_count(),
        retired: hart.instruction_count(),
        data: Vec::new(),
    };
    let report = m.harts[0].translated_core_report();
    (out, report, sink.contents())
}

/// **The oracle for this phase.** The whole `read_count` sweep — across the
/// 52-bit wrap, with unit 0 stopped and started, and with the two accesses
/// the block may not serve — is the interpreter's answer to the bit, and the
/// published reads really did serve most of it.
#[test]
fn the_systimer_sequence_is_the_interpreters_to_the_bit() {
    let (interpreted, none, _) = run_systimer(None, false);
    assert!(none.is_none(), "no core, no report");
    let (translated, report, _) = run_systimer(Some(Emit::EVERYTHING), false);
    assert_eq!(
        translated, interpreted,
        "the published words answer exactly what the model's own `read_word` does"
    );
    let report = report.expect("--jit-report has something to print");
    assert!(
        !report.contains("systimer_fast 0 read(s)"),
        "the published reads were actually used: {report}"
    );
    assert!(
        !report.contains("(armed 0,"),
        "and the block armed at least once: {report}"
    );
}

/// The all-escape build reaches the SYSTIMER through `step_one` instead, which
/// is the path that disarms the block on every instruction. Same answer.
#[test]
fn the_all_escape_build_reads_the_same_timer() {
    let (interpreted, _, _) = run_systimer(None, false);
    let (escaped, _, _) = run_systimer(Some(Emit::NOTHING), false);
    assert_eq!(escaped, interpreted);
}

/// **The trace refusal.** `SocBus::read_mmio` emits a line per access and the
/// free oracle compares those lines, so with a trace running the block must
/// never arm — and the trace is then the interpreter's, line for line.
///
/// This is why the oracle's 20 ms `--trace` cells prove the *refusal* rather
/// than the path, and why the 5,500 ms browser identity row is this phase's
/// full-length oracle.
#[test]
fn a_running_trace_refuses_the_published_reads_and_the_lines_match() {
    let (interpreted, _, lines_i) = run_systimer(None, true);
    let (translated, report, lines_t) = run_systimer(Some(Emit::EVERYTHING), true);
    assert_eq!(translated, interpreted);
    assert!(!lines_i.is_empty(), "the trace wrote something");
    assert_eq!(
        lines_t, lines_i,
        "every SYSTIMER access reached the bus and was traced"
    );
    let report = report.expect("--jit-report has something to print");
    assert!(
        report.contains("systimer_fast 0 read(s) served (armed 0, disarmed 0)"),
        "the block never armed under a trace: {report}"
    );
}
