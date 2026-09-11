//! M7 P4: what whole-image discovery must get right, on guests small enough
//! to reason about.
//!
//! Four constructed cases, each one a rule the phase owns:
//!
//! 1. **Data in text degrades** (JD7). A walk over a whole image meets
//!    literal pools and jump tables where the spike's census never did,
//!    because a census supplies real observed block starts and a symbol table
//!    does not. Data must end a block, never be translated.
//! 2. **RVC alignment.** 48.99 % of real block starts sit at 2 mod 4, so a
//!    walk that assumes 4-byte boundaries desyncs on half the image.
//! 3. **The `fence.i` event** (JD5). The guest writes over code that has
//!    already been translated, publishes it, and jumps back in: the new
//!    instruction is what runs.
//! 4. **The same without the fence**, under `--strict-bus`, is reported as
//!    the firmware bug it is — M5's checker, still armed.
//!
//! The whole-image proof is `scripts/emu/oracle-sweep.sh` against the pinned
//! reference images; this is the part that runs on every machine with no
//! toolchain and no ELF.
#![cfg(feature = "jit")]

use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, StopCondition};
use lp_emu_esp32c6::memmap;

const CODE: u32 = memmap::HP_SRAM_BASE + 0x1000;
const EBREAK: u32 = 0x0010_0073;
const FENCE_I: u32 = 0x0000_100f;
/// A word no RV32IMC decoder accepts: opcode `0x7f` is reserved.
const NOT_AN_INSTRUCTION: u32 = 0xffff_ffff;

// --- the assembler, by hand, as `jit_identity.rs` does it -------------------

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}
fn lui(rd: u32, imm: u32) -> u32 {
    (imm & 0xffff_f000) | (rd << 7) | 0x37
}
fn sw(rs2: u32, rs1: u32, imm: i32) -> u32 {
    let imm = imm as u32 & 0xfff;
    ((imm >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | ((imm & 0x1f) << 7) | 0x23
}
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
/// `c.addi rd, imm` — two bytes, so what follows it starts at 2 mod 4.
fn c_addi(rd: u32, imm: i32) -> u16 {
    let imm = imm as u32 & 0x3f;
    (0b000 << 13) as u16
        | (((imm >> 5) & 1) << 12) as u16
        | ((rd & 0x1f) << 7) as u16
        | ((imm & 0x1f) << 2) as u16
        | 0b01
}

fn place_words(m: &mut Esp32C6Machine, at: u32, words: &[u32]) {
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    m.bus.load_image(at, &bytes).unwrap();
}

fn place_bytes(m: &mut Esp32C6Machine, at: u32, bytes: &[u8]) {
    m.bus.load_image(at, bytes).unwrap();
}

/// Registers, pc and counters, so two runs can be compared whole.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Outcome {
    regs: [i32; 32],
    pc: u32,
    cycles: u64,
    retired: u64,
}

fn outcome(m: &Esp32C6Machine) -> Outcome {
    let hart = &m.harts[0];
    Outcome {
        regs: *hart.regs(),
        pc: hart.pc(),
        cycles: hart.cycle_count(),
        retired: hart.instruction_count(),
    }
}

// --- 1. data in text --------------------------------------------------------

/// A literal pool inside a `.text` span the walk reaches by falling through a
/// branch — and which the guest itself never executes, because the branch is
/// always taken.
fn image_with_a_literal_pool() -> Vec<u32> {
    vec![
        addi(7, 0, 1),      // +0   t2 = 1
        addi(5, 5, 1),      // +4   t0 += 1
        bne(7, 0, 16),      // +8   taken, to +24
        NOT_AN_INSTRUCTION, // +12  the pool: the branch's fall-through
        NOT_AN_INSTRUCTION, // +16
        NOT_AN_INSTRUCTION, // +20
        addi(6, 6, 2),      // +24  t1 += 2
        EBREAK,             // +28
    ]
}

#[test]
fn data_in_text_ends_a_block_and_is_never_translated() {
    let mut plain = Esp32C6Builder::bare().build().unwrap();
    place_words(&mut plain, CODE, &image_with_a_literal_pool());
    plain.harts[0].set_pc(CODE);
    plain.run_until(&StopCondition::after_micros(200));
    let interpreted = outcome(&plain);
    assert_eq!(interpreted.regs[5], 1, "t0 ran");
    assert_eq!(interpreted.regs[6], 2, "t1 ran");

    let mut m = Esp32C6Builder::bare().build().unwrap();
    place_words(&mut m, CODE, &image_with_a_literal_pool());
    m.harts[0].set_pc(CODE);
    m.translate_from_seeds(&[CODE], false, 256).unwrap();
    m.run_until(&StopCondition::after_micros(200));

    assert_eq!(
        outcome(&m),
        interpreted,
        "a translated core is not architectural state, literal pool or not"
    );
    let report = m.harts[0]
        .translated_core_report()
        .expect("a core is installed");
    // Two blocks, four instructions: `addi/addi/bne` before the pool and the
    // `addi` after it. The pool is neither of them, and the `ebreak` that
    // ends the second block is the interpreter's as well.
    assert!(
        report.contains("installed 2 blocks / 4 instr"),
        "the code either side of the pool is translated and the pool is not: {report}"
    );
    assert!(
        report.contains("1 ended undecodable"),
        "the block before the pool ends at it: {report}"
    );
    // The walk reached the pool — as the branch's fall-through — and every
    // start inside it held nothing to run.
    assert!(
        !report.contains("0 named no code"),
        "the literals are starts that hold nothing: {report}"
    );
    // Every instruction the guest retired ran inside translated code — the
    // pool cost coverage of the *image*, and none of the run.
    let (covered, total) = m.translated_coverage().expect("a core is installed");
    assert_eq!((covered, total), (4, 4), "{covered} of {total}");
}

// --- 2. RVC alignment -------------------------------------------------------

/// A loop whose body sits at 2 mod 4: one compressed instruction, then
/// 32-bit ones on odd halfword boundaries.
///
/// A walk that swept on 4-byte boundaries would read the two halves of the
/// `addi` as instructions of their own and translate nonsense — or, since
/// nonsense mostly does not decode, translate nothing and cover none of the
/// run. The coverage assertion below is what tells the two apart.
fn image_at_two_mod_four() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend(c_addi(7, 20).to_le_bytes()); // +0  (2 bytes) t2 = 20
    out.extend(addi(5, 5, 1).to_le_bytes()); // +2  t0 += 1
    out.extend(addi(6, 6, 3).to_le_bytes()); // +6  t1 += 3
    out.extend(addi(7, 7, -1).to_le_bytes()); // +10 t2 -= 1
    out.extend(bne(7, 0, -12).to_le_bytes()); // +14 back to the top at +2
    out.extend(EBREAK.to_le_bytes()); // +18
    out
}

#[test]
fn a_loop_body_at_two_mod_four_is_found_and_run_in_translated_code() {
    let image = image_at_two_mod_four();

    let mut plain = Esp32C6Builder::bare().build().unwrap();
    place_bytes(&mut plain, CODE, &image);
    plain.harts[0].set_pc(CODE);
    plain.run_until(&StopCondition::after_micros(400));
    let interpreted = outcome(&plain);
    assert!(interpreted.retired > 40, "the loop ran: {interpreted:?}");

    let mut m = Esp32C6Builder::bare().build().unwrap();
    place_bytes(&mut m, CODE, &image);
    m.harts[0].set_pc(CODE);
    m.translate_from_seeds(&[CODE], false, 256).unwrap();
    m.run_until(&StopCondition::after_micros(400));

    assert_eq!(outcome(&m), interpreted, "same answer, byte for byte");
    let (covered, total) = m.translated_coverage().expect("a core is installed");
    assert!(
        covered * 2 > total,
        "most of the run should be inside translated code; a desynced walk \
         would cover almost none of it ({covered} of {total})"
    );
}

// --- 3 and 4. publishing over translated code -------------------------------

/// Where the subroutine lives, relative to `CODE`.
const SUB: u32 = 0x80;

/// A guest that calls a subroutine, rewrites its first instruction, and calls
/// it again. With `fence` the rewrite is published the way real silicon
/// requires; without, it is the firmware bug M5's checker exists to catch.
fn image_that_rewrites_its_own_code(fence: bool) -> (Vec<u32>, Vec<u32>) {
    let sub_at = CODE + SUB;
    let mut p = Vec::new();
    p.push(addi(5, 0, 0)); // t0 = 0
    p.extend(li(10, sub_at)); // a0 = &sub
    p.extend(li(11, addi(5, 5, 10))); // a1 = `addi t0, t0, 10`
    let call1 = p.len();
    p.push(0); // jal ra, sub
    p.push(sw(11, 10, 0)); // *sub = a1
    if fence {
        p.push(FENCE_I);
    }
    let call2 = p.len();
    p.push(0); // jal ra, sub
    p.push(EBREAK);
    let at = |i: usize| CODE + 4 * i as u32;
    p[call1] = jal(1, sub_at.wrapping_sub(at(call1)) as i32);
    p[call2] = jal(1, sub_at.wrapping_sub(at(call2)) as i32);

    let sub = vec![
        addi(5, 5, 1), // t0 += 1, until it is rewritten
        jalr(0, 1, 0), // ret
    ];
    (p, sub)
}

#[test]
fn a_fence_i_republishes_the_code_the_guest_wrote() {
    let (main, sub) = image_that_rewrites_its_own_code(true);

    let mut plain = Esp32C6Builder::bare().build().unwrap();
    place_words(&mut plain, CODE, &main);
    place_words(&mut plain, CODE + SUB, &sub);
    plain.harts[0].set_pc(CODE);
    plain.run_until(&StopCondition::after_micros(200));
    assert_eq!(
        plain.harts[0].regs()[5],
        11,
        "the interpreter runs the old instruction once and the new one once"
    );

    let mut m = Esp32C6Builder::bare().build().unwrap();
    place_words(&mut m, CODE, &main);
    place_words(&mut m, CODE + SUB, &sub);
    m.harts[0].set_pc(CODE);
    m.translate_from_seeds(&[CODE, CODE + SUB], false, 256)
        .unwrap();
    m.run_until(&StopCondition::after_micros(200));

    assert_eq!(
        m.harts[0].regs()[5],
        11,
        "translated code must run the instruction the guest published, not \
         the one it was translated from"
    );
    assert_eq!(m.harts[0].fence_i_count(), 1, "exactly one publish");
    assert_eq!(
        m.jit_retranslations(),
        1,
        "one `fence.i`, one retranslation — the second of JD5's two events"
    );
    let report = m.harts[0]
        .translated_core_report()
        .expect("a core is installed");
    // P5 made the `fence.i` response a **retranslation**: the publish does not
    // invalidate blocks inside the core that saw it, it builds a replacement
    // core from the republished bytes and installs that one over it. So the
    // core still installed here is the *new* one, and a new core has nothing
    // to have invalidated. The two assertions above are what carries the
    // meaning this one used to: `jit_retranslations() == 1` says the publish
    // reached the translator, and `t0 == 11` says the guest ran the
    // instruction it published rather than the one it was translated from.
    // **Rewritten by M7b P1**, because the model it described is gone.
    //
    // P5 answered a `fence.i` by building a whole replacement core and
    // installing it over the old one, so the core alive at the end of the run
    // had invalidated nothing and the assertion read `invalidations 0 (dropped
    // 0 blocks)`. That number was a property of the core having been thrown
    // away, not of anything the guest did.
    //
    // The core **survives** the event now: the read-only module is kept and the
    // writable one is retired and replaced (DD18), so the counters are the
    // run's rather than the last core's — and they say the thing the old
    // assertion could only imply. One invalidation, one block dropped: the
    // publish was seen, and the module holding the bytes it changed was
    // retired.
    assert!(
        report.contains("invalidations 1 (dropped 1 blocks)"),
        "the publish retires the module holding the block whose bytes changed, \
         and the core that survives the event says so: {report}"
    );
}

#[test]
fn publishing_without_a_fence_is_reported_as_a_firmware_bug() {
    let (main, sub) = image_that_rewrites_its_own_code(false);
    let mut m = Esp32C6Builder::bare().strict(true).build().unwrap();
    place_words(&mut m, CODE, &main);
    place_words(&mut m, CODE + SUB, &sub);
    m.harts[0].set_pc(CODE);
    m.run_until(&StopCondition::after_micros(200));

    assert_eq!(m.harts[0].fence_i_count(), 0, "the guest published nothing");
    assert!(
        m.bus.missing_fence_reports() > 0,
        "an unpublished rewrite of code the guest has already executed is \
         M5's missing-fence report, and it is still armed"
    );
}

// --- 5. the whole-module retire, twice over the same bytes (M7b P1) ---------

/// Two publishes over the **same** bytes, each with its own `fence.i`.
///
/// M7b P1 made a `fence.i` incremental: the read-only module is kept and only
/// the writable side is re-emitted. The retire that guarantees correctness is
/// unchanged by that, and this is the test that says so — the guest overwrites
/// one instruction, runs it, overwrites it again, and runs the second one.
/// Every publish has to reach the translator, and translated code has to run
/// the instruction the guest published rather than the one it was translated
/// from, both times.
#[test]
fn two_publishes_over_the_same_bytes_each_run_what_was_published() {
    const SUB2: u32 = 0x800;
    let sub_at = CODE + SUB2;
    // Long enough to cross the 8,192-cycle slice cap, so each publish gets its
    // own translation event. Without it the whole guest runs inside one slice,
    // both fences land before the machine ever looks, and the test would be
    // asserting one event where it means to assert two.
    let delay = |p: &mut Vec<u32>| {
        p.extend(li(6, 5_000)); // t1 = 5,000
        p.push(addi(6, 6, -1)); // t1 -= 1
        p.push(bne(6, 0, -4)); // until zero
    };
    let mut p = Vec::new();
    p.push(addi(5, 0, 0)); // t0 = 0
    p.extend(li(10, sub_at)); // a0 = &sub
    p.extend(li(11, addi(5, 5, 10))); // a1 = `addi t0, t0, 10`
    p.extend(li(12, addi(5, 5, 100))); // a2 = `addi t0, t0, 100`
    let call1 = p.len();
    p.push(0); // jal ra, sub          t0 += 1
    p.push(sw(11, 10, 0)); // *sub = a1
    p.push(FENCE_I);
    delay(&mut p);
    let call2 = p.len();
    p.push(0); // jal ra, sub          t0 += 10
    p.push(sw(12, 10, 0)); // *sub = a2
    p.push(FENCE_I);
    delay(&mut p);
    let call3 = p.len();
    p.push(0); // jal ra, sub          t0 += 100
    p.push(EBREAK);
    let at = |i: usize| CODE + 4 * i as u32;
    for &c in &[call1, call2, call3] {
        p[c] = jal(1, sub_at.wrapping_sub(at(c)) as i32);
    }
    let sub = vec![addi(5, 5, 1), jalr(0, 1, 0)];

    let mut plain = Esp32C6Builder::bare().build().unwrap();
    place_words(&mut plain, CODE, &p);
    place_words(&mut plain, sub_at, &sub);
    plain.harts[0].set_pc(CODE);
    plain.run_until(&StopCondition::after_micros(5_000));
    assert_eq!(
        plain.harts[0].regs()[5],
        111,
        "the interpreter runs 1, then 10, then 100"
    );

    let mut m = Esp32C6Builder::bare().build().unwrap();
    place_words(&mut m, CODE, &p);
    place_words(&mut m, sub_at, &sub);
    m.harts[0].set_pc(CODE);
    m.translate_from_seeds(&[CODE, sub_at], false, 256).unwrap();
    m.run_until(&StopCondition::after_micros(5_000));

    assert_eq!(
        m.harts[0].regs()[5],
        111,
        "translated code must run each published instruction in turn, not the \
         one the module was translated from"
    );
    assert_eq!(m.harts[0].fence_i_count(), 2, "two publishes");
    assert_eq!(
        m.jit_retranslations(),
        2,
        "both publishes reached the translator"
    );
    let report = m.harts[0]
        .translated_core_report()
        .expect("a core is installed");
    // Two blocks dropped, one per publish. The *invalidation* count is
    // larger — the hart asks whenever guest code may have changed, and a
    // question whose answer is "nothing moved" is free — so the number that
    // carries the meaning is the one that says a module was retired.
    assert!(
        report.contains("(dropped 2 blocks)"),
        "each publish retires the module holding the block it rewrote — the \
         whole-module retire, twice: {report}"
    );
    assert_eq!(
        outcome(&m),
        outcome(&plain),
        "the whole machine state agrees with the interpreter's"
    );
}

// --- 6. the incremental path itself (M7b P1) --------------------------------

/// A guest whose program lives in **read-only** memory and which publishes
/// **new** code into RAM.
///
/// This is the shape incremental translation exists for. The read-only module
/// holds the program — the guest cannot write those bytes, so it can never go
/// stale — and the `fence.i` replaces only the writable module, which is where
/// the published code lands. The assertions are the three things that have to
/// be true at once: the event took the incremental path, the machine still
/// holds two modules, and the guest ran what it published.
#[test]
fn a_fence_i_that_publishes_new_code_keeps_the_read_only_module() {
    let rom_at = memmap::FLASH_CACHE_BASE + 0x1000;
    let ram_sub = CODE;
    let published = CODE + 0x100;

    // In RAM, translated at boot: `t0 += 1; ret`.
    let ram = vec![addi(5, 5, 1), jalr(0, 1, 0)];

    // In read-only memory: call the RAM routine, write a new routine into RAM,
    // publish it, call that.
    let mut p = Vec::new();
    p.push(addi(5, 0, 0)); // t0 = 0
    p.extend(li(10, ram_sub)); // a0 = &ram_sub
    p.extend(li(11, published)); // a1 = &published
    p.extend(li(12, addi(5, 5, 7))); // a2 = `addi t0, t0, 7`
    p.extend(li(13, jalr(0, 1, 0))); // a3 = `ret`
    p.push(jalr(1, 10, 0)); // call ram_sub        t0 += 1
    p.push(sw(12, 11, 0)); // published[0] = a2
    p.push(sw(13, 11, 4)); // published[1] = a3
    p.push(FENCE_I);
    p.push(jalr(1, 11, 0)); // call published      t0 += 7
    p.push(EBREAK);

    let build = || {
        let mut m = Esp32C6Builder::bare().build().unwrap();
        place_words(&mut m, rom_at, &p);
        place_words(&mut m, ram_sub, &ram);
        m.harts[0].set_pc(rom_at);
        m
    };

    let mut plain = build();
    plain.run_until(&StopCondition::after_micros(200));
    assert_eq!(plain.harts[0].regs()[5], 8, "1 from RAM, then 7 published");

    let mut m = build();
    m.translate_from_seeds(&[rom_at, ram_sub], false, 256)
        .unwrap();
    let before = m
        .harts[0]
        .translated_core_report()
        .expect("a core is installed");
    assert!(
        before.contains("2 module(s)"),
        "boot installs the read-only half and the writable half separately: {before}"
    );

    m.run_until(&StopCondition::after_micros(200));

    assert_eq!(
        m.harts[0].regs()[5],
        8,
        "the guest ran the code it published"
    );
    assert_eq!(m.harts[0].fence_i_count(), 1, "one publish");
    assert_eq!(m.jit_retranslations(), 1, "one translation event");
    assert_eq!(
        m.jit_incremental_events(),
        1,
        "and it took the incremental path: the read-only module was kept"
    );
    let after = m
        .harts[0]
        .translated_core_report()
        .expect("a core is installed");
    assert!(
        after.contains("2 module(s)"),
        "the read-only module plus the replacement writable one: {after}"
    );
    assert_eq!(
        outcome(&m),
        outcome(&plain),
        "the whole machine state agrees with the interpreter's"
    );
}
