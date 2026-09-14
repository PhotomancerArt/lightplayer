//! **Publish-by-store**, the classic's second translation event (M7 P07,
//! XD10): the guest writes code from *inside translated code*, calls it, and
//! both cores run it.
//!
//! A hand-built fixture, no firmware — the reason `dual_core.rs` gives, which
//! holds twice over here: a test that depends on the shipped image reaching a
//! condition is not a test, and the condition this one needs (a store into
//! executable memory made by a translated stay, then a call into the bytes it
//! wrote) is exactly the one the product hits once per project load and
//! nowhere a firmware test could point at.
//!
//! # What the fixture does
//!
//! Core 0 performs the DPORT release so core 1 comes up at its own program,
//! runs a loop long enough to be translated, then **stores a small function
//! into SRAM0 a word at a time** and calls it. The store is the invalidation
//! event (XD3): the bus records it, the hart drains it at polling point (c),
//! the translated core answers by re-reading its modules' bytes, and only the
//! **writable** module goes stale. At the next slice boundary the machine
//! retires that module and re-emits it from the code as it now stands, seeded
//! from the spans the guest wrote.
//!
//! Core 1 spins on a flag and then calls the same published function, which
//! is what makes this a two-core test rather than a one-core one: the app
//! core's own modules have to hold the published bytes too.
//!
//! # How it is judged
//!
//! The same fixture is run twice — once with translated cores installed and
//! once without — and every architectural reading is compared: the words the
//! programs left in DRAM, both harts' retired-instruction counts, and the
//! machine's cycle count. **That is the invariant**: a run is a pure function
//! of the instruction stream, and nothing about whether a block was
//! translated may appear in it.

use lp_emu_core::Bus;
use lp_emu_esp32v3::machine::{
    BootFrame, BootMode, Esp32V3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32v3::memmap;
use lp_emu_esp32v3::periph::dport::{APPCPU_CTRL_A, APPCPU_CTRL_B, APPCPU_CTRL_C, APPCPU_CTRL_D};
use lp_xt_inst::{BrZ, CallOp, Inst, LoadOp, NullaryNarrowOp, NullaryOp, Reg, StoreOp};

// ---------------------------------------------------------------------------
// The fixture's memory map, all of it inside SRAM0 and DRAM
// ---------------------------------------------------------------------------

/// Core 0's literals.
const LIT0: u32 = 0x4008_9000;
/// Core 0's program.
const CODE0: u32 = 0x4008_9080;
/// Core 1's literals.
const LIT1: u32 = 0x4008_9300;
/// Core 1's program — the address core 0 writes into `appcpu_boot_addr`.
const CODE1: u32 = 0x4008_9320;
/// **The JIT region**, in miniature: executable SRAM0 the guest writes into
/// and then executes. The product's own is `0x4008_8000..0x4008_9914`.
const PUBLISHED: u32 = 0x4008_8800;
/// Where core 0 leaves the published function's answer.
const ANSWER0: u32 = 0x3ffe_2000;
/// Where core 1 leaves it.
const ANSWER1: u32 = ANSWER0 + 4;
/// The word core 0 sets once it has published, which is what core 1 spins on.
const FLAG: u32 = ANSWER0 + 8;
/// What the published function returns — an arbitrary constant, chosen so a
/// zero (never called) and a stale answer are both visible.
const PUBLISHED_ANSWER: i32 = 0x5a;
/// How many turns of its two-instruction warm-up loop core 0 takes before it
/// publishes. Long enough that the translated core is well inside a stay when
/// the stores land.
const WARMUP: u32 = 4_000;

// The literal slots, by offset from `LIT0`.
const L_DPORT: u32 = 0;
const L_CODE1: u32 = 4;
const L_WARMUP: u32 = 8;
const L_PUBLISHED: u32 = 12;
const L_ANSWER0: u32 = 16;
/// The published function's words start here.
const L_WORDS: u32 = 32;

fn reg(n: u8) -> Reg {
    Reg::new(n)
}

/// `l32r`'s raw 16-bit field: `(label - ((pc + 3) & !3)) / 4`, backward only.
fn l32r_field(pc: u32, label: u32) -> u16 {
    let base = (pc + 3) & !3;
    (((label as i64 - base as i64) >> 2) as i32 as u32 & 0xffff) as u16
}

/// `call op, pc -> target`: the RM's `nextPC = (PC & ~3) + 4 + offset*4`,
/// solved for the offset.
fn call(op: CallOp, pc: u32, target: u32) -> Inst {
    let base = (pc & !3).wrapping_add(4);
    Inst::Call(op, (target.wrapping_sub(base) as i32) >> 2)
}

/// Lay `insts` out from `at`, returning `(pc, inst)` pairs.
fn assemble(at: u32, insts: &[Inst]) -> Vec<(u32, Inst)> {
    let mut pc = at;
    insts
        .iter()
        .map(|inst| {
            let here = pc;
            pc += lp_xt_inst::encode(inst).len() as u32;
            (here, *inst)
        })
        .collect()
}

/// Where the next instruction would land.
fn next_pc(at: u32, insts: &[Inst]) -> u32 {
    at + insts
        .iter()
        .map(|i| lp_xt_inst::encode(i).len() as u32)
        .sum::<u32>()
}

/// The function the guest publishes: `movi.n a2, ANSWER; ret.n`.
///
/// Two instructions, four bytes, one word store — deliberately small, because
/// what is under test is the *event*, not the walk's reach. `ret.n` and not
/// `retw.n`: `call0` does not rotate the window.
fn published_function() -> Vec<u8> {
    let mut out = Vec::new();
    for inst in [
        Inst::MoviN(reg(2), PUBLISHED_ANSWER),
        Inst::NullaryN(NullaryNarrowOp::RetN),
    ] {
        out.extend(lp_xt_inst::encode(&inst));
    }
    // A word store publishes four bytes at a time and the walk's third path
    // seeds word-aligned addresses, so pad to a whole word.
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

/// The published function's bytes as the words core 0 stores.
fn published_words() -> Vec<u32> {
    published_function()
        .chunks(4)
        .map(|c| u32::from_le_bytes(c.try_into().expect("padded to whole words")))
        .collect()
}

/// Core 0: release core 1, warm up, publish the function with word stores,
/// call it, record the answer, set the flag, park.
fn program0() -> Vec<(u32, Inst)> {
    let (a2, a3, a4, a5) = (reg(2), reg(3), reg(4), reg(5));
    let store = |off: u32| Inst::Store(StoreOp::S32i, a3, a2, off);
    let mut insts: Vec<Inst> = Vec::new();

    // esp-hal's `start_core1`, as `dual_core.rs` writes it: the boot address,
    // the clock gate, the runstall, the `appcpu_resetting` pulse.
    insts.push(Inst::L32r(a2, l32r_field(CODE0, LIT0 + L_DPORT)));
    insts.push(Inst::L32r(
        a3,
        l32r_field(next_pc(CODE0, &insts), LIT0 + L_CODE1),
    ));
    insts.push(store(APPCPU_CTRL_D));
    insts.push(Inst::MoviN(a3, 1));
    insts.push(store(APPCPU_CTRL_B));
    insts.push(Inst::MoviN(a3, 0));
    insts.push(store(APPCPU_CTRL_C));
    insts.push(Inst::MoviN(a3, 1));
    insts.push(store(APPCPU_CTRL_A));
    insts.push(Inst::MoviN(a3, 0));
    insts.push(store(APPCPU_CTRL_A));

    // The warm-up loop: `l32r a4, WARMUP` then `addi a4, a4, -1 ; bnez a4`.
    insts.push(Inst::L32r(
        a4,
        l32r_field(next_pc(CODE0, &insts), LIT0 + L_WARMUP),
    ));
    let loop_pc = next_pc(CODE0, &insts);
    insts.push(Inst::Addi(a4, a4, -1));
    let bnez_pc = loop_pc + 3;
    insts.push(Inst::BranchZ(
        BrZ::Bnez,
        a4,
        loop_pc as i32 - (bnez_pc as i32 + 4),
    ));

    // **The publish**: `a3 = &PUBLISHED`, then one `s32i` per word out of a
    // literal each. Every one of these stores is the invalidation event.
    insts.push(Inst::L32r(
        a3,
        l32r_field(next_pc(CODE0, &insts), LIT0 + L_PUBLISHED),
    ));
    for i in 0..published_words().len() as u32 {
        insts.push(Inst::L32r(
            a5,
            l32r_field(next_pc(CODE0, &insts), LIT0 + L_WORDS + 4 * i),
        ));
        insts.push(Inst::Store(StoreOp::S32i, a5, a3, 4 * i));
    }
    // The product's own publish ends with an `isync`, and so does this: it is
    // a whole flush on the core, and the seam has to survive one landing in
    // the middle of a translated stay.
    insts.push(Inst::Nullary(NullaryOp::Isync));

    // Call the bytes just written, and store what came back.
    let call_pc = next_pc(CODE0, &insts);
    insts.push(call(CallOp::Call0, call_pc, PUBLISHED));
    insts.push(Inst::L32r(
        a3,
        l32r_field(next_pc(CODE0, &insts), LIT0 + L_ANSWER0),
    ));
    insts.push(Inst::Store(StoreOp::S32i, a2, a3, 0));
    insts.push(Inst::MoviN(a2, 1));
    insts.push(Inst::Store(StoreOp::S32i, a2, a3, FLAG - ANSWER0));
    insts.push(Inst::Nullary(NullaryOp::Memw));
    insts.push(Inst::J(-4));
    assemble(CODE0, &insts)
}

/// Core 1: spin on the flag, call the published function, record the answer,
/// park.
fn program1() -> Vec<(u32, Inst)> {
    let (a2, a3) = (reg(2), reg(3));
    let mut insts = vec![Inst::L32r(a3, l32r_field(CODE1, LIT1))];
    let spin_pc = next_pc(CODE1, &insts);
    insts.push(Inst::Load(LoadOp::L32i, a2, a3, FLAG - ANSWER0));
    let beqz_pc = spin_pc + 3;
    insts.push(Inst::BranchZ(
        BrZ::Beqz,
        a2,
        spin_pc as i32 - (beqz_pc as i32 + 4),
    ));
    let call_pc = next_pc(CODE1, &insts);
    insts.push(call(CallOp::Call0, call_pc, PUBLISHED));
    insts.push(Inst::Store(StoreOp::S32i, a2, a3, ANSWER1 - ANSWER0));
    insts.push(Inst::Nullary(NullaryOp::Memw));
    insts.push(Inst::J(-4));
    assemble(CODE1, &insts)
}

/// A ROM-up machine with both programs in SRAM0 and core 0 pointed at its
/// own, optionally with a translated core installed.
///
/// **ROM-up keeps translation off at the builder's door** (XD4/XD10) and this
/// calls [`Machine::install_translated_cores`] directly, which is the
/// machine-level seam the direct-load boot event uses. The rule itself is
/// `jit_default.rs`'s subject; what is wanted here is the event, on a fixture
/// small enough that cranelift is measured in milliseconds.
fn fixture(translate: bool) -> Machine {
    // ⚠️ **`--cache-off-fetch permit`, and the fixture would prove nothing
    // without it.** D4's cache-off watch is a `MemoryCost` on the bus, and a
    // bus with a memory cost model is not a pure fetch — so `XtJitCore::run`
    // refuses every entry (`impure`) and the whole run interprets. On a ROM-up
    // machine with no app the flash cache is never enabled, so the watch is
    // armed from the first instruction to the last. This was measured here,
    // as 595,980 `impure` refusals and zero entries, before the assertions
    // below existed to catch it.
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .cache_off_fetch(lp_emu_esp32v3::cache::CacheOffPolicy::Permit)
        .build()
        .expect("a machine with no app still has a bus and a ROM");

    {
        let bus = machine.bus_mut();
        let mut word = |at: u32, v: u32| {
            bus.load_image(at, &v.to_le_bytes())
                .expect("SRAM0 holds a literal");
        };
        word(LIT0 + L_DPORT, memmap::periph::DPORT);
        word(LIT0 + L_CODE1, CODE1);
        word(LIT0 + L_WARMUP, WARMUP);
        word(LIT0 + L_PUBLISHED, PUBLISHED);
        word(LIT0 + L_ANSWER0, ANSWER0);
        for (i, w) in published_words().iter().enumerate() {
            word(LIT0 + L_WORDS + 4 * i as u32, *w);
        }
        word(LIT1, ANSWER0);
    }
    for (at, inst) in program0().into_iter().chain(program1()) {
        machine
            .bus_mut()
            .load_image(at, &lp_xt_inst::encode(&inst))
            .expect("SRAM0 holds the code");
    }
    machine
        .seed_boot_state(CODE0, BootFrame::at(memmap::ROM_PRO_STACK_TOP))
        .expect("seeded");
    if translate {
        install(&mut machine);
    }
    machine
}

#[cfg(feature = "jit")]
fn install(machine: &mut Machine) {
    machine
        .install_translated_cores(&[CODE0, CODE1], 4_096, 64, "boot")
        .expect("the fixture's own two programs translate");
}

#[cfg(not(feature = "jit"))]
fn install(_machine: &mut Machine) {
    unreachable!("this binary has no translator; only `fixture(false)` is reachable");
}

/// Run to a cycle deadline, failing loudly on anything else.
fn run_for(m: &mut Machine, cycles: u64) {
    let stop = m.cycles() + cycles;
    let o = m.run_until(&StopCondition {
        stop_cycle: Some(stop),
        ..Default::default()
    });
    assert!(
        matches!(o, Outcome::Deadline { .. }),
        "the run stopped on {o:?}\n{:?}",
        m.core_report()
    );
}

fn word_at(m: &mut Machine, at: u32) -> i32 {
    m.bus_mut().read_word(at).expect("DRAM reads")
}

/// Every reading this test compares between the two legs.
#[derive(Debug, PartialEq, Eq)]
struct Readings {
    answer0: i32,
    answer1: i32,
    flag: i32,
    cycles: u64,
    core0: u64,
    core1: u64,
}

fn run_leg(translate: bool) -> (Readings, u64, Vec<String>) {
    let mut m = fixture(translate);
    run_for(&mut m, 600_000);
    let readings = Readings {
        answer0: word_at(&mut m, ANSWER0),
        answer1: word_at(&mut m, ANSWER1),
        flag: word_at(&mut m, FLAG),
        cycles: m.cycles(),
        core0: m.harts[0].instruction_count(),
        core1: m.harts[1].instruction_count(),
    };
    let reports = m.translated_core_reports();
    (readings, m.jit_retranslations(), reports)
}

/// The `N` out of a core's `core<i>: N entries, …` report line.
fn entries_in(report: &str) -> u64 {
    report
        .split_once(": ")
        .and_then(|(_, rest)| rest.split_once(" entries"))
        .and_then(|(n, _)| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("no entry count in {report}"))
}

/// The fixture's instructions decode back to what the test says they are —
/// M0's lesson: never trust an encoding by analogy.
#[test]
fn the_fixture_decodes_back_to_what_it_says_it_is() {
    for (pc, inst) in program0().into_iter().chain(program1()) {
        let bytes = lp_xt_inst::encode(&inst);
        let (decoded, len) = lp_xt_inst::decode(&bytes).expect("decodes");
        assert_eq!(decoded, inst, "at {pc:#010x}");
        assert_eq!(len, bytes.len(), "at {pc:#010x}");
    }
    let f = published_function();
    let (first, len) = lp_xt_inst::decode(&f).expect("the published function decodes");
    assert_eq!(first, Inst::MoviN(reg(2), PUBLISHED_ANSWER));
    assert_eq!(
        lp_xt_inst::decode(&f[len..])
            .expect("and its return")
            .0,
        Inst::NullaryN(NullaryNarrowOp::RetN)
    );
}

/// The fixture is honest with **no** translator at all, so a green below
/// cannot be a fixture that never published.
#[test]
fn the_interpreter_alone_publishes_and_calls_on_both_cores() {
    let (r, _, _) = run_leg(false);
    assert_eq!(r.flag, 1, "core 0 finished publishing: {r:?}");
    assert_eq!(r.answer0, PUBLISHED_ANSWER, "{r:?}");
    assert_eq!(r.answer1, PUBLISHED_ANSWER, "{r:?}");
}

/// **The gate**: code written from inside translated code runs, on both
/// cores, and the run is the interpreter's own.
#[cfg(feature = "jit")]
#[test]
fn code_published_from_inside_a_stay_runs_on_both_cores() {
    let (interp, _, _) = run_leg(false);
    let (jit, retranslations, reports) = run_leg(true);

    // Not vacuous: both harts really entered translated code, so a green here
    // is the translator agreeing with the interpreter and not a core that was
    // never installed.
    assert_eq!(reports.len(), 2, "a core on each hart: {reports:?}");
    for report in &reports {
        assert!(
            entries_in(report) > 0,
            "a hart never entered translated code: {report}"
        );
    }
    // The fixture's whole program lives in SRAM0, which the bus calls
    // writable, so the split gives it one module and no read-only half — and
    // that module is the one the event retires and replaces.
    assert!(
        reports[0].contains("0 writable"),
        "core 0's module is the writable one: {}",
        reports[0]
    );

    assert_eq!(
        jit.answer0, PUBLISHED_ANSWER,
        "core 0 called the function it had just written: {jit:?}"
    );
    assert_eq!(
        jit.answer1, PUBLISHED_ANSWER,
        "core 1 called the same published function: {jit:?}"
    );
    assert!(
        retranslations >= 1,
        "the publish-by-store event never ran: the stores into executable memory \
         did not reach the boundary"
    );
    // The invariant, cell by cell.
    assert_eq!(
        jit, interp,
        "the translated run left a different everything"
    );
}
