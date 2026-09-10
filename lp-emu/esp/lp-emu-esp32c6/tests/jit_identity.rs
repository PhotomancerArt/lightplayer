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
