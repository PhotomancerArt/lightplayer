//! **The burst rule and the escape hatch across events** (M7 P07, XD10).
//!
//! Two things `jit_publish.rs` does not pin down:
//!
//! 1. **A burst is one event.** The product publishes its JIT region as
//!    ~1,300 word stores and the invalidation side-band fires on the *first*
//!    of them. Retiring is immediate — that is what keeps the run exact — but
//!    retranslating per store would pay cranelift 1,300 times for one
//!    publish. The rule is "at the first slice boundary that finds the
//!    published set non-empty and no store into it landed in the window just
//!    run", and this is that rule measured: 256 word stores, spread over
//!    several slices, must produce **one** retranslation.
//! 2. **An `isync` inside a stay is survived.** `isync` is a whole flush on
//!    the translated core, and translated code meets it through the escape
//!    hatch — the interpreter runs it, the core is asked to forget
//!    everything, and the hart's pending-invalidation drain
//!    (`XtHart::drain_core_flush`) applies that *after* the stay rather than
//!    under it. A core that got this wrong would either keep running blocks
//!    it had been told to forget (wrong) or never be entered again (slow and
//!    visible as zero entries).
//!
//! Both are judged the same way as everything else in this plan: the same
//! fixture is run with and without a translated core and every architectural
//! reading has to match.

use lp_emu_core::Bus;
use lp_emu_esp32v3::cache::CacheOffPolicy;
use lp_emu_esp32v3::machine::{
    BootFrame, BootMode, Esp32V3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32v3::memmap;
use lp_xt_inst::{BrZ, Inst, NullaryNarrowOp, NullaryOp, Reg, StoreOp};

/// The program's literals.
const LIT: u32 = 0x4008_9000;
/// The program.
const CODE: u32 = 0x4008_9040;
/// The region the program publishes into — executable SRAM0, as the JIT
/// region is.
const PUBLISHED: u32 = 0x4008_8800;
/// Where the program leaves its counter, so the fixture can be seen to have
/// run to the end.
const DONE: u32 = 0x3ffe_2000;
/// How many word stores the burst makes. 256 words is 1 KiB — one span in the
/// bus's ring, several slices of the machine's window.
const BURST: u32 = 256;
/// Turns of the warm-up loop, whose body carries an `isync`.
const WARMUP: u32 = 512;
/// Turns of the tail loop, after the burst: long enough that a core which
/// never came back from the event would show it.
const TAIL: u32 = 4_000;

const L_WARMUP: u32 = 0;
const L_PUBLISHED: u32 = 4;
const L_WORD: u32 = 8;
const L_BURST: u32 = 12;
const L_TAIL: u32 = 16;
const L_DONE: u32 = 20;

fn reg(n: u8) -> Reg {
    Reg::new(n)
}

fn l32r_field(pc: u32, label: u32) -> u16 {
    let base = (pc + 3) & !3;
    (((label as i64 - base as i64) >> 2) as i32 as u32 & 0xffff) as u16
}

fn next_pc(at: u32, insts: &[Inst]) -> u32 {
    at + insts
        .iter()
        .map(|i| lp_xt_inst::encode(i).len() as u32)
        .sum::<u32>()
}

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

/// The word the burst stores: `movi.n a2, 0x5a; ret.n`, which decodes, so the
/// third path's word seeds find a block at every one of them.
fn published_word() -> u32 {
    let mut bytes = Vec::new();
    bytes.extend(lp_xt_inst::encode(&Inst::MoviN(reg(2), 0x5a)));
    bytes.extend(lp_xt_inst::encode(&Inst::NullaryN(NullaryNarrowOp::RetN)));
    while bytes.len() < 4 {
        bytes.push(0);
    }
    u32::from_le_bytes(bytes[..4].try_into().expect("four bytes"))
}

/// A backward loop of `body`, counted down in `counter`.
fn counted_loop(at: u32, before: &[Inst], counter: Reg, body: &[Inst]) -> Vec<Inst> {
    let mut insts = before.to_vec();
    let top = next_pc(at, &insts);
    insts.extend_from_slice(body);
    insts.push(Inst::Addi(counter, counter, -1));
    let bnez_pc = next_pc(at, &insts);
    insts.push(Inst::BranchZ(
        BrZ::Bnez,
        counter,
        top as i32 - (bnez_pc as i32 + 4),
    ));
    insts
}

/// Warm up (with an `isync` in the loop body), burst 256 word stores into
/// `PUBLISHED`, run a long tail loop, record that it got there, park.
fn program() -> Vec<(u32, Inst)> {
    let (a3, a4, a5, a6) = (reg(3), reg(4), reg(5), reg(6));
    let mut insts: Vec<Inst> = Vec::new();

    // The warm-up loop: an `isync` every turn, from inside a stay.
    insts.push(Inst::L32r(a4, l32r_field(CODE, LIT + L_WARMUP)));
    insts = counted_loop(CODE, &insts, a4, &[Inst::Nullary(NullaryOp::Isync)]);

    // The burst: `a3 = &PUBLISHED`, `a5 = word`, `a6 = BURST`, then
    // `s32i a5, a3, 0 ; addi a3, a3, 4` counted down.
    insts.push(Inst::L32r(
        a3,
        l32r_field(next_pc(CODE, &insts), LIT + L_PUBLISHED),
    ));
    insts.push(Inst::L32r(
        a5,
        l32r_field(next_pc(CODE, &insts), LIT + L_WORD),
    ));
    insts.push(Inst::L32r(
        a6,
        l32r_field(next_pc(CODE, &insts), LIT + L_BURST),
    ));
    insts = counted_loop(
        CODE,
        &insts,
        a6,
        &[Inst::Store(StoreOp::S32i, a5, a3, 0), Inst::Addi(a3, a3, 4)],
    );
    insts.push(Inst::Nullary(NullaryOp::Isync));

    // The tail: entries here are entries made *after* the event.
    insts.push(Inst::L32r(
        a4,
        l32r_field(next_pc(CODE, &insts), LIT + L_TAIL),
    ));
    insts = counted_loop(CODE, &insts, a4, &[Inst::Addi(a5, a5, 0)]);

    insts.push(Inst::L32r(
        a3,
        l32r_field(next_pc(CODE, &insts), LIT + L_DONE),
    ));
    insts.push(Inst::MoviN(a5, 1));
    insts.push(Inst::Store(StoreOp::S32i, a5, a3, 0));
    insts.push(Inst::Nullary(NullaryOp::Memw));
    insts.push(Inst::J(-4));
    assemble(CODE, &insts)
}

/// A ROM-up machine holding the program, core 0 pointed at it.
///
/// `--cache-off-fetch permit` for the reason `jit_publish.rs` spells out: D4's
/// watch is a bus memory cost, a bus with one is not a pure fetch, and a
/// ROM-up machine never enables the flash cache — so without it every entry
/// is refused and the fixture proves nothing.
fn fixture(translate: bool) -> Machine {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .cache_off_fetch(CacheOffPolicy::Permit)
        .build()
        .expect("a machine with no app still has a bus and a ROM");
    {
        let bus = machine.bus_mut();
        let mut word = |at: u32, v: u32| {
            bus.load_image(at, &v.to_le_bytes())
                .expect("SRAM0 holds a literal");
        };
        word(LIT + L_WARMUP, WARMUP);
        word(LIT + L_PUBLISHED, PUBLISHED);
        word(LIT + L_WORD, published_word());
        word(LIT + L_BURST, BURST);
        word(LIT + L_TAIL, TAIL);
        word(LIT + L_DONE, DONE);
    }
    for (at, inst) in program() {
        machine
            .bus_mut()
            .load_image(at, &lp_xt_inst::encode(&inst))
            .expect("SRAM0 holds the code");
    }
    machine
        .seed_boot_state(CODE, BootFrame::at(memmap::ROM_PRO_STACK_TOP))
        .expect("seeded");
    if translate {
        install(&mut machine);
    }
    machine
}

#[cfg(feature = "jit")]
fn install(machine: &mut Machine) {
    machine
        .install_translated_cores(&[CODE], 4_096, 64, "boot")
        .expect("the fixture's own program translates");
}

#[cfg(not(feature = "jit"))]
fn install(_machine: &mut Machine) {
    unreachable!("this binary has no translator; only `fixture(false)` is reachable");
}

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

fn entries_in(report: &str) -> u64 {
    report
        .split_once(": ")
        .and_then(|(_, rest)| rest.split_once(" entries"))
        .and_then(|(n, _)| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("no entry count in {report}"))
}

/// `(done, cycles, instructions, retranslations, reports)`.
fn run_leg(translate: bool) -> (i32, u64, u64, u64, Vec<String>) {
    let mut m = fixture(translate);
    run_for(&mut m, 200_000);
    let done = m.bus_mut().read_word(DONE).expect("DRAM reads");
    (
        done,
        m.cycles(),
        m.harts[0].instruction_count(),
        m.jit_retranslations(),
        m.translated_core_reports(),
    )
}

#[test]
fn the_fixture_decodes_back_to_what_it_says_it_is() {
    for (pc, inst) in program() {
        let bytes = lp_xt_inst::encode(&inst);
        let (decoded, len) = lp_xt_inst::decode(&bytes).expect("decodes");
        assert_eq!(decoded, inst, "at {pc:#010x}");
        assert_eq!(len, bytes.len(), "at {pc:#010x}");
    }
    // The burst really does cross several of the machine's windows: 256 turns
    // of a four-instruction loop is over a thousand instructions.
    assert!(BURST * 4 > 256, "the burst is longer than one slice");
}

/// The interpreter alone runs the fixture to the end — so a green below is
/// the translator agreeing, not a fixture that never got started.
#[test]
fn the_interpreter_alone_runs_the_fixture_to_the_end() {
    let (done, _, _, retranslations, reports) = run_leg(false);
    assert_eq!(done, 1, "the program reached its last store");
    assert_eq!(retranslations, 0, "no core, no event");
    assert!(reports.is_empty(), "no core, no report: {reports:?}");
}

/// **The burst rule**: 256 word stores, one event.
#[cfg(feature = "jit")]
#[test]
fn a_burst_of_stores_is_one_translation_event() {
    let (done, cycles, instret, retranslations, reports) = run_leg(true);
    assert_eq!(done, 1, "the program reached its last store");
    assert_eq!(
        retranslations, 1,
        "{BURST} word stores must be one publish-by-store event, not one per \
         store: {reports:?}"
    );
    let (i_done, i_cycles, i_instret, ..) = run_leg(false);
    assert_eq!(
        (done, cycles, instret),
        (i_done, i_cycles, i_instret),
        "the translated run left a different (done, cycles, instructions)"
    );
}

/// **The escape hatch across events**: an `isync` from inside a stay is a
/// whole flush the hart applies after the stay, and the core comes back.
#[cfg(feature = "jit")]
#[test]
fn an_isync_from_inside_a_stay_does_not_end_the_core() {
    let (_, _, _, _, reports) = run_leg(true);
    assert_eq!(reports.len(), 1, "core 0 only: {reports:?}");
    let report = &reports[0];
    // `WARMUP` turns of the warm-up loop each run an `isync` while the module
    // is live. A core that answered a whole flush by giving up would show one
    // entry and then nothing; a core that ignored it would be wrong, and the
    // identity assertion above is what says it was not.
    assert!(
        entries_in(report) > u64::from(WARMUP),
        "the core kept being entered across {WARMUP} `isync`s: {report}"
    );
    assert!(
        report.contains("stale 0,"),
        "no module went stale over an `isync`: its bytes did not change: {report}"
    );
}
