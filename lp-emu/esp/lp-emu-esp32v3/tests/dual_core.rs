//! **Two cores run** — M4 P1's gates, each a test rather than an eyeball.
//!
//! Three kinds of test, in the order a reader should meet them:
//!
//! 1. **A hand-built fixture, no firmware.** Core 0 performs esp-hal's exact
//!    `start_core1` sequence on DPORT (boot address, clock gate, runstall,
//!    the `appcpu_resetting` pulse), spins, and rings the doorbell
//!    (`cpu_intr_from_cpu_1`, interrupt source 25). Core 1 comes up **at
//!    `appcpu_boot_addr` with no ROM code run**, routes the source into its
//!    own matrix, enables the line and parks in `waiti`. The seeded entry
//!    state is the proof of the model and `EPC2` is the proof the doorbell
//!    was taken. Built by hand for the phase file's reason: a test that
//!    depends on the firmware reaching a condition is not a test.
//! 2. **The bench's fact, on the fixture.** After the release, core 1 writes
//!    nothing into `0x3ffe0440..0x3ffe1320` — the canary L2 ran on the
//!    DOM-Z-102, run here against the same span
//!    (`the_released_core_writes_nothing_into_the_rom_pro_span`).
//! 3. **Ruling R4 on a synthetic table disagreement.** The stop, its text,
//!    and the `permit` escape.
//! 4. **The shipped image** (`#[ignore]`d, `just test-emu-esp32v3-boot`):
//!    `[INIT] RMT ISR on APP core`, the binds in core 1's matrix, the pusher
//!    parked in `waiti` on core 1, and core 1 executing no mask-ROM
//!    instruction on the way to the bind
//!    (`the_shipped_image_runs_no_rom_code_on_core_one` — the mechanism
//!    behind (2), where the span itself is live allocator memory and a scan
//!    of it would measure the firmware's allocations rather than core 1).
//!
//! # ⚠️ What changed under this branch, and why the fixture reads differently
//!
//! These tests were first written against a machine that released core 1 at
//! `_ResetVector` and let the vendored mask ROM's reset path run on it. Two
//! of them asserted exactly that: `the_doorbell_reaches_core_one` read the
//! ROM's own `appcpu_boot_addr` polls (`pc=0x40000462`, `pc=0x400076e0`) out
//! of the DPORT bus trace, and `the_release_resets_core_one` asserted core 1
//! started somewhere inside the mask ROM. **Those assertions were the
//! emulator's claim, and the bench refused it** — L2 (PR #695) found
//! `changed=0` across `0x3ffe0440..0x3ffe1440` on silicon, so nothing in the
//! ROM's reset path runs on core 1 when esp-hal starts it. They now assert
//! the opposite fact: the released core is at `appcpu_boot_addr` on its
//! first instruction, the trace holds **no** ROM read of `DPORT+0x038` by
//! hart 1, and the span is untouched. `machine.rs`'s "How core 1 starts"
//! carries the evidence and the hardware rationale still owed;
//! `docs/defects/2026-09-10-the-emulator-ran-the-rom-reset-path-on-the-app-core.md`
//! is the entry.

use lp_emu_core::Bus;
use lp_emu_esp32v3::cache::MmuDivergencePolicy;
use lp_emu_esp32v3::flash::FlashBacking;
use lp_emu_esp32v3::machine::{
    AppSource, BootFrame, BootMode, Esp32V3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32v3::periph::dport::{
    APPCPU_CTRL_A, APPCPU_CTRL_B, APPCPU_CTRL_C, APPCPU_CTRL_D, CORE_0_INTR_MAP, CORE_1_INTR_MAP,
    CPU_INTR_FROM_CPU0,
};
use lp_emu_esp32v3::test_support::{fw_esp32v3_image, merged_chip_image, skip_notice};
use lp_emu_esp32v3::{intmatrix, memmap};
use lp_xt_inst::{BrZ, Inst, NullaryOp, Reg, SpecialReg, SrOp, StoreOp};

// ---------------------------------------------------------------------------
// The fixture
// ---------------------------------------------------------------------------

/// Core 0's literals: `&DPORT`, `CODE1`, the delay count.
const LIT0: u32 = 0x4008_9000;
/// Core 0's program.
const CODE0: u32 = 0x4008_9020;
/// Core 1's literals: `&DPORT`, the `INTENABLE` mask.
const LIT1: u32 = 0x4008_9100;
/// Core 1's program — what core 0 writes into `appcpu_boot_addr`, and
/// therefore the very first instruction core 1 executes after the release.
const CODE1: u32 = 0x4008_9120;
/// The CPU interrupt core 1 routes the doorbell to: 19 is a **level-2**
/// external line (`CORE_INTERRUPTS`), so the take lands in
/// `_Level2InterruptVector` with `EPC2` naming the instruction after the
/// `waiti` — unambiguous, where a level-1 line would share the user vector
/// with every exception.
const DOORBELL_CPU_INT: u32 = 19;
/// `FROM_CPU_INTR1`, the wire pusher's doorbell.
const DOORBELL_SOURCE: u32 = intmatrix::FROM_CPU_INTR0 as u32 + 1;
/// How many turns of its two-instruction loop core 0 spins before ringing:
/// ~120k cycles, which is far longer than core 1 needs to reach its `waiti`
/// (eight instructions — there is no ROM path in front of it) and is kept at
/// the pre-bench value so the gate still proves a *parked* core costs
/// nothing over a long wait.
const DELAY: u32 = 60_000;
/// `cpu_intr_from_cpu_1`.
const RING: u32 = CPU_INTR_FROM_CPU0 + 4;

fn reg(n: u8) -> Reg {
    Reg::new(n)
}

/// `l32r`'s raw 16-bit field: `(label - ((pc + 3) & !3)) / 4`, backward only.
fn l32r_field(pc: u32, label: u32) -> u16 {
    let base = (pc + 3) & !3;
    (((label as i64 - base as i64) >> 2) as i32 as u32 & 0xffff) as u16
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

/// Core 0: esp-hal's `start_core1` on DPORT, a delay, the doorbell, a self
/// jump. `s32i a3, a2, off` is the PAC's `write` on each `appcpu_ctrl_*`.
fn program0() -> Vec<(u32, Inst)> {
    let a2 = reg(2);
    let a3 = reg(3);
    let a4 = reg(4);
    let store = |off: u32| Inst::Store(StoreOp::S32i, a3, a2, off);
    let mut insts = vec![
        Inst::L32r(a2, l32r_field(CODE0, LIT0)),
        Inst::L32r(a3, l32r_field(CODE0 + 3, LIT0 + 4)),
        store(APPCPU_CTRL_D),
        Inst::MoviN(a3, 1),
        store(APPCPU_CTRL_B),
        Inst::MoviN(a3, 0),
        store(APPCPU_CTRL_C),
        Inst::MoviN(a3, 1),
        store(APPCPU_CTRL_A),
        Inst::MoviN(a3, 0),
        store(APPCPU_CTRL_A),
    ];
    // `l32r a4, DELAY` — its field depends on where it lands.
    let so_far: u32 = insts
        .iter()
        .map(|i| lp_xt_inst::encode(i).len() as u32)
        .sum();
    insts.push(Inst::L32r(a4, l32r_field(CODE0 + so_far, LIT0 + 8)));
    // delay: addi a4, a4, -1 ; bnez a4, delay   (bnez target = pc + 4 + off)
    let delay_pc: u32 = CODE0
        + insts
            .iter()
            .map(|i| lp_xt_inst::encode(i).len() as u32)
            .sum::<u32>();
    insts.push(Inst::Addi(a4, a4, -1));
    let bnez_pc = delay_pc + 3;
    insts.push(Inst::BranchZ(
        BrZ::Bnez,
        a4,
        delay_pc as i32 - (bnez_pc as i32 + 4),
    ));
    insts.push(Inst::MoviN(a3, 1));
    insts.push(store(RING));
    insts.push(Inst::Nullary(NullaryOp::Memw));
    insts.push(Inst::J(-4));
    assemble(CODE0, &insts)
}

/// Core 1: route source 25 into **its own** matrix, enable the line, park.
fn program1() -> Vec<(u32, Inst)> {
    let a2 = reg(2);
    let a3 = reg(3);
    let a4 = reg(4);
    let insts = vec![
        Inst::L32r(a3, l32r_field(CODE1, LIT1)),
        Inst::MoviN(a4, DOORBELL_CPU_INT as i32),
        Inst::Store(StoreOp::S32i, a4, a3, CORE_1_INTR_MAP + 4 * DOORBELL_SOURCE),
        Inst::L32r(a2, l32r_field(CODE1 + 8, LIT1 + 4)),
        Inst::Sr(SrOp::Wsr, SpecialReg::Intenable, a2),
        Inst::Nullary(NullaryOp::Rsync),
        Inst::Waiti(0),
        Inst::J(-4),
    ];
    assemble(CODE1, &insts)
}

/// The pc of core 1's `j .` — the instruction after its `waiti`, which is
/// what `EPC2` must hold once the doorbell is taken.
fn after_waiti() -> u32 {
    program1()
        .iter()
        .find(|(_, i)| matches!(i, Inst::J(_)))
        .expect("the self jump")
        .0
}

/// The pc of core 0's `j .`.
fn core0_parked_pc() -> u32 {
    program0()
        .iter()
        .find(|(_, i)| matches!(i, Inst::J(_)))
        .expect("the self jump")
        .0
}

/// A trace sink a test can read back.
#[derive(Clone, Default)]
struct SharedSink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for SharedSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedSink {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// A strict ROM-up machine with the boot set, both programs in SRAM0, core
/// 0 pointed at its program with the ROM's own stack, DPORT traced.
fn fixture() -> (Machine, SharedSink) {
    let sink = SharedSink::default();
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .strict(true)
        .trace(Box::new(sink.clone()), vec!["DPORT".to_string()])
        .build()
        .expect("a machine with no app still has a bus and a ROM");

    let bus = machine.bus_mut();
    let word = |bus: &mut lp_emu_esp_common::SocBus, at: u32, v: u32| {
        bus.load_image(at, &v.to_le_bytes())
            .expect("SRAM0 holds a literal");
    };
    word(bus, LIT0, memmap::periph::DPORT);
    word(bus, LIT0 + 4, CODE1);
    word(bus, LIT0 + 8, DELAY);
    word(bus, LIT1, memmap::periph::DPORT);
    word(bus, LIT1 + 4, 1 << DOORBELL_CPU_INT);
    for (at, inst) in program0().into_iter().chain(program1()) {
        bus.load_image(at, &lp_xt_inst::encode(&inst))
            .expect("SRAM0 holds the code");
    }
    machine
        .seed_boot_state(CODE0, BootFrame::at(memmap::ROM_PRO_STACK_TOP))
        .expect("seeded");
    (machine, sink)
}

/// Run in 10 µs steps until `done`, or fail with the core report.
fn run_until_state(m: &mut Machine, what: &str, done: impl Fn(&Machine) -> bool) {
    let mut guard = 0;
    while !done(m) {
        let next = m.cycles() + 2_400;
        let o = m.run_until(&StopCondition {
            stop_cycle: Some(next),
            ..Default::default()
        });
        assert!(
            matches!(o, Outcome::Deadline { .. }),
            "{what}: the run stopped on {o:?}\n{:?}",
            m.core_report()
        );
        guard += 1;
        assert!(
            guard < 1_000,
            "{what}: never happened\n{:?}",
            m.core_report()
        );
    }
}

/// The fixture's instructions decode back to what the test says they are —
/// the bytes come from the repo's encoder, and M0 found a real misdecode
/// that came from trusting an encoding by analogy.
#[test]
fn the_fixture_decodes_back_to_what_it_says_it_is() {
    for (pc, inst) in program0().into_iter().chain(program1()) {
        let bytes = lp_xt_inst::encode(&inst);
        let (decoded, len) = lp_xt_inst::decode(&bytes).expect("decodes");
        assert_eq!(decoded, inst, "at {pc:#010x}");
        assert_eq!(len, bytes.len(), "at {pc:#010x}");
    }
}

/// **Gate 4: the doorbell reaches core one.** And gates 3 and 6 on the
/// fixture: core 1 parks in `waiti` and costs nothing while parked, and it
/// got there through the ROM's own wait loop.
#[test]
fn the_doorbell_reaches_core_one() {
    let (mut m, trace) = fixture();
    assert!(m.core_stalled(1), "held until DPORT releases it");

    // Core 0's release sequence runs in its first window; core 1 then starts
    // at CODE1 — the address core 0 wrote into `appcpu_boot_addr` — with no
    // ROM code between the release and its first instruction.
    run_until_state(&mut m, "core 1 released", |m| !m.core_stalled(1));
    let released_at = m.cycles();
    assert!(
        m.harts[1].instruction_count() <= 2_400,
        "a released core counts from zero: at most one 10 µs step's worth so far, not {}",
        m.harts[1].instruction_count()
    );
    assert!(
        (CODE1..CODE1 + 0x40).contains(&m.harts[1].pc()),
        "and starts at appcpu_boot_addr, not in the mask ROM: {:#010x}",
        m.harts[1].pc()
    );

    // Gate 3 on the fixture: core 1 reaches its `waiti` and parks.
    run_until_state(&mut m, "core 1 parked", |m| m.parked(1));
    let parked_at = m.cycles();
    assert_eq!(m.harts[1].pc(), after_waiti(), "pc is past the waiti");
    assert!(m.wfi_ends(1) >= 1, "the window ended in Wfi");
    assert!(
        m.core_report()[1].contains("parked(waiti)"),
        "{:?}",
        m.core_report()
    );
    assert!(
        m.harts[0].cpu().a(4) > 0,
        "core 0 is still in its delay: the doorbell has not rung"
    );
    let core1_instructions = m.core_instructions(1);
    assert!(
        core1_instructions <= program1().len() as u64 + 4,
        "CODE1 is all core 1 ran — no ROM path in front of it, so at most its own {} \
         instructions (plus the waiti's turns), not {core1_instructions}",
        program1().len()
    );
    println!(
        "core 1: released at {released_at}, parked at {parked_at} after {core1_instructions} \
         instructions (CODE1 only, no ROM)"
    );

    // Gate 6 on the fixture, inverted by the bench (see the module docs): the
    // ROM's own `appcpu_boot_addr` polls did **not** run. The bus trace names
    // the pc of every DPORT access; neither the reset handler's
    // app-fast-boot test at 0x40000462 nor `main+0x1c`'s `l32i` at
    // 0x400076e0 appears, and no DPORT access at all comes from mask-ROM
    // addresses on this fixture.
    let t = trace.text();
    let reads = |pc: &str| {
        t.lines()
            .filter(|l| l.contains(pc) && l.contains("DPORT+0x038"))
            .count()
    };
    assert_eq!(
        reads("pc=0x40000462"),
        0,
        "the reset handler's fast-boot test never ran — no ROM code on core 1:\n{t}"
    );
    assert_eq!(
        reads("pc=0x400076e0"),
        0,
        "ROM main's APP-core wait loop never ran:\n{t}"
    );

    // A parked core costs nothing: another 100 µs of core 0's delay retires
    // nothing on core 1.
    m.run_until(&StopCondition {
        stop_cycle: Some(m.cycles() + 24_000),
        ..Default::default()
    });
    assert!(m.harts[0].cpu().a(4) > 0, "core 0 is still delaying");
    assert!(m.parked(1), "still parked");
    assert_eq!(
        m.core_instructions(1),
        core1_instructions,
        "a parked core retires nothing"
    );

    // The ring: core 0's `s32i` to cpu_intr_from_cpu_1 raises source 25,
    // core 1's own map routes it to CPU interrupt 19, and the feed between
    // windows takes it. The fixture installs no level-2 handler, so what
    // runs next is the ROM's own vector — `_Level2FromVector`, the
    // unhandled-interrupt `break`, and `_DebugExceptionVector`, whose
    // `simcall` this hart refuses as an unsupported instruction (DD16). That
    // refusal is a `Fault` on core 1 within the same window as the take,
    // and it is the fixture's expected end: everything the gate needs is in
    // the registers by then.
    let mut guard = 0;
    let end = loop {
        let next = m.cycles() + 2_400;
        let o = m.run_until(&StopCondition {
            stop_cycle: Some(next),
            ..Default::default()
        });
        if !m.parked(1) || !matches!(o, Outcome::Deadline { .. }) {
            break o;
        }
        guard += 1;
        assert!(
            guard < 1_000,
            "the doorbell never rang: {:?}",
            m.core_report()
        );
    };
    assert!(
        m.bus().irq.level(DOORBELL_SOURCE as u16),
        "core 0 rang swi1 (DPORT.cpu_intr_from_cpu_1): {end:?}"
    );
    assert!(!m.parked(1), "the doorbell un-parked core 1: {end:?}");
    assert_eq!(
        m.harts[1].sr().epc[2],
        after_waiti(),
        "EPC2 names the instruction after the waiti: a level-2 interrupt was taken ({end:?})"
    );
    match &end {
        Outcome::Deadline { .. } => {}
        Outcome::Fault { core: 1, pc, .. } => {
            let sym = m.symbolize(*pc).unwrap_or_default();
            assert!(
                sym.contains("_DebugExceptionVector"),
                "the ROM's unhandled-interrupt path ends in the debug vector's simcall, not {sym} \
                 at {pc:#010x}"
            );
        }
        other => panic!("the fixture ended on {other:?}"),
    }
    m.bus_mut().set_hart(1);
    assert_ne!(
        m.bus().pending_cpu_interrupt_mask() & (1 << DOORBELL_CPU_INT),
        0,
        "the doorbell is asserted at core 1's input"
    );
    m.bus_mut().set_hart(0);
    assert_eq!(
        m.bus().pending_cpu_interrupt_mask() & (1 << DOORBELL_CPU_INT),
        0,
        "and not at core 0's: core 0's map for source 25 is untouched"
    );
    assert_eq!(
        m.harts[0].pc(),
        core0_parked_pc(),
        "core 0 parked itself after ringing"
    );
    assert!(
        m.core_instructions(1) > core1_instructions,
        "core 1 ran again after the take"
    );
}

/// The release is a **reset plus a seeded entry state**, and this test's
/// meaning changed with the model (see the module docs).
///
/// It used to assert that slot 1 came back as a bare fresh hart pointed at
/// `_ResetVector`, on the reading that the `appcpu_resetting` pulse put the
/// APP core through the mask ROM. The bench refused that reading, so what it
/// asserts now is the *other* half: the slot is still wiped — nothing of its
/// previous state survives — and then exactly the five fields
/// `Machine::service_app_core_start` documents are seeded, the boot address
/// among them.
#[test]
fn the_release_resets_core_one_and_seeds_its_entry_state() {
    let (mut m, _trace) = fixture();
    // Scribble on slot 1 before the release; none of it may survive.
    m.harts[1].set_pc(0xdead_beee);
    m.harts[1].cpu_mut().set_a(5, 0x1234_5678);
    m.harts[1].cpu_mut().set_a(1, 0xbeef_0000);
    // Four cycles a step, so the run returns as near the release as the loop
    // can get: the release is serviced at the boundary of core 0's store and
    // core 1's own window opens inside the same iteration, so core 1 has
    // always retired an instruction or two by the time anything can look.
    let mut guard = 0;
    while m.core_stalled(1) {
        m.run_until(&StopCondition {
            stop_cycle: Some(m.cycles() + 4),
            ..Default::default()
        });
        guard += 1;
        assert!(guard < 200_000, "core 1 was never released");
    }

    // What pins "it started at `appcpu_boot_addr`" is the pair: the pc is
    // inside CODE1's eight instructions, and the retired count is far too
    // small for anything to have run in front of them — the ROM's own reset
    // path to a `callx8` is a few thousand instructions.
    assert!(
        (CODE1..=after_waiti()).contains(&m.harts[1].pc()),
        "the released core is executing CODE1, not the mask ROM: {:#010x}",
        m.harts[1].pc()
    );
    assert!(
        m.harts[1].instruction_count() <= program1().len() as u64,
        "and it got there with no ROM path in front of it: {} instructions retired",
        m.harts[1].instruction_count()
    );
    assert_eq!(
        m.harts[1].cpu().a(5),
        0,
        "the slot is still wiped: the scribble is gone"
    );
    assert_eq!(
        m.harts[1].ps(),
        lp_emu_esp32v3::machine::APP_CORE_RELEASE_PS,
        "PS is the ROM's post-_start word plus the CALLINC(2) of the call that reaches the entry"
    );
    let frame = m.app_core_boot_frame();
    assert_eq!(
        frame.sp,
        memmap::ROM_APP_STACK_TOP,
        "the ROM ELF's __stack_app and memory.x's reserved_rom_stack_app end agree"
    );
    assert_eq!(
        m.harts[1].cpu().a(1),
        frame.sp,
        "a1 is the ROM's APP-core stack top"
    );
    assert_eq!(
        m.app_core_frame(),
        Some(frame),
        "and the machine records the frame it released with"
    );
    // The save area under it, so a window overflow through the outermost
    // frame spills into the ROM's APP stack instead of through a null.
    for (i, word) in frame.save_area.iter().enumerate() {
        let at = frame.sp - 16 + 4 * i as u32;
        assert_eq!(
            m.peek_word(at).expect("mapped"),
            *word,
            "the boot frame's save area at {at:#010x}"
        );
    }
    assert_eq!(
        m.harts[1].cpu().cpenable,
        lp_emu_esp32v3::machine::CPENABLE_RESET,
        "CPENABLE at the part's reset value on the released core"
    );
    assert_eq!(
        m.harts[1].sr().vecbase,
        memmap::ROM_MASK_BASE,
        "VECBASE at the part's reset value; the firmware's entry sets its own"
    );
    assert!(
        m.harts[1].cycle_count() >= m.harts[0].cycle_count().saturating_sub(512),
        "its counters start at the machine's clock"
    );
}

/// **The bench's fact, pinned on the fixture: the released core writes
/// nothing into the ROM's PRO-stack span.**
///
/// L2 (PR #695) put a 4,096 B canary of `0xA5` at `0x3ffe0440` on the
/// DOM-Z-102, called the product's `start_app_core_isr`, waited for the bind
/// and scanned: `changed=0 ranges=0 handler_words=0 data_xtos_pro=0
/// bss_xtos_pro=0 outside=0`, `verdict=B`, twice. This is that canary
/// against this machine, on the fixture rather than the firmware so it does
/// not depend on the firmware reaching a condition: fill
/// `0x3ffe0440..0x3ffe1320` — the exact span the ROM's `flag = 1` unpack and
/// bss entries would rewrite — release core 1, run it to its park, and
/// require every byte back.
///
/// Core 0's fixture program never touches the span (its literals and code
/// are in SRAM0 and its stack is the PRO ROM stack, which starts 8 KiB
/// above), so any byte that moved is core 1's.
///
/// Under the model this branch replaced, this test failed with 3,800 changed
/// bytes in two ranges.
#[test]
fn the_released_core_writes_nothing_into_the_rom_pro_span() {
    const CANARY: u8 = 0xA5;
    // `.data_xtos_pro` + `.bss_xtos_pro`: what the ROM's tables rewrite, and
    // what `fw-esp32v3` hands its allocator as heap region 0.
    let span = memmap::ROM_PRO_STACK_BASE - 0xEE0..memmap::ROM_PRO_STACK_BASE;
    assert_eq!(
        (span.start, span.end),
        (0x3ffe_0440, 0x3ffe_1320),
        "the span L2 scanned on the bench"
    );

    let (mut m, _trace) = fixture();
    let filled = vec![CANARY; (span.end - span.start) as usize];
    m.bus_mut()
        .load_image(span.start, &filled)
        .expect("the ROM's PRO stack span is mapped RAM");

    run_until_state(&mut m, "core 1 released", |m| !m.core_stalled(1));
    run_until_state(&mut m, "core 1 parked", |m| m.parked(1));
    // …and a long while after it, covering everything a ROM reset path would
    // have had time to do.
    m.run_until(&StopCondition {
        stop_cycle: Some(m.cycles() + 240_000),
        ..Default::default()
    });

    let want = u32::from_le_bytes([CANARY; 4]);
    let mut changed: Vec<(u32, u32)> = Vec::new();
    for at in (span.start..span.end).step_by(4) {
        let word = m.peek_word(at).expect("mapped");
        if word != want {
            changed.push((at, word));
        }
    }
    assert!(
        changed.is_empty(),
        "the bench says changed=0 over {:#010x}..{:#010x}; this machine changed {} bytes, \
         first word at {:#010x} (={:#010x})",
        span.start,
        span.end,
        changed.len(),
        changed[0].0,
        changed[0].1,
    );
    println!(
        "[APPCORE-CANARY/emu] scan changed=0 over {:#010x}..{:#010x} ({} B) — verdict=B",
        span.start,
        span.end,
        span.end - span.start
    );
}

// ---------------------------------------------------------------------------
// Ruling R4
// ---------------------------------------------------------------------------

fn mmu_fixture(policy: MmuDivergencePolicy) -> Machine {
    let m = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .strict(true)
        .app_mmu_divergence(policy)
        .build()
        .expect("builds");
    {
        let mut c = m.cache().lock().expect("cache");
        // Entry 77 is 0x400d0000, the first IROM page. Two different flash
        // pages behind one window.
        c.mmu.set_entry(0, 77, 5);
        c.mmu.set_entry(1, 77, 6);
    }
    m
}

/// **Gate 7: R4's stop fires on a synthetic table disagreement**, names both
/// entries and the page, and exits 7.
#[test]
fn a_flash_mmu_table_disagreement_is_a_strict_stop() {
    let mut m = mmu_fixture(MmuDivergencePolicy::Stop);
    let outcome = m.run_until(&StopCondition::after_micros(10));
    let Outcome::MmuDivergence { cycle, divergence } = outcome else {
        panic!("expected the R4 stop, got {outcome:?}");
    };
    assert_eq!(divergence.index, 77);
    assert_eq!(divergence.pro, 5);
    assert_eq!(divergence.app, 6);
    assert_eq!(divergence.vaddr, Some(0x400d_0000));
    assert_eq!(
        outcome_code(&Outcome::MmuDivergence { cycle, divergence }),
        7
    );
    let text = m.mmu_divergence_message(cycle, &divergence);
    println!("{text}");
    for needle in [
        "FLASH-MMU DIVERGENCE",
        "entry=77",
        "0x400d0000",
        "the APP core's table maps flash page 0x6",
        "the PRO core's maps 0x5",
        "--app-mmu-divergence permit",
    ] {
        assert!(text.contains(needle), "the stop names `{needle}`:\n{text}");
    }
}

/// …and `permit` continues, serving the PRO core's view.
#[test]
fn app_mmu_divergence_permit_continues() {
    let mut m = mmu_fixture(MmuDivergencePolicy::Permit);
    let outcome = m.run_until(&StopCondition::after_micros(10));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "permit runs on to the deadline: {outcome:?}"
    );
}

fn outcome_code(o: &Outcome) -> i32 {
    o.exit_code()
}

// ---------------------------------------------------------------------------
// The shipped image
// ---------------------------------------------------------------------------

/// The P8 gate script: one `stopAllProjects` a millisecond after the io_task
/// is up (`tests/boot_idle.rs` says why that request).
fn stop_all_script() -> lp_emu_esp_common::ScriptedSource {
    lp_emu_esp_common::ScriptedSource::new().after(
        "[INIT] I/O task spawned",
        memmap::CYCLES_PER_US * 1_000,
        b"M!{\"id\":1,\"msg\":\"stopAllProjects\"}\n",
    )
}

const REPLY_LINE: &str = "\"id\":1,\"msg\":\"stopAllProjects\"";
const DUAL_CORE_LINE: &str = "[INIT] RMT ISR on APP core";
const FALLBACK_LINE: &str = "[INIT] APP core unavailable; RMT ISR on PRO core";

/// The shipped image on the merged chip, under `--strict-bus`, to the
/// heartbeat reply — with core 1 running. `None` when there is no image.
fn dual_core_run(test: &str, mode: BootMode, micros: u64) -> Option<Machine> {
    let elf = match fw_esp32v3_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(test, &reason);
            return None;
        }
    };
    let merged = match merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(test, &reason);
            return None;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let mut m = Esp32V3Builder::new()
        .boot_mode(mode)
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
        .flash_len(len)
        .strict(true)
        .uart0_script(stop_all_script())
        .build()
        .expect("builds");
    let outcome = m.run_until(&StopCondition {
        exit_on: Some(REPLY_LINE.to_string()),
        ..StopCondition::after_micros(micros)
    });
    let text = m.uart0().text();
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{mode:?}: the boot did not reach the heartbeat reply with core 1 running: {outcome:?}\n\
         {:#?}\n\
         If this is a strict-bus read at 0x00000008 from `LpFs::read_file` ~30k cycles after\n\
         `core 1: released by DPORT`, the APP core is running the mask ROM's reset path over\n\
         the firmware's heap region 0 — the FIXED emulator defect\n\
         docs/defects/2026-09-10-the-emulator-ran-the-rom-reset-path-on-the-app-core.md,\n\
         which would mean this branch's release model has regressed. The firmware is not at\n\
         fault: silicon rewrites nothing there (L2, PR #695). Console so far:\n{text}",
        m.core_report()
    );
    assert!(
        m.first_strict_violation().is_none(),
        "{mode:?}: no strict refusal anywhere in the boot"
    );
    assert_eq!(
        m.bus().unmapped_reads() + m.bus().unmapped_writes(),
        0,
        "{mode:?}: zero unmapped accesses"
    );
    Some(m)
}

/// **Gate 1: the firmware reports the dual-core deployment** — the line
/// silicon prints, not the Q5 fallback. This is the phase.
#[test]
#[ignore = "needs the shipped image and espflash; `just test-emu-esp32v3-boot`"]
fn the_firmware_reports_the_dual_core_deployment() {
    let Some(m) = dual_core_run(
        "the_firmware_reports_the_dual_core_deployment",
        BootMode::Direct,
        2_000_000,
    ) else {
        return;
    };
    let text = m.uart0().text();
    assert!(text.contains(DUAL_CORE_LINE), "{DUAL_CORE_LINE}:\n{text}");
    assert!(!text.contains(FALLBACK_LINE), "not the fallback:\n{text}");
    println!(
        "run: cycles={} instructions={} (core0={} core1={}) idle={}",
        m.cycles(),
        m.instructions(),
        m.core_instructions(0),
        m.core_instructions(1),
        m.idle_skips()
    );
}

/// **The shipped image runs no ROM code on core 1** — the mechanism behind
/// the bench's `changed=0`, asserted where it can be attributed.
///
/// `the_released_core_writes_nothing_into_the_rom_pro_span` is the memory
/// half, and it is on the fixture for a reason that this test had to learn
/// the hard way: on the shipped image `0x3ffe0440..0x3ffe1320` is **live
/// allocator memory** — it is heap region 0 — and the PRO core writes there
/// throughout the boot. A scan of that span across the release-to-bind
/// window measures the firmware's own allocations, not core 1: it reports 8
/// changed words 6,000 cycles after the release and 192 by the bind, with
/// core 1 doing exactly none of it. So the span is pinned on the fixture,
/// where core 0 provably never touches it, and what is pinned *here* is the
/// cause rather than the effect.
///
/// Two claims, both attributable to core 1 alone:
///
/// 1. **The address DPORT holds is the firmware's, not the mask ROM's**, and
///    the machine released core 1 with the ROM's APP-core stack top under it.
/// 2. **Core 1 never executes in the mask ROM**, sampled from the release the
///    whole way to `[INIT] RMT ISR on APP core`.
///
/// Claim 2 is a sample, and says so: core 1's pc is read once per short run
/// window rather than per instruction, and the first read is a few hundred
/// instructions into its life rather than at its first (the release is
/// serviced inside a window, not at its edge — the run prints the count).
/// It is not a weak claim: under the model this branch replaced core 1 spent
/// its first several *thousand* instructions inside `_ResetVector` and the
/// ROM's unpack tables, so every one of the first hundreds of samples would
/// land in the ROM, the earliest included.
#[test]
#[ignore = "needs the shipped image and espflash; `just test-emu-esp32v3-boot`"]
fn the_shipped_image_runs_no_rom_code_on_core_one() {
    let test = "the_shipped_image_runs_no_rom_code_on_core_one";
    let elf = match fw_esp32v3_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(test, &reason);
            return;
        }
    };
    let merged = match merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(test, &reason);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let mut m = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
        .flash_len(len)
        .strict(true)
        .build()
        .expect("builds");

    let rom = memmap::ROM_MASK_BASE..memmap::ROM_MASK_BASE + memmap::ROM_MASK_LEN;

    // Up to the release, in short steps, so the look below is as near the
    // release as the run loop can get.
    let mut guard = 0;
    while m.core_stalled(1) {
        let o = m.run_until(&StopCondition {
            stop_cycle: Some(m.cycles() + 2_400),
            ..Default::default()
        });
        assert!(
            matches!(o, Outcome::Deadline { .. }),
            "the boot stopped before releasing core 1: {o:?}\n{}",
            m.uart0().text()
        );
        guard += 1;
        assert!(
            guard < 20_000,
            "core 1 was never released:\n{}",
            m.uart0().text()
        );
    }
    let released_at = m.cycles();

    // (1) The boot address the guest wrote, read back through DPORT, and the
    // frame the machine released with.
    let entry = m
        .peek_word(memmap::periph::DPORT + APPCPU_CTRL_D)
        .expect("DPORT is mapped");
    assert!(
        !rom.contains(&entry),
        "esp-hal's boot address is in the firmware's image, not the mask ROM: {entry:#010x}"
    );
    let retired_at_release = m.core_instructions(1);
    assert_eq!(
        m.app_core_frame().map(|f| f.sp),
        Some(memmap::ROM_APP_STACK_TOP),
        "the machine released core 1 with the ROM's APP-core stack top"
    );

    // (2) …and it never enters the ROM on the way to the bind.
    let mut samples = 0u64;
    loop {
        assert!(
            !rom.contains(&m.harts[1].pc()),
            "core 1 is executing mask-ROM code at {:#010x} — the release model has regressed \
             to the ROM reset path (sample {samples}, cycle {})",
            m.harts[1].pc(),
            m.cycles()
        );
        samples += 1;
        let o = m.run_until(&StopCondition {
            exit_on: Some(DUAL_CORE_LINE.to_string()),
            stop_cycle: Some(m.cycles() + 2_400),
            ..StopCondition::after_micros(2_000_000)
        });
        match o {
            Outcome::ExitMatched { .. } => break,
            Outcome::Deadline { .. } => {}
            other => panic!(
                "the bind line did not reach the wire: {other:?}\n{}",
                m.uart0().text()
            ),
        }
        assert!(
            samples < 20_000,
            "the bind line never reached the wire:\n{}",
            m.uart0().text()
        );
    }
    println!(
        "[APPCORE-CANARY/emu] shipped image: core 1 started at appcpu_boot_addr {entry:#010x} \
         at cycle {released_at} ({retired_at_release} retired by the first sample), and \
         executed no mask-ROM instruction in \
         {samples} samples to `{DUAL_CORE_LINE}` ({} instructions retired on core 1) \
         — verdict=B",
        m.core_instructions(1)
    );
}

/// The same from the reset vector, through the real ROM and the real
/// bootloader — core 1 held by the machine until DPORT releases it, then
/// started at `appcpu_boot_addr` exactly as on the direct load.
#[test]
#[ignore = "needs espflash; `just test-emu-esp32v3-boot`"]
fn the_rom_up_path_reports_the_dual_core_deployment() {
    let Some(m) = dual_core_run(
        "the_rom_up_path_reports_the_dual_core_deployment",
        BootMode::RomUp,
        3_000_000,
    ) else {
        return;
    };
    let text = m.uart0().text();
    assert!(text.contains(DUAL_CORE_LINE), "{DUAL_CORE_LINE}:\n{text}");
    assert!(!text.contains(FALLBACK_LINE), "not the fallback:\n{text}");
}

/// **Gate 2: core one binds in its own matrix.** `core_1_intr_map[RMT]` and
/// `core_1_intr_map[FROM_CPU_INTR1]` route while `core_0_intr_map[RMT]`
/// reads esp-hal's DISABLED line, 16 — `install_isr`'s
/// `interrupt::disable(Cpu::ProCpu, Interrupt::RMT)`. Read back through
/// DPORT as the guest would.
#[test]
#[ignore = "needs the shipped image and espflash; `just test-emu-esp32v3-boot`"]
fn core_one_binds_in_its_own_matrix() {
    let Some(mut m) = dual_core_run(
        "core_one_binds_in_its_own_matrix",
        BootMode::Direct,
        2_000_000,
    ) else {
        return;
    };
    const RMT: u32 = 47;
    let map = |m: &mut Machine, base: u32, source: u32| -> u32 {
        m.bus_mut().set_hart(0);
        Bus::read_word(m.bus_mut(), memmap::periph::DPORT + base + 4 * source).expect("mapped")
            as u32
    };
    let rmt1 = map(&mut m, CORE_1_INTR_MAP, RMT);
    let bell1 = map(&mut m, CORE_1_INTR_MAP, DOORBELL_SOURCE);
    let rmt0 = map(&mut m, CORE_0_INTR_MAP, RMT);
    println!(
        "core_1_intr_map[RMT]={rmt1} core_1_intr_map[swi1]={bell1} core_0_intr_map[RMT]={rmt0}"
    );
    assert!(rmt1 < 32, "the RMT ISR is bound on core 1");
    assert!(bell1 < 32, "the doorbell is bound on core 1");
    assert_eq!(
        rmt0,
        intmatrix::ESP_HAL_DISABLED_CPU_INTERRUPT,
        "and disabled on the PRO core"
    );
}

/// **Gate 3: the pusher parks in `waiti` on core one.** At least one window
/// ended in `Wfi` on hart 1, and at the reply the pc at the park symbolizes
/// inside `wire_pusher::idle_once`.
#[test]
#[ignore = "needs the shipped image and espflash; `just test-emu-esp32v3-boot`"]
fn the_pusher_parks_in_waiti_on_core_one() {
    let Some(m) = dual_core_run(
        "the_pusher_parks_in_waiti_on_core_one",
        BootMode::Direct,
        2_000_000,
    ) else {
        return;
    };
    assert!(m.wfi_ends(1) >= 1, "core 1 ended a window in waiti");
    assert!(
        m.parked(1),
        "and is parked at the reply: {:?}",
        m.core_report()
    );
    let pc = m.harts[1].pc();
    let sym = m.symbolize(pc).unwrap_or_default();
    assert!(
        sym.contains("idle_once"),
        "the park is the pusher's idle: pc={pc:#010x} ({sym})"
    );
}
