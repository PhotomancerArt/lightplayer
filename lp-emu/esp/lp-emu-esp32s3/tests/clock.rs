//! The S3's clock and its watchdog, through the machine: the RWDT fires
//! when it is not fed and does not when it is, a software interrupt raised
//! through `SYSTEM.cpu_intr_from_cpu[n]` survives a subsequent MMIO store
//! (the X43 regression, on this chip, at a real quantum), `SYSTIMER`'s
//! microsecond derivation, `TIMG`'s two counters and no LACT, and the three
//! inputs of the hold on core 1.
//!
//! Every guest program here is assembled with `lp_xt_inst::encode` — never a
//! hex literal (M0's `rev8` lesson) — and placed in the SRAM1 **I-bus** view,
//! because a windowed call cannot cross a 1 GiB region (`tests/boot.rs`, DD83)
//! and the mask ROM's own vectors are what dispatch the interrupts these
//! tests take: `_Level2Vector` and `_Level3Vector` are `xsr.excsaveN a2; jx
//! a2`, so a test installs its handler by writing `EXCSAVEN`.
//!
//! ⚠️ **Never single-step an interrupt test.** 1-cycle stepping re-feeds the
//! matrix every instruction and hides exactly the class of bug the X43 test
//! exists for; every machine here runs at the default 256-cycle quantum.

use lp_emu_esp32s3::intmatrix::{ESP_HAL_DISABLED_CPU_INTERRUPT, INTR_MAP};
use lp_emu_esp32s3::loader::EfuseIdentity;
use lp_emu_esp32s3::machine::{BootFrame, Esp32S3Builder, Machine, Outcome, StopCondition};
use lp_emu_esp32s3::periph::rtc_cntl::{
    OPTIONS0, STALLED, SW_CPU_STALL, WDTCONFIG0, WDTCONFIG1, WDTFEED, WDTWPROTECT,
};
use lp_emu_esp32s3::periph::system::{CORE_1_CONTROL_0, CPU_INTR_FROM_CPU0};
use lp_emu_esp32s3::periph::systimer::{CYCLES_PER_TICK, UNIT0_OP, UNIT0_VALUE_HI, UNIT0_VALUE_LO};
use lp_emu_esp32s3::periph::{RC_SLOW_HZ, WDT_WKEY, timg};
use lp_emu_esp32s3::{memmap, regs};
use lp_xt_inst::{AluRrr, Inst, LoadOp, NullaryOp, Reg, SpecialReg, SrOp, StoreOp};

/// The literal pool: `l32r` reaches backwards only, so it sits below the
/// code.
const LIT: u32 = 0x4037_9000;
/// The program.
const CODE: u32 = 0x4037_9100;
/// The interrupt handler.
const HANDLER: u32 = 0x4037_9200;
/// Data words, on the data side: a flag, a counter, two timestamps.
const FLAG: u32 = 0x3FC9_4000;
const COUNT: u32 = 0x3FC9_4004;
const T_BEFORE: u32 = 0x3FC9_4008;
const T_HANDLER: u32 = 0x3FC9_400c;

/// The CPU interrupt the X43 test routes `FROM_CPU_INTR0` to: 19 is a
/// **level-2** external line (`machine::CORE_INTERRUPTS`), so the take
/// lands in `_Level2Vector` with `EPC2` naming the interrupted instruction —
/// unambiguous, where a level-1 line would share the user vector with every
/// exception.
const SWI_CPU_INT: u32 = 19;
/// `FROM_CPU_INTR0`.
const SWI_SOURCE: u16 = regs::source::FROM_CPU_INTR0;
/// CPU interrupt 15 is `CCOMPARE1`'s, level 3 (`CORE_INTERRUPTS[15]`), which
/// the fed-watchdog test uses as its one-second pacer.
const TIMER1_CPU_INT: u32 = 15;

/// `RwdtStageAction::ResetSystem` in stage 0, both reset lengths 7,
/// `pause_in_slp`, `wdt_en` — the word esp-hal's `enable()` leaves in
/// `wdtconfig0` (`rtc_cntl/mod.rs:566-596`).
const RWDT_ENABLE: u32 = (1 << 31) | (4 << 28) | (7 << 16) | (7 << 13) | (1 << 9);
/// `us_to_rtc_ticks(30 s) >> 1` at 136 kHz — the firmware's boot timeout.
const HOLD_30S: u32 = (30 * RC_SLOW_HZ / 2) as u32;
/// The tightened 8 s runtime timeout.
const HOLD_8S: u32 = (8 * RC_SLOW_HZ / 2) as u32;
/// `wdtfeed.wdt_feed`.
const FEED_BIT: u32 = 1 << 31;

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

/// A builder for programs whose `l32r`s need their own pc: each pushed
/// instruction knows where it lands.
struct Asm {
    at: u32,
    insts: Vec<Inst>,
}

impl Asm {
    fn new(at: u32) -> Self {
        Self {
            at,
            insts: Vec::new(),
        }
    }

    fn pc(&self) -> u32 {
        self.at
            + self
                .insts
                .iter()
                .map(|i| lp_xt_inst::encode(i).len() as u32)
                .sum::<u32>()
    }

    fn push(&mut self, inst: Inst) -> &mut Self {
        self.insts.push(inst);
        self
    }

    fn l32r(&mut self, r: Reg, label: u32) -> &mut Self {
        let field = l32r_field(self.pc(), label);
        self.push(Inst::L32r(r, field))
    }

    /// `j .` — the pc of the jump itself.
    fn park(&mut self) -> u32 {
        let pc = self.pc();
        self.push(Inst::J(-4));
        pc
    }

    fn done(&self) -> Vec<(u32, Inst)> {
        assemble(self.at, &self.insts)
    }
}

/// A strict machine with no application, `words` placed at the literal
/// pool, `program` and `handler` placed, core 0 pointed at the program with
/// the ROM's own stack.
fn fixture(words: &[u32], program: &[(u32, Inst)], handler: &[(u32, Inst)]) -> Machine {
    let mut machine = Esp32S3Builder::new()
        .strict(true)
        .build()
        .expect("a machine with no app still has a bus and a ROM");
    let bus = machine.bus_mut();
    for (i, w) in words.iter().enumerate() {
        bus.load_image(LIT + 4 * i as u32, &w.to_le_bytes())
            .expect("the pool");
    }
    for (at, inst) in program.iter().chain(handler) {
        bus.load_image(*at, &lp_xt_inst::encode(inst))
            .expect("the code");
    }
    for at in [FLAG, COUNT, T_BEFORE, T_HANDLER] {
        bus.load_image(at, &0u32.to_le_bytes()).expect("the data");
    }
    machine
        .seed_boot_state(CODE, BootFrame::rom_pro_stack(machine.rom()))
        .expect("seeded");
    machine
}

fn word(m: &mut Machine, at: u32) -> u32 {
    m.peek_word(at).expect("mapped")
}

// ---------------------------------------------------------------------------
// The RWDT, both directions
// ---------------------------------------------------------------------------

/// The feeder's arm — `set_timeout(Stage0, hold)` then `enable()`,
/// register for register — followed by a park with interrupts off, so the
/// idle skip carries the run straight to whatever the scheduler holds.
fn arm_program(hold: u32) -> (Vec<u32>, Vec<(u32, Inst)>) {
    let (a2, a3, a4, a5) = (reg(2), reg(3), reg(4), reg(5));
    // pool: &RTC_CNTL, WDT_WKEY, hold, RWDT_ENABLE
    let words = vec![memmap::periph::RTC_CNTL, WDT_WKEY, hold, RWDT_ENABLE];
    let mut p = Asm::new(CODE);
    p.l32r(a2, LIT);
    p.l32r(a3, LIT + 4);
    // set_timeout: unlock, hold, lock.
    p.push(Inst::Store(StoreOp::S32i, a3, a2, WDTWPROTECT));
    p.l32r(a4, LIT + 8);
    p.push(Inst::Store(StoreOp::S32i, a4, a2, WDTCONFIG1));
    p.push(Inst::MoviN(a5, 0));
    p.push(Inst::Store(StoreOp::S32i, a5, a2, WDTWPROTECT));
    // enable: unlock, 0, en|pause, the full word, lock.
    p.push(Inst::Store(StoreOp::S32i, a3, a2, WDTWPROTECT));
    p.push(Inst::Store(StoreOp::S32i, a5, a2, WDTCONFIG0));
    p.l32r(a4, LIT + 12);
    p.push(Inst::Store(StoreOp::S32i, a4, a2, WDTCONFIG0));
    p.push(Inst::Store(StoreOp::S32i, a5, a2, WDTWPROTECT));
    p.push(Inst::Nullary(NullaryOp::Memw));
    (words, p.done())
}

/// **The RWDT fires**: armed for 30 s and never fed, the run ends in a
/// reset at exactly 30 s of emulated time after the arm.
#[test]
fn the_rwdt_fires_thirty_seconds_after_an_unfed_arm() {
    let (words, mut program) = arm_program(HOLD_30S);
    // Park: `waiti 0` with INTENABLE 0 never wakes, so the deterministic
    // idle skip jumps to the RWDT's event.
    let park_pc = {
        let mut p = Asm::new(
            program
                .last()
                .map(|(pc, i)| pc + lp_xt_inst::encode(i).len() as u32)
                .unwrap(),
        );
        p.push(Inst::Waiti(0));
        let pc = p.park();
        program.extend(p.done());
        pc
    };
    let mut m = fixture(&words, &program, &[]);
    let arm_cycle_upper = program.len() as u64 + 2;

    let outcome = m.run_until(&StopCondition::after_micros(40_000_000));
    let Outcome::Reset { cycle, source, .. } = outcome else {
        panic!("expected the RWDT to reset the chip, got {outcome:?}");
    };
    assert_eq!(source, "RTC_CNTL RWDT stage 0 (ResetSystem)");
    assert_eq!(
        outcome.exit_code(),
        2,
        "the classic's code for a reset the machine reports"
    );
    // Exactly 30 s after the `wdtconfig0` store, which is within the first
    // few dozen cycles of the run.
    let thirty_s = 30 * memmap::CPU_HZ;
    assert!(
        cycle >= thirty_s && cycle <= thirty_s + arm_cycle_upper,
        "expired at cycle {cycle}, not 30 s ({thirty_s}) after an arm in the first \
         {arm_cycle_upper} cycles"
    );
    assert!(
        m.idle_skips() > 0,
        "the run got there by idle skips, not by spinning"
    );
    assert!(
        m.parked(0) && m.harts[0].pc() == park_pc,
        "core 0 was parked at its waiti when the watchdog bit"
    );
    println!(
        "RWDT FIRES: armed for 30 s, never fed, reset at cycle {cycle} ({} us emulated), {} \
         instructions run, {} idle skips",
        cycle / memmap::CYCLES_PER_US,
        m.instructions(),
        m.idle_skips()
    );
}

/// **A fed watchdog does not fire**: armed for 8 s and fed once a second by
/// a `CCOMPARE1` handler for forty seconds, the run reaches its deadline
/// with no reset; then the feeds stop and it bites 8 s later — the
/// firmware's deliberate withholding, visible.
#[test]
fn a_fed_rwdt_does_not_fire_and_a_withheld_feed_does() {
    let (mut words, mut program) = arm_program(HOLD_8S);
    // pool[4..]: HANDLER, one second, 1 << 15, FEED_BIT, &COUNT, &T_BEFORE
    let one_second = memmap::CPU_HZ as u32;
    words.extend([
        HANDLER,
        one_second,
        1 << TIMER1_CPU_INT,
        FEED_BIT,
        COUNT,
        T_BEFORE,
    ]);
    let (a2, a3, a5, a6, a7, a8, a9, a10) = (
        reg(2),
        reg(3),
        reg(5),
        reg(6),
        reg(7),
        reg(8),
        reg(9),
        reg(10),
    );
    // After the arm: the handler into EXCSAVE3, CCOMPARE1 = CCOUNT + 1 s,
    // INTENABLE = 1 << 15, park in `waiti`.
    let mut p = Asm::new(
        program
            .last()
            .map(|(pc, i)| pc + lp_xt_inst::encode(i).len() as u32)
            .unwrap(),
    );
    p.l32r(a5, LIT + 16);
    p.push(Inst::Sr(SrOp::Wsr, SpecialReg::Excsave3, a5));
    p.push(Inst::Sr(SrOp::Rsr, SpecialReg::Ccount, a6));
    p.l32r(a7, LIT + 20);
    p.push(Inst::Rrr(AluRrr::Add, a6, a6, a7));
    p.push(Inst::Sr(SrOp::Wsr, SpecialReg::Ccompare1, a6));
    p.l32r(a8, LIT + 24);
    p.push(Inst::Sr(SrOp::Wsr, SpecialReg::Intenable, a8));
    p.push(Inst::Nullary(NullaryOp::Rsync));
    let loop_pc = p.pc();
    p.push(Inst::Waiti(0));
    p.push(Inst::J(loop_pc as i32 - (p.pc() as i32 + 4)));
    program.extend(p.done());

    // The handler. `_Level3Vector` is `xsr.excsave3 a2; jx a2`, so on entry
    // a2 holds the handler's address and EXCSAVE3 the caller's a2; a second
    // `xsr` puts both back — a2 = &RTC_CNTL again, EXCSAVE3 = the handler
    // for the next take. Then: feed (unlock, feed, lock), CCOMPARE1 += 1 s,
    // COUNT += 1, rfi 3. `a3`/`a7` are the main program's and still hold the
    // key and one second — a level-3 interrupt rotates no window.
    let mut h = Asm::new(HANDLER);
    h.push(Inst::Sr(SrOp::Xsr, SpecialReg::Excsave3, a2));
    h.push(Inst::Store(StoreOp::S32i, a3, a2, WDTWPROTECT));
    h.l32r(a9, LIT + 28);
    h.push(Inst::Store(StoreOp::S32i, a9, a2, WDTFEED));
    h.push(Inst::MoviN(a10, 0));
    h.push(Inst::Store(StoreOp::S32i, a10, a2, WDTWPROTECT));
    h.push(Inst::Sr(SrOp::Rsr, SpecialReg::Ccompare1, a6));
    h.push(Inst::Rrr(AluRrr::Add, a6, a6, a7));
    h.push(Inst::Sr(SrOp::Wsr, SpecialReg::Ccompare1, a6));
    h.l32r(a9, LIT + 32);
    h.push(Inst::Load(LoadOp::L32i, a10, a9, 0));
    h.push(Inst::Addi(a10, a10, 1));
    h.push(Inst::Store(StoreOp::S32i, a10, a9, 0));
    h.push(Inst::Rfi(3));
    let handler = h.done();

    let mut m = fixture(&words, &program, &handler);
    // Forty seconds, fed every second: the deadline, not a reset.
    let outcome = m.run_until(&StopCondition::after_micros(40_000_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "fed every second, the RWDT must not fire in 40 s: {outcome:?}"
    );
    let feeds = word(&mut m, COUNT);
    assert!(
        (39..=40).contains(&feeds),
        "one feed per second for forty seconds, not {feeds}"
    );
    let epc3 = m.harts[0].sr().epc[3];
    assert!(
        (loop_pc..loop_pc + 8).contains(&epc3),
        "EPC3 = {epc3:#010x} names the parked loop at {loop_pc:#010x}"
    );

    // The io task goes silent: INTENABLE off, no more feeds. 8 s after the
    // last one the RWDT bites — deliberately.
    m.harts[0].interrupts_mut().intenable = 0;
    let last_feed_at = m.cycles();
    let outcome = m.run_until(&StopCondition::after_micros(60_000_000));
    let Outcome::Reset { cycle, source, .. } = outcome else {
        panic!("with the feeds withheld the RWDT must fire: {outcome:?}");
    };
    assert_eq!(source, "RTC_CNTL RWDT stage 0 (ResetSystem)");
    let eight_s = 8 * memmap::CPU_HZ;
    assert!(
        cycle > last_feed_at && cycle <= last_feed_at + eight_s,
        "bit at {cycle}, within 8 s ({eight_s}) of the last feed at {last_feed_at}"
    );
    assert_eq!(
        word(&mut m, COUNT),
        feeds,
        "no feed after the silence began"
    );
    println!(
        "RWDT FED: {feeds} feeds over 40 s, no reset; feeds withheld at cycle {last_feed_at}, \
         reset at cycle {cycle} ({} s later)",
        (cycle - last_feed_at) / memmap::CPU_HZ
    );
}

// ---------------------------------------------------------------------------
// X43 on this chip
// ---------------------------------------------------------------------------

/// The program the X43 test runs, with `FROM_CPU_INTR0` routed to
/// `cpu_int`: raise the source with `INTENABLE` still 0, perform **another
/// MMIO store**, then enable the line — and record `CCOUNT` just before
/// the enable so the handler's own timestamp says whether the take was
/// immediate or a window late.
fn x43_program(cpu_int: u32) -> (Vec<u32>, Vec<(u32, Inst)>, Vec<(u32, Inst)>, u32) {
    // pool: &SYSTEM, &INTERRUPT_CORE0, HANDLER, 1 << 19, &FLAG, &T_BEFORE, &T_HANDLER
    let words = vec![
        memmap::periph::SYSTEM,
        memmap::periph::INTERRUPT_CORE0,
        HANDLER,
        1 << SWI_CPU_INT,
        FLAG,
        T_BEFORE,
        T_HANDLER,
    ];
    let (a2, a3, a4, a5, a6, a7) = (reg(2), reg(3), reg(4), reg(5), reg(6), reg(7));
    let mut p = Asm::new(CODE);
    p.l32r(a2, LIT);
    p.l32r(a3, LIT + 4);
    // core_0_intr_map[FROM_CPU_INTR0] = cpu_int.
    p.push(Inst::MoviN(a4, cpu_int as i32));
    p.push(Inst::Store(
        StoreOp::S32i,
        a4,
        a3,
        INTR_MAP + 4 * u32::from(SWI_SOURCE),
    ));
    p.l32r(a5, LIT + 8);
    p.push(Inst::Sr(SrOp::Wsr, SpecialReg::Excsave2, a5));
    // Raise: `SoftwareInterrupt::raise`, with the line NOT yet enabled.
    p.push(Inst::MoviN(a4, 1));
    p.push(Inst::Store(StoreOp::S32i, a4, a2, CPU_INTR_FROM_CPU0));
    p.push(Inst::Load(LoadOp::L32i, a4, a2, CPU_INTR_FROM_CPU0));
    // ⚠️ The subsequent MMIO store — X43's trap. `core_1_control_1` is the
    // PAC's "R/W register, no function".
    p.push(Inst::MoviN(a4, 0x55));
    p.push(Inst::Store(StoreOp::S32i, a4, a2, CORE_1_CONTROL_0 + 4));
    p.push(Inst::Nullary(NullaryOp::Memw));
    // Timestamp, then enable the line: the take must follow at once.
    p.push(Inst::Sr(SrOp::Rsr, SpecialReg::Ccount, a6));
    p.l32r(a7, LIT + 20);
    p.push(Inst::Store(StoreOp::S32i, a6, a7, 0));
    p.l32r(a6, LIT + 12);
    let enable_pc = p.pc();
    p.push(Inst::Sr(SrOp::Wsr, SpecialReg::Intenable, a6));
    p.push(Inst::Nullary(NullaryOp::Rsync));
    p.park();
    let program = p.done();

    // The handler: `xsr.excsave2 a2` undoes the vector's swap (a2 = &SYSTEM
    // again, EXCSAVE2 = the handler), then timestamp, clear the source, set
    // the flag, rfi 2.
    let mut h = Asm::new(HANDLER);
    h.push(Inst::Sr(SrOp::Xsr, SpecialReg::Excsave2, a2));
    h.push(Inst::Sr(SrOp::Rsr, SpecialReg::Ccount, a6));
    h.l32r(a7, LIT + 24);
    h.push(Inst::Store(StoreOp::S32i, a6, a7, 0));
    h.push(Inst::MoviN(a4, 0));
    h.push(Inst::Store(StoreOp::S32i, a4, a2, CPU_INTR_FROM_CPU0));
    h.l32r(a7, LIT + 16);
    h.push(Inst::MoviN(a4, 1));
    h.push(Inst::Store(StoreOp::S32i, a4, a7, 0));
    h.push(Inst::Rfi(2));
    (words, program, h.done(), enable_pc)
}

/// **The X43 regression, on this chip.** A software interrupt raised
/// through `SYSTEM.cpu_intr_from_cpu0` survives a subsequent MMIO store:
/// the hart re-samples the bus after every store, and a matrix that
/// answered `cpu_interrupt` instead of the mask form would have zeroed the
/// asserted mask on the second store and dropped the line until the next
/// window boundary. The handler's timestamp is what tells the two apart —
/// which is why this runs at the default 256-cycle quantum and never
/// single-stepped.
#[test]
fn a_software_interrupt_survives_a_subsequent_mmio_store_x43() {
    let (words, program, handler, enable_pc) = x43_program(SWI_CPU_INT);
    let mut m = fixture(&words, &program, &handler);
    assert_eq!(
        m.core_quantum(),
        256,
        "a real quantum, not 1-cycle stepping"
    );

    let outcome = m.run_until(&StopCondition {
        stop_cycle: Some(20_000),
        ..Default::default()
    });
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the fixture parks in `j .`: {outcome:?} (first violation {:?})",
        m.first_strict_violation()
    );
    assert_eq!(
        word(&mut m, FLAG),
        1,
        "the handler ran: the interrupt was taken"
    );
    assert!(
        !m.bus().irq.level(SWI_SOURCE),
        "and cleared the source through the SYSTEM view"
    );
    let epc2 = m.harts[0].sr().epc[2];
    assert!(
        epc2 >= enable_pc && epc2 < enable_pc + 16,
        "EPC2 = {epc2:#010x} names the instruction the enable interrupted, at or just after \
         {enable_pc:#010x} — not the window boundary"
    );
    let before = word(&mut m, T_BEFORE);
    let taken = word(&mut m, T_HANDLER);
    assert!(
        taken > before && taken - before < 16,
        "the take followed the enable within a few instructions (CCOUNT {before} → {taken}); a \
         matrix that zeroed the mask on the second store would have delayed it to the next \
         window boundary, up to 256 cycles later"
    );
    assert_eq!(
        word(&mut m, CORE_1_CONTROL_0 + 4 + memmap::periph::SYSTEM),
        0x55,
        "the second store landed too"
    );
    println!(
        "X43: FROM_CPU_INTR0 -> CPU int {SWI_CPU_INT}, raised with INTENABLE=0, one more MMIO \
         store, enabled at pc {enable_pc:#010x} / CCOUNT {before}, taken at CCOUNT {taken}, \
         EPC2 {epc2:#010x}, quantum {}",
        m.core_quantum()
    );
}

/// The control: esp-hal's "disabled" value 16 routes to a real line the
/// program never enables, so the same sequence takes nothing — the routing
/// is what drove the take above, not the source level alone.
#[test]
fn the_esp_hal_disabled_value_is_a_line_the_hart_never_enables() {
    let (words, program, handler, _) = x43_program(ESP_HAL_DISABLED_CPU_INTERRUPT);
    let mut m = fixture(&words, &program, &handler);
    let outcome = m.run_until(&StopCondition {
        stop_cycle: Some(20_000),
        ..Default::default()
    });
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert_eq!(word(&mut m, FLAG), 0, "no handler ran");
    assert!(m.bus().irq.level(SWI_SOURCE), "the source is still high");
    m.bus_mut().set_hart(0);
    assert_eq!(
        m.bus().pending_cpu_interrupt_mask(),
        1 << ESP_HAL_DISABLED_CPU_INTERRUPT,
        "asserted on line 16, which INTENABLE (1 << 19) does not enable"
    );
}

// ---------------------------------------------------------------------------
// SYSTIMER and TIMG
// ---------------------------------------------------------------------------

/// **The microsecond pin.** `Instant::now()` on this chip is Unit0's count
/// through `ticks_to_us`, which `init_timestamp_scaler` turns into a shift
/// by 4 for 16 ticks/µs. One emulated millisecond — `CPU_HZ / 1000` guest
/// cycles at 240 MHz — must read back as 16 000 ticks and 1 000 µs.
#[test]
fn systimer_unit0_gives_esp_hal_one_thousand_microseconds_per_emulated_millisecond() {
    let mut p = Asm::new(CODE);
    p.park();
    let mut m = fixture(&[], &p.done(), &[]);
    let one_ms = memmap::CPU_HZ / 1_000;
    let outcome = m.run_until(&StopCondition {
        stop_cycle: Some(one_ms),
        ..Default::default()
    });
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert_eq!(m.cycles(), one_ms);
    // esp-hal's `unit_value(Unit0)`: update, then lo, hi.
    let base = memmap::periph::SYSTIMER;
    let now = m.cycles();
    m.bus_mut().set_time(now);
    assert!(m.poke_word(base + UNIT0_OP, 1 << 30));
    let lo = word(&mut m, base + UNIT0_VALUE_LO);
    let hi = word(&mut m, base + UNIT0_VALUE_HI);
    let ticks = (u64::from(hi) << 32) | u64::from(lo);
    assert_eq!(CYCLES_PER_TICK, 15, "240 MHz over XTAL/2.5 = 16 MHz");
    assert_eq!(ticks, 16_000, "16 MHz for one millisecond");
    assert_eq!(
        ticks >> 4,
        1_000,
        "esp-hal's shift by log2(16): microseconds"
    );
    println!(
        "SYSTIMER: {one_ms} cycles = {ticks} ticks = {} us (CYCLES_PER_TICK {CYCLES_PER_TICK})",
        ticks >> 4
    );
}

/// **No LACT.** The S3's timer groups have two counters and nothing at the
/// classic's LACT offsets but the interrupt registers; the absence is
/// asserted against the generated table, not assumed.
#[test]
fn timg_has_two_counters_and_no_lact() {
    assert_eq!(timg::COUNTERS, 2);
    assert_eq!(regs::TIMG0.name(0x000), Some("t0.config"));
    assert_eq!(regs::TIMG0.name(0x024), Some("t1.config"));
    assert_eq!(
        regs::TIMG0.name(0x048),
        Some("wdtconfig0"),
        "the WDT follows the second counter"
    );
    // The classic's LACT block is `+0x70..+0x94` with `int_ena` pushed to
    // `+0x98`; here `+0x70` IS `int_ena` and nothing in the table is LACT.
    assert_eq!(regs::TIMG0.name(0x070), Some("int_ena"));
    assert_eq!(regs::TIMG0.name(0x074), Some("int_raw"));
    assert_eq!(regs::TIMG0.name(0x078), Some("int_st"));
    assert_eq!(regs::TIMG0.name(0x07c), Some("int_clr"));
    assert_eq!(regs::TIMG0.name(0x080), Some("rtccalicfg2"));
    assert_eq!(
        regs::TIMG0.name(0x098),
        None,
        "the classic's int_ena offset names nothing"
    );
    assert!(
        regs::TIMG0
            .entries
            .iter()
            .all(|(_, n)| !n.to_ascii_lowercase().contains("lact")),
        "no LACT register in the S3's TIMG"
    );
    // And through the machine: T1 is its own counter at +0x24.
    let mut p = Asm::new(CODE);
    p.park();
    let mut m = fixture(&[], &p.done(), &[]);
    let base = memmap::periph::TIMG0;
    m.bus_mut().set_time(0);
    // Both counters on XTAL, T0 at divider 2 (20 MHz), T1 at 40 (1 MHz).
    let cfg = |div: u32| (div << 13) | (1 << 30) | (1 << 31) | (1 << 9);
    assert!(m.poke_word(base + 0x000, cfg(2)));
    assert!(m.poke_word(base + 0x024, cfg(40)));
    let one_ms = memmap::CPU_HZ / 1_000;
    m.run_until(&StopCondition {
        stop_cycle: Some(one_ms),
        ..Default::default()
    });
    m.bus_mut().set_time(one_ms);
    assert!(m.poke_word(base + 0x00c, 1 << 31));
    assert!(m.poke_word(base + 0x030, 1 << 31));
    assert_eq!(word(&mut m, base + 0x004), 20_000, "T0: 20 MHz for 1 ms");
    assert_eq!(word(&mut m, base + 0x028), 1_000, "T1: 1 MHz for 1 ms");
}

// ---------------------------------------------------------------------------
// The hold on core 1, the reset cause and the identity, through the bus
// ---------------------------------------------------------------------------

/// A guest write to `SYSTEM.core_1_control_0` changes what the machine
/// reports about the hold — the register P03 wired and P04 opened.
#[test]
fn a_guest_write_to_core_1_control_0_changes_the_hold_report() {
    let mut p = Asm::new(CODE);
    p.park();
    let mut m = fixture(&[], &p.done(), &[]);
    let reg = memmap::periph::SYSTEM + CORE_1_CONTROL_0;
    assert_eq!(
        word(&mut m, reg),
        0x04,
        "the PAC's reset, read through the view"
    );
    let inputs = m.stall_inputs(1);
    assert!(inputs.iter().any(|i| i.contains("reseting")));
    assert!(inputs.iter().any(|i| i.contains("!clkgate_en")));
    assert!(m.core_stalled(1));
    // `start_core1`'s end state: clkgate_en set, runstall and reseting
    // clear.
    assert!(m.poke_word(reg, 0b010));
    assert!(!m.core_1_control().holds_core1());
    let inputs = m.stall_inputs(1);
    assert!(
        !inputs.iter().any(|i| i.starts_with("SYSTEM")),
        "the register no longer holds it: {inputs:?}"
    );
    assert!(
        m.core_stalled(1),
        "but the machine still does (M6 releases no core)"
    );
    assert_eq!(inputs.len(), 1, "{inputs:?}");
    // `runstall` holds it again.
    assert!(m.poke_word(reg, 0b011));
    assert!(m.stall_inputs(1).iter().any(|i| i.contains("runstall")));
}

/// The third input: RTC_CNTL's two-register stall key reaches the machine.
#[test]
fn the_stall_key_reaches_the_machine_from_rtc_cntl() {
    let mut p = Asm::new(CODE);
    p.park();
    let mut m = fixture(&[], &p.done(), &[]);
    assert!(!m.core_stalled(0));
    assert!(!m.stall_key().stalled(0));
    // `internal_park_core(ProCpu)`: c1 = 0x21, then c0 = 0x02.
    let rtc = memmap::periph::RTC_CNTL;
    let c1 = word(&mut m, rtc + SW_CPU_STALL);
    assert!(m.poke_word(rtc + SW_CPU_STALL, c1 | (0x21 << 26)));
    assert!(!m.core_stalled(0), "one half is not the key");
    let o0 = word(&mut m, rtc + OPTIONS0);
    assert!(m.poke_word(rtc + OPTIONS0, o0 | (0x02 << 2)));
    assert!(m.core_stalled(0), "both halves: the machine sees it");
    assert_eq!(m.stall_key().key(0) & 0xff, STALLED);
    assert!(m.stall_inputs(0).iter().any(|i| i.contains("0x86")));
    // And the reset cause the ROM reads.
    let rs = word(&mut m, rtc + 0x038);
    assert_eq!(rs & 0x3f, 1, "POWERON_RESET for the PRO core");
    assert_eq!((rs >> 6) & 0x3f, 1, "and the APP core");
}

/// `--efuse-mac` / `--efuse-rev` reach the eFuse words esp-hal reads, in
/// the S3's own split; the default is the PAC's zero.
#[test]
fn the_identity_reaches_the_efuse_block() {
    let mut p = Asm::new(CODE);
    p.park();
    let mut m = fixture(&[], &p.done(), &[]);
    let e = memmap::periph::EFUSE;
    assert_eq!(word(&mut m, e + 0x44), 0, "no dump: the PAC's zero");
    assert_eq!(m.efuse(), EfuseIdentity::default());

    let mut other = Esp32S3Builder::new()
        .efuse(EfuseIdentity {
            mac: [0x11, 0x22, 0x33, 0x44, 0x55, 0x66],
            wafer_major: 1,
            wafer_minor: 9,
        })
        .build()
        .expect("builds");
    assert_eq!(other.peek_word(e + 0x44), Some(0x3344_5566));
    assert_eq!(other.peek_word(e + 0x48), Some(0x0000_1122));
    assert_eq!(
        other.peek_word(e + 0x50),
        Some(1 << 18),
        "minor & 7 at bit 114"
    );
    assert_eq!(
        other.peek_word(e + 0x58),
        Some((1 << 23) | (1 << 24)),
        "minor >> 3 at bit 183, major at 184 — the S3's own word, not the C6's"
    );
}
